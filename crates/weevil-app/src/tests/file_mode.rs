use super::*;

#[test]
fn subtitle_suffix_matches_language_suffix() {
    assert_eq!(
        subtitle_suffix("Movie", "Movie.zh"),
        Some(".zh".to_string())
    );
    assert_eq!(subtitle_suffix("Movie", "Movie"), Some(String::new()));
    assert_eq!(subtitle_suffix("Movie", "Other"), None);
}

#[test]
fn select_output_paths_first_strategy_keeps_only_primary() {
    let outputs = vec![PathBuf::from("one/Title"), PathBuf::from("two/Title")];
    let selected =
        select_output_paths(outputs, "{title}", MultiFolderStrategy::First).expect("selected");
    assert_eq!(selected.primary.dir, PathBuf::from("one"));
    assert_eq!(selected.primary.file_base, "Title");
    assert!(selected.extras.is_empty());
}

#[test]
fn select_output_paths_non_first_keeps_extras() {
    let outputs = vec![PathBuf::from("one/Title"), PathBuf::from("two/Title")];
    let selected =
        select_output_paths(outputs, "{title}", MultiFolderStrategy::HardLink).expect("selected");
    assert_eq!(selected.primary.dir, PathBuf::from("one"));
    assert_eq!(selected.primary.file_base, "Title");
    assert_eq!(selected.extras.len(), 1);
    assert_eq!(selected.extras[0].dir, PathBuf::from("two"));
    assert_eq!(selected.extras[0].file_base, "Title");
}
