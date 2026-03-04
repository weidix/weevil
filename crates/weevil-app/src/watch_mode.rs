use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use fs2::FileExt;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use tokio::fs;
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::info;

use crate::dir_mode;
use crate::errors::AppError;
use crate::fetch_runtime;
use crate::mode_params::{FetchModeParams, FileModeParams};
use crate::video_parts;

const STABLE_WINDOW: Duration = Duration::from_secs(3);
const CHECK_TICK: Duration = Duration::from_secs(1);
const FAILURE_BACKOFF: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
struct PendingFile {
    last_len: u64,
    last_change_at: Instant,
    next_retry_at: Instant,
}

impl PendingFile {
    fn new(len: u64, now: Instant) -> Self {
        Self {
            last_len: len,
            last_change_at: now,
            next_retry_at: now,
        }
    }

    fn startup_scan(len: u64, now: Instant) -> Self {
        Self {
            last_len: len,
            last_change_at: now.checked_sub(STABLE_WINDOW).unwrap_or(now),
            next_retry_at: now,
        }
    }

    fn mark_changed(&mut self, len: u64, now: Instant) {
        self.last_len = len;
        self.last_change_at = now;
    }
}

pub(crate) async fn run_watch_mode(
    input: &Path,
    params: &FileModeParams,
    fetch: &FetchModeParams,
    max_depth: i32,
) -> Result<(), AppError> {
    fetch_runtime::preflight_script(fetch, params.scripts()).await?;
    info!(
        target: "weevil.app",
        input = %input.display(),
        max_depth,
        scripts = ?params.scripts(),
        "watch mode initializing"
    );

    let mut seen = HashSet::new();
    let mut pending: HashMap<PathBuf, PendingFile> = HashMap::new();
    let startup_files = dir_mode::scan_video_files(input, max_depth).await?;
    enqueue_startup_scan_files(startup_files, &mut pending).await?;
    info!(
        "watch startup scan queued {} existing video file(s)",
        pending.len()
    );

    let (sender, mut receiver) = mpsc::unbounded_channel();
    let mut watcher = notify::recommended_watcher(move |result| {
        let _ = sender.send(result);
    })
    .map_err(|err| watch_init_error(input, err))?;

    watcher
        .watch(input, RecursiveMode::Recursive)
        .map_err(|err| watch_start_error(input, err))?;

    info!(
        "watch mode started listening with notify backend: {:?}",
        input
    );

    let mut ticker = interval(CHECK_TICK);

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                process_ready_files(&mut seen, &mut pending, params, fetch).await?;
            }
            message = receiver.recv() => {
                match message {
                    Some(Ok(event)) => {
                        handle_event(event, input, max_depth, &mut seen, &mut pending).await?;
                    }
                    Some(Err(err)) => {
                        info!("watch backend event error: {err}");
                    }
                    None => {
                        return Err(watch_channel_error(input));
                    }
                }
            }
        }

        retain_existing_paths(&mut seen).await;
        retain_existing_pending(&mut pending).await;
    }
}

async fn retain_existing_paths(seen: &mut HashSet<PathBuf>) {
    let paths = seen.iter().cloned().collect::<Vec<_>>();
    for path in paths {
        if !path_exists(&path).await {
            seen.remove(&path);
        }
    }
}

async fn enqueue_startup_scan_files(
    paths: Vec<PathBuf>,
    pending: &mut HashMap<PathBuf, PendingFile>,
) -> Result<(), AppError> {
    let now = Instant::now();
    for path in paths {
        let metadata = match fs::metadata(&path).await {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(AppError::InputMetadata {
                    path: path.clone(),
                    source: err,
                });
            }
        };
        if !metadata.is_file() {
            continue;
        }
        pending.insert(path, PendingFile::startup_scan(metadata.len(), now));
    }

    Ok(())
}

async fn retain_existing_pending(pending: &mut HashMap<PathBuf, PendingFile>) {
    let paths = pending.keys().cloned().collect::<Vec<_>>();
    for path in paths {
        if !path_exists(&path).await {
            pending.remove(&path);
        }
    }
}

async fn process_ready_files(
    seen: &mut HashSet<PathBuf>,
    pending: &mut HashMap<PathBuf, PendingFile>,
    params: &FileModeParams,
    fetch: &FetchModeParams,
) -> Result<(), AppError> {
    let ready = collect_ready_files(seen, pending).await;
    if ready.is_empty() {
        return Ok(());
    }

    let ready_groups = group_ready_files(&ready).await?;

    let now = Instant::now();
    let script_throttle = fetch_runtime::script_throttle_config(fetch);
    let results =
        fetch_runtime::run_batch_fetch_with_results(ready_groups, params, fetch, script_throttle)
            .await?;
    for (group, result) in results {
        match result {
            Ok(()) => {
                info!("watch processed files: {:?}", group);
                for path in group {
                    seen.insert(path.clone());
                    pending.remove(&path);
                }
            }
            Err(err) => {
                info!("watch failed for {:?}: {}", group, err);
                for path in group {
                    if let Some(retry_state) = pending.get_mut(&path) {
                        retry_state.next_retry_at = now + FAILURE_BACKOFF;
                    }
                }
            }
        }
    }
    Ok(())
}

async fn collect_ready_files(
    seen: &HashSet<PathBuf>,
    pending: &mut HashMap<PathBuf, PendingFile>,
) -> Vec<PathBuf> {
    let now = Instant::now();
    let paths = pending.keys().cloned().collect::<Vec<_>>();
    let mut ready_files = Vec::new();

    for path in paths {
        if seen.contains(&path) {
            pending.remove(&path);
            continue;
        }

        let Some(state) = pending.get_mut(&path) else {
            continue;
        };

        if now < state.next_retry_at {
            continue;
        }

        let Some(current_len) = file_len(&path).await else {
            pending.remove(&path);
            continue;
        };

        if current_len != state.last_len {
            state.mark_changed(current_len, now);
            continue;
        }

        if now.duration_since(state.last_change_at) < STABLE_WINDOW {
            continue;
        }

        let ready = match is_file_ready(&path).await {
            Ok(value) => value,
            Err(err) => {
                info!("watch failed to inspect file {:?}: {}", path, err);
                state.next_retry_at = now + FAILURE_BACKOFF;
                continue;
            }
        };
        if !ready {
            continue;
        }

        ready_files.push(path);
    }

    ready_files
}

async fn group_ready_files(paths: &[PathBuf]) -> Result<Vec<Vec<PathBuf>>, AppError> {
    let mut groups = Vec::new();
    let mut seen = HashSet::new();

    for path in paths {
        if seen.contains(path) {
            continue;
        }
        let mut group = video_parts::sibling_split_group_paths(path).await?;
        group.sort();
        for part in &group {
            seen.insert(part.clone());
        }
        groups.push(group);
    }

    Ok(groups)
}

async fn handle_event(
    event: Event,
    root: &Path,
    max_depth: i32,
    seen: &mut HashSet<PathBuf>,
    pending: &mut HashMap<PathBuf, PendingFile>,
) -> Result<(), AppError> {
    if !is_data_change_event(&event.kind) {
        return Ok(());
    }

    let now = Instant::now();
    for path in event.paths {
        if !path.starts_with(root) {
            continue;
        }
        if !path_exists(&path).await {
            seen.remove(&path);
            pending.remove(&path);
            continue;
        }
        if !path_is_file(&path).await {
            continue;
        }
        if !path_within_max_depth(root, &path, max_depth) {
            continue;
        }
        if !dir_mode::is_video_path(&path) {
            continue;
        }

        let metadata = fs::metadata(&path)
            .await
            .map_err(|err| AppError::InputMetadata {
                path: path.clone(),
                source: err,
            })?;
        let len = metadata.len();

        if let Some(state) = pending.get_mut(&path) {
            state.mark_changed(len, now);
        } else {
            pending.insert(path.clone(), PendingFile::new(len, now));
        }
    }
    Ok(())
}

fn is_data_change_event(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

fn path_within_max_depth(root: &Path, path: &Path, max_depth: i32) -> bool {
    if max_depth == -1 {
        return true;
    }
    if max_depth < -1 {
        return false;
    }

    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let components = relative.components().count();
    if components == 0 {
        return true;
    }

    let depth = components.saturating_sub(1);
    let Ok(limit) = usize::try_from(max_depth) else {
        return false;
    };
    depth <= limit
}

async fn file_len(path: &Path) -> Option<u64> {
    match fs::metadata(path).await {
        Ok(metadata) => Some(metadata.len()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => None,
    }
}

async fn is_file_ready(path: &Path) -> Result<bool, AppError> {
    let path = path.to_path_buf();
    let path_for_error = path.clone();
    tokio::task::spawn_blocking(move || {
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(err) => return Err(err),
        };
        match file.try_lock_exclusive() {
            Ok(()) => {
                file.unlock()?;
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    })
    .await
    .map_err(|err| AppError::FetchRuntime {
        reason: format!("failed to inspect file readiness {path_for_error:?}: {err}"),
    })?
    .map_err(|err| AppError::FileLock {
        path: path_for_error,
        source: err,
    })
}

async fn path_exists(path: &Path) -> bool {
    fs::try_exists(path).await.unwrap_or(false)
}

async fn path_is_file(path: &Path) -> bool {
    fs::metadata(path)
        .await
        .map(|metadata| metadata.is_file())
        .unwrap_or(false)
}

fn watch_init_error(path: &Path, err: notify::Error) -> AppError {
    AppError::DirRead {
        path: path.to_path_buf(),
        source: std::io::Error::other(format!("failed to initialize notify watcher: {err}")),
    }
}

fn watch_start_error(path: &Path, err: notify::Error) -> AppError {
    AppError::DirRead {
        path: path.to_path_buf(),
        source: std::io::Error::other(format!("failed to watch directory with notify: {err}")),
    }
}

fn watch_channel_error(path: &Path) -> AppError {
    AppError::DirRead {
        path: path.to_path_buf(),
        source: std::io::Error::other("notify event channel disconnected unexpectedly"),
    }
}

#[cfg(test)]
#[path = "tests/watch_mode.rs"]
mod tests;
