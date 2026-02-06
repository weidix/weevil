use std::fs;
use std::path::Path;

use super::*;

fn touch(path: &Path) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(path, b"data").expect("write file");
}

#[test]
fn collect_video_files_respects_depth_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    touch(&root.join("Movie.mkv"));
    touch(&root.join("notes.txt"));
    touch(&root.join("child").join("Other.mp4"));

    let mut files = collect_video_files(root, Some(0)).expect("collect files");
    files.sort();
    assert_eq!(files, vec![root.join("Movie.mkv")]);
}

#[test]
fn collect_video_files_includes_subdir_depth_one() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    touch(&root.join("Movie.mkv"));
    touch(&root.join("child").join("Other.mp4"));
    touch(&root.join("child").join("deep").join("Skip.avi"));

    let mut files = collect_video_files(root, Some(1)).expect("collect files");
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
