use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use tokio::fs;

use crate::errors::AppError;
use crate::fetch_runtime;
use crate::mode_params::{FetchModeParams, FileModeParams};
use crate::video_parts;

pub(crate) async fn run_dir_mode(
    input: &Path,
    params: &FileModeParams,
    fetch: &FetchModeParams,
    max_depth: i32,
) -> Result<(), AppError> {
    let mut files = scan_video_files(input, max_depth).await?;
    files.sort();
    let groups = video_parts::group_video_paths(&files)?;
    fetch_runtime::run_batch_fetch(groups, params, fetch).await
}

pub(crate) async fn scan_video_files(
    input: &Path,
    max_depth: i32,
) -> Result<Vec<PathBuf>, AppError> {
    ensure_input_dir(input).await?;
    let depth_limit = normalize_max_depth(max_depth)?;
    collect_video_files(input, depth_limit).await
}

async fn ensure_input_dir(input: &Path) -> Result<(), AppError> {
    let metadata = fs::metadata(input)
        .await
        .map_err(|err| AppError::InputMetadata {
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

async fn collect_video_files(
    root: &Path,
    max_depth: Option<usize>,
) -> Result<Vec<PathBuf>, AppError> {
    let mut files = Vec::new();
    let mut pending = VecDeque::new();
    pending.push_back((root.to_path_buf(), 0usize));

    while let Some((dir, depth)) = pending.pop_front() {
        let mut entries = fs::read_dir(&dir).await.map_err(|err| AppError::DirRead {
            path: dir.clone(),
            source: err,
        })?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|err| AppError::DirRead {
                path: dir.clone(),
                source: err,
            })?
        {
            let file_type = entry
                .file_type()
                .await
                .map_err(|err| AppError::DirEntryType {
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

pub(crate) fn is_video_path(path: &Path) -> bool {
    video_parts::is_video_path(path)
}

#[cfg(test)]
#[path = "tests/dir_mode.rs"]
mod tests;
