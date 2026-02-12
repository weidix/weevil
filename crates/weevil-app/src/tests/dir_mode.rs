use std::path::Path;

use super::*;
use crate::mode_params::FetchModeParams;

async fn touch(path: &Path) {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .expect("create parent");
    }
    tokio::fs::write(path, b"data").await.expect("write file");
}

#[tokio::test]
async fn collect_video_files_respects_depth_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    touch(&root.join("Movie.mkv")).await;
    touch(&root.join("notes.txt")).await;
    touch(&root.join("child").join("Other.mp4")).await;

    let mut files = collect_video_files(root, Some(0))
        .await
        .expect("collect files");
    files.sort();
    assert_eq!(files, vec![root.join("Movie.mkv")]);
}

#[tokio::test]
async fn collect_video_files_includes_subdir_depth_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    touch(&root.join("Movie.mkv")).await;
    touch(&root.join("child").join("Other.mp4")).await;
    touch(&root.join("child").join("deep").join("Skip.avi")).await;

    let mut files = collect_video_files(root, Some(1))
        .await
        .expect("collect files");
    files.sort();
    assert_eq!(
        files,
        vec![root.join("Movie.mkv"), root.join("child").join("Other.mp4")]
    );
}

#[test]
fn normalize_max_depth_accepts_unlimited() {
    let depth = normalize_max_depth(-1).expect("depth");
    assert!(depth.is_none());
}

#[test]
fn normalize_max_depth_rejects_too_negative() {
    let err = normalize_max_depth(-2).expect_err("expected error");
    assert!(matches!(err, AppError::MaxDepthInvalid { depth: -2 }));
}

#[test]
fn fetch_mode_detects_multithread() {
    let serial = FetchModeParams::new(1, false, 1000);
    assert!(!serial.multithread_enabled());

    let parallel = FetchModeParams::new(3, false, 1000);
    assert!(parallel.multithread_enabled());

    let unlimited = FetchModeParams::new(0, true, 1000);
    assert!(unlimited.multithread_enabled());
}

#[test]
fn group_video_paths_merges_split_parts() {
    let files = vec![
        PathBuf::from("in/Movie-CD2.mkv"),
        PathBuf::from("in/Movie-CD1.mkv"),
        PathBuf::from("in/Other.mkv"),
    ];

    let groups = crate::video_parts::group_video_paths(&files).expect("grouped");
    assert_eq!(groups.len(), 2);
    assert_eq!(
        groups[0],
        vec![
            PathBuf::from("in/Movie-CD1.mkv"),
            PathBuf::from("in/Movie-CD2.mkv")
        ]
    );
    assert_eq!(groups[1], vec![PathBuf::from("in/Other.mkv")]);
}
