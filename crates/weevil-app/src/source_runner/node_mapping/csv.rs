use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CsvOrder {
    NodeToFrom,
    NodeFromTo,
}

pub(super) fn resolve_csv_order(
    order: &mut Option<CsvOrder>,
    node: &str,
    col2: &str,
    col3: &str,
) -> bool {
    if order.is_some() {
        return false;
    }

    if let Some(header) = detect_header_order(node, col2, col3) {
        *order = Some(header);
        return true;
    }

    *order = Some(CsvOrder::NodeToFrom);
    false
}

fn detect_header_order(node: &str, col2: &str, col3: &str) -> Option<CsvOrder> {
    if node.eq_ignore_ascii_case("node")
        && col2.eq_ignore_ascii_case("to")
        && col3.trim().to_ascii_lowercase().starts_with("from")
    {
        return Some(CsvOrder::NodeToFrom);
    }
    if node.eq_ignore_ascii_case("node")
        && col2.eq_ignore_ascii_case("from")
        && col3.eq_ignore_ascii_case("to")
    {
        return Some(CsvOrder::NodeFromTo);
    }
    None
}

pub(super) fn parse_csv_line(line: &str) -> Result<Vec<String>, String> {
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

pub(super) fn collect_from_values_line(
    columns: &[String],
    line_no: usize,
) -> Result<Vec<String>, String> {
    collect_from_values_impl(columns, format!("line {line_no}"))
}

pub(super) fn collect_from_values_offset(
    columns: &[String],
    offset: u64,
) -> Result<Vec<String>, String> {
    collect_from_values_impl(columns, format!("offset {offset}"))
}

fn collect_from_values_impl(columns: &[String], context: String) -> Result<Vec<String>, String> {
    let mut values = Vec::new();
    if columns.is_empty() {
        return Err(format!(
            "invalid CSV at {context}: from values must be non-empty"
        ));
    }
    for column in columns {
        let value = column.trim();
        if value.is_empty() {
            return Err(format!(
                "invalid CSV at {context}: from values must be non-empty"
            ));
        }
        values.push(value.to_string());
    }
    Ok(values)
}

pub(super) fn normalize_key(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

pub(super) fn hash_key(node_key: &str, value_key: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    node_key.hash(&mut hasher);
    0u8.hash(&mut hasher);
    value_key.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn normalize_optional_key(value: Option<&str>) -> Option<String> {
    value.and_then(|current| {
        let key = normalize_key(current);
        if key.is_empty() { None } else { Some(key) }
    })
}

pub(super) fn has_content(value: &str) -> bool {
    !value.trim().is_empty()
}
