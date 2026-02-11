use std::collections::HashSet;
use std::io::BufRead;

use super::NodeValueMapper;
use super::csv::{
    CsvOrder, collect_from_values_line, normalize_key, parse_csv_line, resolve_csv_order,
};

pub(super) fn load_csv_reader(
    mapper: &mut NodeValueMapper,
    reader: &mut dyn BufRead,
) -> Result<(), String> {
    let mut order = None;
    let mut line_no = 0usize;
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

        if node.is_empty() || to.is_empty() {
            return Err(format!(
                "invalid CSV at line {line_no}: node/to/from must be non-empty"
            ));
        }

        let from_values = collect_from_values_line(from_columns, line_no)?;
        let mut seen_from = HashSet::new();
        for from in from_values {
            let from_key = normalize_key(from.as_str());
            if seen_from.insert(from_key) {
                mapper.insert_rule(node, from.as_str(), to);
            }
        }
    }

    Ok(())
}
