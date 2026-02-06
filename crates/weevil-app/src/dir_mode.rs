use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use crate::errors::AppError;
use crate::file_mode::{self, MultiFolderStrategy};

const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "avi", "mov", "m4v", "wmv", "flv", "webm", "ts", "m2ts", "mts", "mpg", "mpeg",
];

pub(crate) fn run_dir_mode(
    input: &Path,
    script: &Path,
    output_template: &str,
    input_name_remove: &[String],
    folder_multi: MultiFolderStrategy,
    max_depth: i32,
) -> Result<(), AppError> {
    ensure_input_dir(input)?;
    let depth_limit = normalize_max_depth(max_depth)?;
    let mut files = collect_video_files(input, depth_limit)?;
    files.sort();
    for file in files {
        file_mode::run_file_mode(
            &file,
            script,
            output_template,
            input_name_remove,
            folder_multi,
        )?;
    }
    Ok(())
}

fn ensure_input_dir(input: &Path) -> Result<(), AppError> {
    let metadata = fs::metadata(input).map_err(|err| AppError::InputMetadata {
        path: input.to_path_buf(),
        source: err,
    })?;
    if !metadata.is_dir() {
        return Err(AppError::InputNotDir {
            path: input.to_path_buf(),
        });
    }
    Ok(())
}

fn normalize_max_depth(max_depth: i32) -> Result<Option<usize>, AppError> {
    if max_depth == -1 {
        return Ok(None);
    }
    if max_depth < -1 {
        return Err(AppError::MaxDepthInvalid { depth: max_depth });
    }
    let depth =
        usize::try_from(max_depth).map_err(|_| AppError::MaxDepthInvalid { depth: max_depth })?;
    Ok(Some(depth))
}

fn collect_video_files(root: &Path, max_depth: Option<usize>) -> Result<Vec<PathBuf>, AppError> {
    let mut files = Vec::new();
    let mut pending = VecDeque::new();
    pending.push_back((root.to_path_buf(), 0usize));

    while let Some((dir, depth)) = pending.pop_front() {
        let entries = fs::read_dir(&dir).map_err(|err| AppError::DirRead {
            path: dir.clone(),
            source: err,
        })?;
        for entry in entries {
            let entry = entry.map_err(|err| AppError::DirRead {
                path: dir.clone(),
                source: err,
            })?;
            let file_type = entry.file_type().map_err(|err| AppError::DirEntryType {
                path: entry.path(),
                source: err,
            })?;
            let path = entry.path();
            if file_type.is_file() {
                if is_video_path(&path) {
                    files.push(path);
                }
                continue;
            }
            if file_type.is_dir() && should_descend(depth, max_depth) {
                pending.push_back((path, depth.saturating_add(1)));
            }
        }
    }

    Ok(files)
}

fn should_descend(depth: usize, max_depth: Option<usize>) -> bool {
    match max_depth {
        Some(limit) => depth < limit,
        None => true,
    }
}

fn is_video_path(path: &Path) -> bool {
    let extension = match path.extension().and_then(|value| value.to_str()) {
        Some(value) => value,
        None => return false,
    };
    let extension = extension.to_ascii_lowercase();
    VIDEO_EXTENSIONS
        .iter()
        .any(|candidate| *candidate == extension)
}

#[cfg(test)]
#[path = "tests/dir_mode.rs"]
mod tests;
