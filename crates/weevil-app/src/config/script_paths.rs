use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut deduped = Vec::new();
    for path in paths {
        if !deduped.contains(&path) {
            deduped.push(path);
        }
    }
    deduped
}

pub(super) fn expand_script_patterns(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut expanded = Vec::new();

    for path in paths {
        if !contains_glob(&path) {
            expanded.push(path);
            continue;
        }

        let mut matched = expand_glob_pattern(&path);
        if matched.is_empty() {
            expanded.push(path);
            continue;
        }

        expanded.append(&mut matched);
    }

    dedupe_paths(expanded)
}

#[derive(Debug, Clone)]
enum GlobSegment {
    Recursive,
    Pattern(String),
    Literal(OsString),
}

fn expand_glob_pattern(pattern: &Path) -> Vec<PathBuf> {
    let (base, segments) = split_pattern(pattern);
    if segments.is_empty() {
        return vec![pattern.to_path_buf()];
    }

    let mut matches = Vec::new();
    expand_segments(&base, &segments, &mut matches);
    matches.sort();
    dedupe_paths(matches)
}

fn split_pattern(pattern: &Path) -> (PathBuf, Vec<GlobSegment>) {
    let mut base = PathBuf::new();
    let mut segments = Vec::new();
    let mut entered_glob = false;

    for component in pattern.components() {
        let os_component = component.as_os_str();
        let component_string = os_component.to_string_lossy();

        let is_recursive = component_string == "**";
        let is_pattern = contains_glob_text(&component_string);

        if entered_glob || is_recursive || is_pattern {
            entered_glob = true;
            if is_recursive {
                segments.push(GlobSegment::Recursive);
            } else if is_pattern {
                segments.push(GlobSegment::Pattern(component_string.into_owned()));
            } else {
                segments.push(GlobSegment::Literal(os_component.to_os_string()));
            }
        } else {
            base.push(os_component);
        }
    }

    (base, segments)
}

fn expand_segments(current: &Path, segments: &[GlobSegment], matches: &mut Vec<PathBuf>) {
    if segments.is_empty() {
        if current.is_file() {
            matches.push(normalize_match_path(current));
        }
        return;
    }

    match &segments[0] {
        GlobSegment::Literal(component) => {
            let next = current.join(component);
            if next.exists() {
                expand_segments(&next, &segments[1..], matches);
            }
        }
        GlobSegment::Pattern(pattern) => {
            for entry in list_entries(current) {
                let Some(file_name) = entry.file_name() else {
                    continue;
                };
                let entry_name = file_name.to_string_lossy();
                if wildcard_component_matches(pattern, &entry_name) {
                    expand_segments(&entry, &segments[1..], matches);
                }
            }
        }
        GlobSegment::Recursive => {
            expand_segments(current, &segments[1..], matches);

            for entry in list_entries(current) {
                if is_directory(&entry) {
                    expand_segments(&entry, segments, matches);
                }
            }
        }
    }
}

fn list_entries(path: &Path) -> Vec<PathBuf> {
    let base = if path.as_os_str().is_empty() {
        Path::new(".")
    } else {
        path
    };

    let Ok(read_dir) = fs::read_dir(base) else {
        return Vec::new();
    };

    let mut entries = Vec::new();
    for entry in read_dir.flatten() {
        entries.push(entry.path());
    }
    entries.sort();
    entries
}

fn is_directory(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_dir())
        .unwrap_or(false)
}

fn normalize_match_path(path: &Path) -> PathBuf {
    let path_buf = path.to_path_buf();
    path_buf
        .strip_prefix(Path::new("."))
        .map(Path::to_path_buf)
        .unwrap_or(path_buf)
}

fn contains_glob(path: &Path) -> bool {
    path.components()
        .any(|component| contains_glob_text(&component.as_os_str().to_string_lossy()))
}

fn contains_glob_text(text: &str) -> bool {
    text.contains('*') || text.contains('?')
}

fn wildcard_component_matches(pattern: &str, target: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let target: Vec<char> = target.chars().collect();
    let mut dp = vec![vec![false; target.len() + 1]; pattern.len() + 1];

    dp[0][0] = true;
    for index in 1..=pattern.len() {
        if pattern[index - 1] == '*' {
            dp[index][0] = dp[index - 1][0];
        }
    }

    for pattern_index in 1..=pattern.len() {
        for target_index in 1..=target.len() {
            dp[pattern_index][target_index] = match pattern[pattern_index - 1] {
                '*' => dp[pattern_index - 1][target_index] || dp[pattern_index][target_index - 1],
                '?' => dp[pattern_index - 1][target_index - 1],
                character => {
                    dp[pattern_index - 1][target_index - 1] && character == target[target_index - 1]
                }
            };
        }
    }

    dp[pattern.len()][target.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_component_supports_star_and_question() {
        assert!(wildcard_component_matches("*.lua", "demo.lua"));
        assert!(wildcard_component_matches("a?.lua", "ab.lua"));
        assert!(!wildcard_component_matches("a?.lua", "abc.lua"));
        assert!(!wildcard_component_matches("*.lua", "demo.txt"));
    }

    #[test]
    fn expand_script_patterns_keeps_literal_when_glob_unmatched() {
        let literal = PathBuf::from("scripts/missing/*.lua");
        let expanded = expand_script_patterns(vec![literal.clone()]);
        assert_eq!(expanded, vec![literal]);
    }
}
