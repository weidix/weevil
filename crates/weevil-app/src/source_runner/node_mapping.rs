use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::nfo::{Actor, Movie};

mod csv;
mod loader;

use self::csv::{
    CsvOrder, collect_from_values_line, collect_from_values_offset, has_content, hash_key,
    normalize_key, normalize_optional_key, parse_csv_line, resolve_csv_order,
};
use self::loader::load_csv_reader;

#[derive(Debug, Clone)]
pub(crate) struct NodeValueMapper {
    data: NodeValueMapperData,
}

#[derive(Debug, Clone)]
enum NodeValueMapperData {
    InMemory {
        rules: HashMap<String, HashMap<String, String>>,
    },
    Indexed(IndexedNodeValueMapper),
}

#[derive(Debug, Clone)]
struct IndexedNodeValueMapper {
    index: Vec<IndexedEntry>,
    file: Arc<Mutex<File>>,
    order: CsvOrder,
}

type IndexedEntry = (u64, u64);

impl Default for NodeValueMapper {
    fn default() -> Self {
        Self {
            data: NodeValueMapperData::InMemory {
                rules: HashMap::new(),
            },
        }
    }
}

impl NodeValueMapper {
    #[cfg(test)]
    pub(crate) fn from_csv(content: &str) -> Result<Self, String> {
        let mut mapper = Self::default();
        let mut order = None;

        for (index, raw_line) in content.lines().enumerate() {
            let line_no = index + 1;
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let columns = parse_csv_line(raw_line)
                .map_err(|reason| format!("invalid CSV at line {line_no}: {reason}"))?;
            if columns.len() < 3 {
                let len = columns.len();
                return Err(format!(
                    "invalid CSV at line {line_no}: expected at least 3 columns (node, to, from...), got {len}"
                ));
            }

            let node = columns[0].trim();
            let col2 = columns[1].trim();
            let col3 = columns[2].trim();

            if resolve_csv_order(&mut order, node, col2, col3) {
                continue;
            }

            let order = order.unwrap_or(CsvOrder::NodeToFrom);
            let (node, to, from_columns) = match order {
                CsvOrder::NodeToFrom => (node, col2, &columns[2..]),
                CsvOrder::NodeFromTo => {
                    if columns.len() != 3 {
                        let len = columns.len();
                        return Err(format!(
                            "invalid CSV at line {line_no}: expected 3 columns (node, from, to), got {len}"
                        ));
                    }
                    (node, col3, &columns[1..2])
                }
            };

            if node.is_empty() {
                return Err(format!(
                    "invalid CSV at line {line_no}: node/from must be non-empty"
                ));
            }

            let from_values = collect_from_values_line(from_columns, line_no)?;
            for from in from_values {
                mapper.insert_rule(node, from.as_str(), to);
            }
        }

        Ok(mapper)
    }

    pub(crate) fn from_csv_file(file: File) -> Result<Self, String> {
        let mut reader = BufReader::new(file);
        let mut index: Vec<IndexedEntry> = Vec::new();
        let mut order = None;
        let mut line_no = 0usize;
        let mut offset = 0u64;
        let mut line = String::new();

        loop {
            line.clear();
            let next_line = line_no + 1;
            let bytes = reader
                .read_line(&mut line)
                .map_err(|source| format!("failed to read CSV at line {next_line}: {source}"))?;
            if bytes == 0 {
                break;
            }
            line_no += 1;
            let line_offset = offset;
            let bytes_u64 = u64::try_from(bytes)
                .map_err(|_| format!("CSV line {line_no} size {bytes} exceeds u64::MAX"))?;
            offset = offset
                .checked_add(bytes_u64)
                .ok_or_else(|| format!("CSV offset overflow at line {line_no}"))?;

            let raw_line = line.trim_end_matches(&['\n', '\r'][..]);
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let columns = parse_csv_line(raw_line)
                .map_err(|reason| format!("invalid CSV at line {line_no}: {reason}"))?;
            if columns.len() < 3 {
                let len = columns.len();
                return Err(format!(
                    "invalid CSV at line {line_no}: expected at least 3 columns (node, to, from...), got {len}"
                ));
            }

            let node = columns[0].trim();
            let col2 = columns[1].trim();
            let col3 = columns[2].trim();

            if resolve_csv_order(&mut order, node, col2, col3) {
                continue;
            }

            let order = order.unwrap_or(CsvOrder::NodeToFrom);
            let (node, _to, from_columns) = match order {
                CsvOrder::NodeToFrom => (node, col2, &columns[2..]),
                CsvOrder::NodeFromTo => {
                    if columns.len() != 3 {
                        let len = columns.len();
                        return Err(format!(
                            "invalid CSV at line {line_no}: expected 3 columns (node, from, to), got {len}"
                        ));
                    }
                    (node, col3, &columns[1..2])
                }
            };

            if node.is_empty() {
                return Err(format!(
                    "invalid CSV at line {line_no}: node/from must be non-empty"
                ));
            }

            let from_values = collect_from_values_line(from_columns, line_no)?;
            let node_key = normalize_key(node);
            let mut seen_from = HashSet::new();
            for from in from_values {
                let from_key = normalize_key(from.as_str());
                let key_hash = hash_key(node_key.as_str(), from_key.as_str());
                if seen_from.insert(from_key) {
                    index.push((key_hash, line_offset));
                }
            }
        }

        index.sort_by_key(|(hash, _)| *hash);
        let file = reader.into_inner();
        let order = order.unwrap_or(CsvOrder::NodeToFrom);
        Ok(Self {
            data: NodeValueMapperData::Indexed(IndexedNodeValueMapper {
                index,
                file: Arc::new(Mutex::new(file)),
                order,
            }),
        })
    }

    pub(crate) fn from_csv_files(paths: &[PathBuf]) -> Result<Self, String> {
        let mut mapper = Self::default();
        for path in paths {
            let file = File::open(path)
                .map_err(|source| format!("failed to read node mapping CSV {path:?}: {source}"))?;
            let mut reader = BufReader::new(file);
            load_csv_reader(&mut mapper, &mut reader)
                .map_err(|reason| format!("failed to parse node mapping CSV {path:?}: {reason}"))?;
        }
        Ok(mapper)
    }

    pub(crate) fn has_rules(&self) -> bool {
        match &self.data {
            NodeValueMapperData::InMemory { rules } => !rules.is_empty(),
            NodeValueMapperData::Indexed(indexed) => !indexed.index.is_empty(),
        }
    }

    pub(crate) fn apply_movie(&self, movie: &mut Movie) {
        if !self.has_rules() {
            return;
        }

        map_option_value(self, "title", &mut movie.title);
        map_option_value(self, "originaltitle", &mut movie.originaltitle);
        map_option_value(self, "sorttitle", &mut movie.sorttitle);
        map_option_value(self, "premiered", &mut movie.premiered);
        map_option_value(self, "director", &mut movie.director);
        map_option_value(self, "plot", &mut movie.plot);
        map_option_value(self, "outline", &mut movie.outline);
        map_option_value(self, "tagline", &mut movie.tagline);
        map_option_value(self, "studio", &mut movie.studio);
        map_option_value(self, "trailer", &mut movie.trailer);
        map_option_value(self, "fileinfo", &mut movie.fileinfo);
        map_option_value(self, "dateadded", &mut movie.dateadded);

        map_list_values(self, "credits", &mut movie.credits);
        map_list_values(self, "genre", &mut movie.genre);
        map_list_values(self, "tag", &mut movie.tag);
        map_list_values(self, "country", &mut movie.country);

        if let Some(set_info) = movie.set_info.as_mut() {
            map_option_value(self, "set.name", &mut set_info.name);
            map_option_value(self, "set.overview", &mut set_info.overview);
        }

        map_actor_values(self, &mut movie.actor);
    }

    fn insert_rule(&mut self, node: &str, from: &str, to: &str) {
        let NodeValueMapperData::InMemory { rules } = &mut self.data else {
            unreachable!("node value mapper must be in-memory to insert rules");
        };
        let node_key = normalize_key(node);
        let from_key = normalize_key(from);
        let target = to.trim().to_string();

        rules.entry(node_key).or_default().insert(from_key, target);
    }

    fn map_with_node(&self, node: &str, value: &str) -> Option<String> {
        let node_key = normalize_key(node);
        let value_key = normalize_key(value);

        match &self.data {
            NodeValueMapperData::InMemory { rules } => rules
                .get(node_key.as_str())
                .and_then(|node_rules| node_rules.get(value_key.as_str()))
                .cloned(),
            NodeValueMapperData::Indexed(indexed) => {
                indexed.map_value(node_key.as_str(), value_key.as_str())
            }
        }
    }

    fn map_value(&self, node: &str, value: &str) -> String {
        self.map_with_node(node, value)
            .unwrap_or_else(|| value.trim().to_string())
    }

    fn map_value_by_nodes(&self, nodes: &[&str], value: &str) -> String {
        for node in nodes {
            if let Some(mapped) = self.map_with_node(node, value) {
                return mapped;
            }
        }
        value.trim().to_string()
    }
}

fn map_option_value(mapper: &NodeValueMapper, node: &str, value: &mut Option<String>) {
    let Some(current) = value.take() else {
        return;
    };

    let mapped = mapper.map_value(node, current.as_str());
    if mapped.is_empty() {
        return;
    }

    *value = Some(mapped);
}

fn map_list_values(mapper: &NodeValueMapper, node: &str, values: &mut Vec<String>) {
    let mut mapped = Vec::with_capacity(values.len());
    let mut seen = HashSet::new();

    for value in std::mem::take(values) {
        let final_value = mapper.map_value(node, value.as_str());
        if final_value.is_empty() {
            continue;
        }

        let key = normalize_key(final_value.as_str());
        if seen.insert(key) {
            mapped.push(final_value);
        }
    }

    *values = mapped;
}

fn map_actor_values(mapper: &NodeValueMapper, actors: &mut Vec<Actor>) {
    let mut mapped = Vec::with_capacity(actors.len());

    for mut actor in std::mem::take(actors) {
        map_actor_option_value(mapper, &["actor", "actor.name"], &mut actor.name);
        map_actor_option_value(mapper, &["actor.role"], &mut actor.role);
        map_actor_option_value(mapper, &["actor.gender"], &mut actor.gender);

        if let Some(existing) = find_actor_by_name_mut(&mut mapped, actor.name.as_deref()) {
            merge_actor(existing, actor);
            continue;
        }

        mapped.push(actor);
    }

    for (index, actor) in mapped.iter_mut().enumerate() {
        actor.order = Some(u32::try_from(index + 1).unwrap_or(u32::MAX));
    }

    *actors = mapped;
}

fn map_actor_option_value(mapper: &NodeValueMapper, nodes: &[&str], value: &mut Option<String>) {
    let Some(current) = value.take() else {
        return;
    };

    let mapped = mapper.map_value_by_nodes(nodes, current.as_str());
    if mapped.is_empty() {
        return;
    }

    *value = Some(mapped);
}

fn find_actor_by_name_mut<'a>(
    actors: &'a mut [Actor],
    name: Option<&str>,
) -> Option<&'a mut Actor> {
    let incoming = normalize_optional_key(name);
    let Some(incoming_name) = incoming else {
        return None;
    };

    for actor in actors {
        if normalize_optional_key(actor.name.as_deref()) == Some(incoming_name.clone()) {
            return Some(actor);
        }
    }

    None
}

fn merge_actor(target: &mut Actor, incoming: Actor) {
    merge_option_string(&mut target.name, incoming.name);
    merge_option_string(&mut target.role, incoming.role);
    merge_option_string(&mut target.gender, incoming.gender);
    if target.order.is_none() {
        target.order = incoming.order;
    }
}

fn merge_option_string(target: &mut Option<String>, incoming: Option<String>) {
    if target.as_deref().is_some_and(has_content) {
        return;
    }

    if let Some(value) = incoming {
        let trimmed = value.trim();
        if has_content(trimmed) {
            *target = Some(trimmed.to_string());
        }
    }
}

impl IndexedNodeValueMapper {
    fn map_value(&self, node_key: &str, value_key: &str) -> Option<String> {
        let key_hash = hash_key(node_key, value_key);
        let entries = self.index.as_slice();
        let pos = entries
            .binary_search_by_key(&key_hash, |(hash, _)| *hash)
            .ok()?;
        let mut start = pos;
        while start > 0 && entries[start - 1].0 == key_hash {
            start -= 1;
        }
        let mut end = pos + 1;
        while end < entries.len() && entries[end].0 == key_hash {
            end += 1;
        }
        for (_, offset) in entries[start..end].iter().rev() {
            match self.read_value_at(*offset, node_key, value_key) {
                Ok(Some(mapped)) => return Some(mapped),
                Ok(None) => {}
                Err(reason) => panic!("node mapping CSV lookup failed: {reason}"),
            }
        }
        None
    }

    fn read_value_at(
        &self,
        offset: u64,
        node_key: &str,
        value_key: &str,
    ) -> Result<Option<String>, String> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| "node mapping CSV lock poisoned".to_string())?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|source| format!("failed to seek CSV to offset {offset}: {source}"))?;

        let mut reader = BufReader::new(&mut *file);
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|source| format!("failed to read CSV at offset {offset}: {source}"))?;
        if bytes == 0 {
            return Ok(None);
        }

        let raw_line = line.trim_end_matches(&['\n', '\r'][..]);
        let columns = parse_csv_line(raw_line)
            .map_err(|reason| format!("invalid CSV at offset {offset}: {reason}"))?;
        if columns.len() < 3 {
            let len = columns.len();
            return Err(format!(
                "invalid CSV at offset {offset}: expected at least 3 columns (node, to, from...), got {len}"
            ));
        }

        let node = columns[0].trim();
        let col2 = columns[1].trim();
        let col3 = columns[2].trim();

        let (node, to, from_columns) = match self.order {
            CsvOrder::NodeToFrom => (node, col2, &columns[2..]),
            CsvOrder::NodeFromTo => {
                if columns.len() != 3 {
                    let len = columns.len();
                    return Err(format!(
                        "invalid CSV at offset {offset}: expected 3 columns (node, from, to), got {len}"
                    ));
                }
                (node, col3, &columns[1..2])
            }
        };

        if node.is_empty() {
            return Err(format!(
                "invalid CSV at offset {offset}: node/from must be non-empty"
            ));
        }

        let from_values = collect_from_values_offset(from_columns, offset)?;
        if normalize_key(node) == node_key
            && from_values
                .iter()
                .any(|from| normalize_key(from.as_str()) == value_key)
        {
            return Ok(Some(to.trim().to_string()));
        }

        Ok(None)
    }
}

#[cfg(test)]
#[path = "../tests/node_mapping.rs"]
mod tests;
