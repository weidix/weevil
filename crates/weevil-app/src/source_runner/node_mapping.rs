use std::collections::{HashMap, HashSet};

use crate::nfo::{Actor, Movie};

#[derive(Debug, Clone, Default)]
pub(crate) struct NodeValueMapper {
    rules: HashMap<String, HashMap<String, String>>,
}

impl NodeValueMapper {
    pub(crate) fn from_csv(content: &str) -> Result<Self, String> {
        let mut mapper = Self::default();
        let mut first_data_row = true;

        for (index, raw_line) in content.lines().enumerate() {
            let line_no = index + 1;
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let columns = parse_csv_line(raw_line)
                .map_err(|reason| format!("invalid CSV at line {line_no}: {reason}"))?;
            if columns.len() != 3 {
                return Err(format!(
                    "invalid CSV at line {line_no}: expected 3 columns (node, from, to), got {}",
                    columns.len()
                ));
            }

            let node = columns[0].trim();
            let from = columns[1].trim();
            let to = columns[2].trim();

            if first_data_row
                && node.eq_ignore_ascii_case("node")
                && from.eq_ignore_ascii_case("from")
                && to.eq_ignore_ascii_case("to")
            {
                first_data_row = false;
                continue;
            }

            first_data_row = false;

            if node.is_empty() || from.is_empty() || to.is_empty() {
                return Err(format!(
                    "invalid CSV at line {line_no}: node/from/to must be non-empty"
                ));
            }

            mapper.insert_rule(node, from, to);
        }

        Ok(mapper)
    }

    pub(crate) fn has_rules(&self) -> bool {
        !self.rules.is_empty()
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
        let node_key = normalize_key(node);
        let from_key = normalize_key(from);
        let target = to.trim().to_string();

        self.rules
            .entry(node_key)
            .or_default()
            .insert(from_key, target);
    }

    fn map_with_node(&self, node: &str, value: &str) -> Option<String> {
        let node_key = normalize_key(node);
        let value_key = normalize_key(value);

        self.rules
            .get(node_key.as_str())
            .and_then(|node_rules| node_rules.get(value_key.as_str()))
            .cloned()
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

fn parse_csv_line(line: &str) -> Result<Vec<String>, String> {
    let mut values = Vec::new();
    let mut buffer = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        if in_quotes {
            match ch {
                '"' => {
                    if matches!(chars.peek(), Some('"')) {
                        buffer.push('"');
                        chars.next();
                    } else {
                        in_quotes = false;
                    }
                }
                _ => buffer.push(ch),
            }
            continue;
        }

        match ch {
            '"' => in_quotes = true,
            ',' => {
                values.push(buffer.trim().to_string());
                buffer.clear();
            }
            _ => buffer.push(ch),
        }
    }

    if in_quotes {
        return Err("unterminated quoted value".to_string());
    }

    values.push(buffer.trim().to_string());
    Ok(values)
}

fn normalize_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn normalize_optional_key(value: Option<&str>) -> Option<String> {
    value.and_then(|current| {
        let key = normalize_key(current);
        if key.is_empty() { None } else { Some(key) }
    })
}

fn has_content(value: &str) -> bool {
    !value.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_csv_accepts_header_and_comments() {
        let mapper = NodeValueMapper::from_csv(
            r#"
# comment line
node,from,to
genre,剧情,Drama
tag,中字,Chinese Subtitle
"actor","Alice A.",Alice
"#,
        )
        .expect("mapper");

        assert!(mapper.has_rules());
        assert_eq!(mapper.map_value("genre", "剧情"), "Drama");
        assert_eq!(mapper.map_value("tag", "中字"), "Chinese Subtitle");
        assert_eq!(mapper.map_value("actor", "Alice A."), "Alice");
    }

    #[test]
    fn parse_csv_rejects_invalid_column_count() {
        let error = NodeValueMapper::from_csv("genre,action")
            .expect_err("should reject invalid column count");
        assert!(error.contains("expected 3 columns"));
    }

    #[test]
    fn apply_movie_maps_and_dedupes_genre_and_tag() {
        let mapper = NodeValueMapper::from_csv(
            r#"
genre,剧情,Drama
genre,drama,Drama
tag,中文字幕,Chinese Subtitle
tag,中字,Chinese Subtitle
"#,
        )
        .expect("mapper");

        let mut movie = Movie {
            genre: vec![
                "剧情".to_string(),
                "drama".to_string(),
                "Action".to_string(),
            ],
            tag: vec![
                "中文字幕".to_string(),
                "中字".to_string(),
                "Uncut".to_string(),
            ],
            ..Movie::default()
        };

        mapper.apply_movie(&mut movie);

        assert_eq!(movie.genre, vec!["Drama".to_string(), "Action".to_string()]);
        assert_eq!(
            movie.tag,
            vec!["Chinese Subtitle".to_string(), "Uncut".to_string()]
        );
    }

    #[test]
    fn apply_movie_dedupes_actor_by_mapped_name() {
        let mapper = NodeValueMapper::from_csv(
            r#"
actor,Bob,Bob
actor,Bob,Bob
actor.role,主演,Lead
"#,
        )
        .expect("mapper");

        let mut movie = Movie {
            actor: vec![
                Actor {
                    name: Some("Bob".to_string()),
                    role: Some("主演".to_string()),
                    gender: None,
                    order: Some(1),
                },
                Actor {
                    name: Some("Bob".to_string()),
                    role: None,
                    gender: Some("female".to_string()),
                    order: Some(2),
                },
                Actor {
                    name: Some("Alice".to_string()),
                    role: None,
                    gender: None,
                    order: Some(3),
                },
            ],
            ..Movie::default()
        };

        mapper.apply_movie(&mut movie);

        assert_eq!(movie.actor.len(), 2);
        assert_eq!(movie.actor[0].name.as_deref(), Some("Bob"));
        assert_eq!(movie.actor[0].role.as_deref(), Some("Lead"));
        assert_eq!(movie.actor[0].gender.as_deref(), Some("female"));
        assert_eq!(movie.actor[0].order, Some(1));
        assert_eq!(movie.actor[1].name.as_deref(), Some("Alice"));
        assert_eq!(movie.actor[1].order, Some(2));
    }

    #[test]
    fn apply_movie_maps_set_fields() {
        let mapper = NodeValueMapper::from_csv(
            r#"
set.name,合集A,Collection A
set.overview,说明A,Overview A
"#,
        )
        .expect("mapper");

        let mut movie = Movie {
            set_info: Some(crate::nfo::SetInfo {
                name: Some("合集A".to_string()),
                overview: Some("说明A".to_string()),
            }),
            ..Movie::default()
        };

        mapper.apply_movie(&mut movie);

        let set = movie.set_info.expect("set");
        assert_eq!(set.name.as_deref(), Some("Collection A"));
        assert_eq!(set.overview.as_deref(), Some("Overview A"));
    }
}
