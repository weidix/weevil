use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use fs2::FileExt;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use tracing::info;

use crate::dir_mode;
use crate::errors::AppError;
use crate::file_mode;
use crate::mode_params::FileModeParams;

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

    fn mark_changed(&mut self, len: u64, now: Instant) {
        self.last_len = len;
        self.last_change_at = now;
    }
}

pub(crate) fn run_watch_mode(
    input: &Path,
    params: &FileModeParams,
    max_depth: i32,
) -> Result<(), AppError> {
    let mut seen = dir_mode::scan_video_files(input, max_depth)?
        .into_iter()
        .collect::<HashSet<_>>();
    let mut pending: HashMap<PathBuf, PendingFile> = HashMap::new();

    let (sender, receiver) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |result| {
        let _ = sender.send(result);
    })
    .map_err(|err| watch_init_error(input, err))?;

    watcher
        .watch(input, RecursiveMode::Recursive)
        .map_err(|err| watch_start_error(input, err))?;

    info!("watch mode started with notify backend: {:?}", input);

    loop {
        match receiver.recv_timeout(CHECK_TICK) {
            Ok(Ok(event)) => {
                handle_event(event, input, max_depth, &mut seen, &mut pending)?;
            }
            Ok(Err(err)) => {
                info!("watch backend event error: {err}");
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(watch_channel_error(input));
            }
        }

        seen.retain(|path| path.exists());
        pending.retain(|path, _| path.exists());

        process_ready_files(&mut seen, &mut pending, params);
    }
}

fn process_ready_files(
    seen: &mut HashSet<PathBuf>,
    pending: &mut HashMap<PathBuf, PendingFile>,
    params: &FileModeParams,
) {
    let now = Instant::now();
    let paths = pending.keys().cloned().collect::<Vec<_>>();

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

        let Some(current_len) = file_len(&path) else {
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

        let ready = match is_file_ready(&path) {
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

        match file_mode::run_file_mode(&path, params) {
            Ok(()) => {
                info!("watch processed file: {:?}", path);
                seen.insert(path.clone());
                pending.remove(&path);
            }
            Err(err) => {
                info!("watch failed for {:?}: {}", path, err);
                if let Some(retry_state) = pending.get_mut(&path) {
                    retry_state.next_retry_at = now + FAILURE_BACKOFF;
                }
            }
        }
    }
}

fn handle_event(
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
        if !path.exists() {
            seen.remove(&path);
            pending.remove(&path);
            continue;
        }
        if !path.is_file() {
            continue;
        }
        if !path_within_max_depth(root, &path, max_depth) {
            continue;
        }
        if !dir_mode::is_video_path(&path) {
            continue;
        }

        let metadata = fs::metadata(&path).map_err(|err| AppError::InputMetadata {
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

fn file_len(path: &Path) -> Option<u64> {
    match fs::metadata(path) {
        Ok(metadata) => Some(metadata.len()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => None,
    }
}

fn is_file_ready(path: &Path) -> Result<bool, AppError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => {
            return Err(AppError::FileLock {
                path: path.to_path_buf(),
                source: err,
            });
        }
    };
    match file.try_lock_exclusive() {
        Ok(()) => {
            file.unlock().map_err(|err| AppError::FileLock {
                path: path.to_path_buf(),
                source: err,
            })?;
            Ok(true)
        }
        Err(_) => Ok(false),
    }
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
