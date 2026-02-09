use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Instant;

use super::*;

fn touch(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, b"data").expect("write file");
}

#[test]
fn file_len_returns_none_for_removed_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("Movie.mkv");
    touch(&file);
    fs::remove_file(&file).expect("remove file");

    let len = file_len(&file);
    assert!(len.is_none());
}

#[test]
fn is_file_ready_for_existing_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("Movie.mkv");
    touch(&file);

    let ready = is_file_ready(&file).expect("ready");
    assert!(ready);
}

#[test]
fn path_within_max_depth_respects_limits() {
    let root = Path::new("/watch");
    assert!(path_within_max_depth(
        root,
        Path::new("/watch/movie.mkv"),
        0
    ));
    assert!(path_within_max_depth(
        root,
        Path::new("/watch/season/movie.mkv"),
        1,
    ));
    assert!(!path_within_max_depth(
        root,
        Path::new("/watch/season/movie.mkv"),
        0,
    ));
    assert!(path_within_max_depth(
        root,
        Path::new("/watch/deep/path/movie.mkv"),
        -1,
    ));
}

#[test]
fn collect_ready_files_skips_seen_items() {
    let path = PathBuf::from("/tmp/already-seen.mkv");
    let mut seen = std::collections::HashSet::new();
    seen.insert(path.clone());

    let mut pending = std::collections::HashMap::new();
    pending.insert(path.clone(), PendingFile::new(0, Instant::now()));

    let ready = collect_ready_files(&seen, &mut pending);
    assert!(ready.is_empty());
    assert!(!pending.contains_key(&path));
}
