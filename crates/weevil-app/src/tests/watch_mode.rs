use std::path::Path;
use std::path::PathBuf;
use std::time::Instant;

use super::*;

async fn touch(path: &Path) {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .expect("create parent");
    }
    tokio::fs::write(path, b"data").await.expect("write file");
}

#[tokio::test]
async fn file_len_returns_none_for_removed_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("Movie.mkv");
    touch(&file).await;
    tokio::fs::remove_file(&file).await.expect("remove file");

    let len = file_len(&file).await;
    assert!(len.is_none());
}

#[tokio::test]
async fn is_file_ready_for_existing_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("Movie.mkv");
    touch(&file).await;

    let ready = is_file_ready(&file).await.expect("ready");
    assert!(ready);
}

#[test]
fn path_within_max_depth_respects_limits() {
    let root = Path::new("watch");
    assert!(path_within_max_depth(root, Path::new("watch/movie.mkv"), 0));
    assert!(path_within_max_depth(
        root,
        Path::new("watch/season/movie.mkv"),
        1,
    ));
    assert!(!path_within_max_depth(
        root,
        Path::new("watch/season/movie.mkv"),
        0,
    ));
    assert!(path_within_max_depth(
        root,
        Path::new("watch/deep/path/movie.mkv"),
        -1,
    ));
}

#[tokio::test]
async fn collect_ready_files_skips_seen_items() {
    let path = PathBuf::from("already-seen.mkv");
    let mut seen = std::collections::HashSet::new();
    seen.insert(path.clone());

    let mut pending = std::collections::HashMap::new();
    pending.insert(path.clone(), PendingFile::new(0, Instant::now()));

    let ready = collect_ready_files(&seen, &mut pending).await;
    assert!(ready.is_empty());
    assert!(!pending.contains_key(&path));
}

#[tokio::test]
async fn group_ready_files_merges_sibling_split_parts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let first = dir.path().join("Movie-CD1.mkv");
    let second = dir.path().join("Movie-CD2.mkv");
    touch(&first).await;
    touch(&second).await;

    let ready = vec![first.clone()];
    let mut groups = group_ready_files(&ready).await.expect("grouped");
    groups[0].sort();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0], vec![first, second]);
}

#[tokio::test]
async fn enqueue_startup_scan_files_are_ready_without_waiting_stable_window() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("Movie.mkv");
    touch(&file).await;

    let mut pending = std::collections::HashMap::new();
    enqueue_startup_scan_files(vec![file.clone()], &mut pending)
        .await
        .expect("enqueue startup files");

    let seen = std::collections::HashSet::new();
    let ready = collect_ready_files(&seen, &mut pending).await;
    assert_eq!(ready, vec![file]);
}
