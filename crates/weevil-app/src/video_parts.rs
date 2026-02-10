use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

use crate::errors::AppError;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "mov", "m4v", "wmv", "flv", "webm", "ts", "m2ts", "mts", "mpg", "mpeg",
];

#[derive(Debug, Clone)]
pub(crate) struct VideoInputPart {
    pub(crate) path: PathBuf,
    pub(crate) input_stem: String,
    pub(crate) output_suffix: String,
}

#[derive(Debug, Clone)]
pub(crate) struct VideoInputGroup {
    pub(crate) input_stem: String,
    pub(crate) primary_path: PathBuf,
    pub(crate) parts: Vec<VideoInputPart>,
}

#[derive(Debug, Clone)]
struct SplitPartInfo {
    base_stem: String,
    part_index: u32,
}

#[derive(Debug, Clone)]
struct InputCandidate {
    path: PathBuf,
    input_stem: String,
    split: Option<SplitPartInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum GroupKey {
    Split { parent: PathBuf, normalized: String },
    Single { path: PathBuf },
}

pub(crate) fn group_video_inputs(paths: &[PathBuf]) -> Result<Vec<VideoInputGroup>, AppError> {
    let unique = dedupe_paths(paths);
    if unique.is_empty() {
        return Ok(Vec::new());
    }

    let mut grouped = BTreeMap::<GroupKey, Vec<InputCandidate>>::new();
    for path in unique {
        let input_stem = file_stem_string(&path)?;
        let split = parse_split_part_info(&input_stem);

        let key = if let Some(split_info) = &split {
            GroupKey::Split {
                parent: parent_dir(&path),
                normalized: normalize_group_key(&split_info.base_stem),
            }
        } else {
            GroupKey::Single { path: path.clone() }
        };

        grouped.entry(key).or_default().push(InputCandidate {
            path,
            input_stem,
            split,
        });
    }

    let mut result = Vec::with_capacity(grouped.len());
    for (_, mut candidates) in grouped {
        sort_candidates(&mut candidates);
        if candidates.len() == 1 {
            let candidate = candidates.remove(0);
            result.push(VideoInputGroup {
                input_stem: candidate.input_stem.clone(),
                primary_path: candidate.path.clone(),
                parts: vec![VideoInputPart {
                    path: candidate.path,
                    input_stem: candidate.input_stem,
                    output_suffix: String::new(),
                }],
            });
            continue;
        }

        let lookup_stem = candidates
            .first()
            .and_then(|candidate| candidate.split.as_ref())
            .map(|split| split.base_stem.clone())
            .unwrap_or_else(|| candidates[0].input_stem.clone());

        let mut parts = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            let output_suffix = candidate
                .split
                .as_ref()
                .map(|split| format!(".part{:02}", split.part_index))
                .unwrap_or_default();
            parts.push(VideoInputPart {
                path: candidate.path,
                input_stem: candidate.input_stem,
                output_suffix,
            });
        }
        let primary_path = parts[0].path.clone();
        result.push(VideoInputGroup {
            input_stem: lookup_stem,
            primary_path,
            parts,
        });
    }

    result.sort_by(|left, right| left.primary_path.cmp(&right.primary_path));
    Ok(result)
}

pub(crate) fn group_video_paths(paths: &[PathBuf]) -> Result<Vec<Vec<PathBuf>>, AppError> {
    let groups = group_video_inputs(paths)?;
    Ok(groups
        .into_iter()
        .map(|group| {
            group
                .parts
                .into_iter()
                .map(|part| part.path)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>())
}

pub(crate) fn sibling_split_group_paths(path: &Path) -> Result<Vec<PathBuf>, AppError> {
    if !is_video_path(path) {
        return Ok(vec![path.to_path_buf()]);
    }

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let entries = fs::read_dir(parent).map_err(|err| AppError::DirRead {
        path: parent.to_path_buf(),
        source: err,
    })?;

    let mut videos = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| AppError::DirRead {
            path: parent.to_path_buf(),
            source: err,
        })?;
        let file_type = entry.file_type().map_err(|err| AppError::DirEntryType {
            path: entry.path(),
            source: err,
        })?;
        if !file_type.is_file() {
            continue;
        }
        let entry_path = entry.path();
        if is_video_path(&entry_path) {
            videos.push(entry_path);
        }
    }

    let groups = group_video_paths(&videos)?;
    let target = path.to_path_buf();
    for group in groups {
        if group.contains(&target) {
            return Ok(group);
        }
    }
    Ok(vec![target])
}

pub(crate) fn is_video_path(path: &Path) -> bool {
    let extension = match path.extension().and_then(|value| value.to_str()) {
        Some(value) => value,
        None => return false,
    };
    let extension = extension.to_ascii_lowercase();
    VIDEO_EXTENSIONS
        .iter()
        .any(|candidate| *candidate == extension)
}

pub(crate) fn stem_contains_split_marker(stem: &str) -> bool {
    split_marker_regex().is_match(stem)
}

fn sort_candidates(candidates: &mut [InputCandidate]) {
    candidates.sort_by(|left, right| {
        let left_index = left
            .split
            .as_ref()
            .map(|value| value.part_index)
            .unwrap_or(u32::MAX);
        let right_index = right
            .split
            .as_ref()
            .map(|value| value.part_index)
            .unwrap_or(u32::MAX);
        left_index
            .cmp(&right_index)
            .then_with(|| left.path.cmp(&right.path))
    });
}

fn parse_split_part_info(stem: &str) -> Option<SplitPartInfo> {
    let captures = split_part_regex().captures(stem)?;
    let base = captures.name("base")?.as_str();
    let base = base
        .trim_end_matches([' ', '.', '_', '-'])
        .trim_start_matches([' ', '.', '_', '-']);
    if base.is_empty() {
        return None;
    }

    let index = captures.name("index")?.as_str().parse::<u32>().ok()?;
    if index == 0 {
        return None;
    }

    Some(SplitPartInfo {
        base_stem: base.to_string(),
        part_index: index,
    })
}

fn split_part_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?ix)
            ^(?P<base>.+?)
            [\s._-]*
            (?:cd|disc|disk|part|pt)
            [\s._-]*
            0*(?P<index>[1-9][0-9]{0,2})
            $",
        )
        .expect("split-part regex must be valid")
    })
}

fn split_marker_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?ix)
            (?:^|[\s._-])
            (?:cd|disc|disk|part|pt)
            [\s._-]*0*[1-9][0-9]{0,2}
            (?:$|[\s._-])",
        )
        .expect("split-marker regex must be valid")
    })
}

fn normalize_group_key(base: &str) -> String {
    let mut normalized = String::new();
    for ch in base.chars() {
        if ch.is_alphanumeric() {
            normalized.extend(ch.to_lowercase());
        }
    }
    if normalized.is_empty() {
        base.to_ascii_lowercase()
    } else {
        normalized
    }
}

fn parent_dir(path: &Path) -> PathBuf {
    path.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(PathBuf::new)
}

fn dedupe_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut deduped = Vec::with_capacity(paths.len());
    let mut seen = HashSet::new();
    for path in paths {
        if seen.insert(path.clone()) {
            deduped.push(path.clone());
        }
    }
    deduped
}

fn file_stem_string(path: &Path) -> Result<String, AppError> {
    path.file_stem()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string())
        .ok_or_else(|| AppError::PathStemNotUtf8 {
            path: path.to_path_buf(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_video_inputs_merges_split_parts_in_same_directory() {
        let inputs = vec![
            PathBuf::from("in/Movie-CD2.mkv"),
            PathBuf::from("in/Movie-CD1.mkv"),
        ];

        let groups = group_video_inputs(&inputs).expect("groups");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].input_stem, "Movie");
        assert_eq!(
            groups[0]
                .parts
                .iter()
                .map(|part| part.output_suffix.clone())
                .collect::<Vec<_>>(),
            vec![".part01".to_string(), ".part02".to_string()]
        );
    }

    #[test]
    fn group_video_inputs_does_not_merge_across_directories() {
        let inputs = vec![
            PathBuf::from("a/Movie-CD1.mkv"),
            PathBuf::from("b/Movie-CD2.mkv"),
        ];

        let groups = group_video_inputs(&inputs).expect("groups");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].parts.len(), 1);
        assert_eq!(groups[1].parts.len(), 1);
        assert!(groups[0].parts[0].output_suffix.is_empty());
        assert!(groups[1].parts[0].output_suffix.is_empty());
    }

    #[test]
    fn group_video_inputs_keeps_non_split_files_single() {
        let inputs = vec![PathBuf::from("in/Movie.2024.mkv")];

        let groups = group_video_inputs(&inputs).expect("groups");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].input_stem, "Movie.2024");
        assert!(groups[0].parts[0].output_suffix.is_empty());
    }

    #[test]
    fn stem_contains_split_marker_detects_part_tokens() {
        assert!(stem_contains_split_marker("Movie-CD1"));
        assert!(stem_contains_split_marker("Movie.part02.zh"));
        assert!(stem_contains_split_marker("Movie_Disc3"));
        assert!(!stem_contains_split_marker("Movie.zh"));
        assert!(!stem_contains_split_marker("The.Party"));
    }
}
