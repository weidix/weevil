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
