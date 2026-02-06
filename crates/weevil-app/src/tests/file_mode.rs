use super::*;

#[test]
fn subtitle_suffix_matches_language_suffix() {
    assert_eq!(
        subtitle_suffix("Movie", "Movie.zh"),
        Some(".zh-CN".to_string())
    );
    assert_eq!(subtitle_suffix("Movie", "Movie"), Some(String::new()));
    assert_eq!(subtitle_suffix("Movie", "Other"), None);
}

#[test]
fn subtitle_suffix_matches_normalized_names() {
    assert_eq!(
        subtitle_suffix("My-Movie", "my movie.ZH"),
        Some(".zh-CN".to_string())
    );
}

#[test]
fn subtitle_suffix_allows_short_name_matching() {
    assert_eq!(
        subtitle_suffix("Movie.2020.1080p", "Movie.zh"),
        Some(".zh-CN".to_string())
    );
    assert_eq!(
        subtitle_suffix("Movie.Special.Edition", "Movie"),
        Some(String::new())
    );
}

#[test]
fn subtitle_suffix_ignores_noise_tokens() {
    assert_eq!(
        subtitle_suffix("Movie.2020.1080p.BluRay.x264", "Movie.zh"),
        Some(".zh-CN".to_string())
    );
}

#[test]
fn subtitle_suffix_rejects_too_short_short_name() {
    assert_eq!(subtitle_suffix("Up.2009", "U"), None);
}

#[test]
fn subtitle_suffix_rejects_unrelated_names() {
    assert_eq!(subtitle_suffix("Movie.One", "Movie.Two.zh"), None);
    assert_eq!(subtitle_suffix("The.Room", "The.Roommate"), None);
}

#[test]
fn subtitle_suffix_normalizes_language_aliases() {
    assert_eq!(
        subtitle_suffix("Movie", "Movie.zh_CN"),
        Some(".zh-CN".to_string())
    );
    assert_eq!(
        subtitle_suffix("Movie", "Movie.ch"),
        Some(".zh-CN".to_string())
    );
    assert_eq!(
        subtitle_suffix("Movie", "Movie.zh-TW"),
        Some(".zh-TW".to_string())
    );
    assert_eq!(
        subtitle_suffix("Movie", "Movie.en_US"),
        Some(".en-US".to_string())
    );
    assert_eq!(
        subtitle_suffix("Movie", "Movie.pt_br"),
        Some(".pt-BR".to_string())
    );
}

#[test]
fn subtitle_suffix_keeps_language_and_other_suffix_parts() {
    assert_eq!(
        subtitle_suffix("Movie", "Movie.zh_CN.forced"),
        Some(".zh-CN.forced".to_string())
    );
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
