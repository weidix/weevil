use super::config::TranslationKey;
use super::*;
use crate::errors::AppError;
use crate::nfo::Movie;

#[test]
fn resolve_translation_config_requires_target_lang() {
    let config: TranslationConfig = toml::from_str(
        r#"
keys = ["title"]

[endpoints]
kind = "openai"
url = "https://example.invalid/v1/chat/completions"
api-key = "test-key"
model = "gpt-test"
"#,
    )
    .expect("config");
    let error = resolve_translation_config(Some(&config), None).expect_err("invalid");
    assert!(matches!(error, AppError::FetchRuntime { .. }));
}

#[test]
fn resolve_translation_config_empty_keys_disables_translation() {
    let config: TranslationConfig = toml::from_str(
        r#"
target-lang = "en"
keys = []

[endpoints]
kind = "openai"
url = "https://example.invalid/v1/chat/completions"
api-key = "test-key"
model = "gpt-test"
"#,
    )
    .expect("config");
    let resolved = resolve_translation_config(Some(&config), None).expect("resolved");
    assert!(!resolved.is_enabled());
    assert!(resolved.keys().is_empty());
}

#[test]
fn resolve_translation_config_rejects_unknown_keys() {
    let config: TranslationConfig = toml::from_str(
        r#"
target-lang = "en"
keys = ["unknown"]

[endpoints]
kind = "openai"
url = "https://example.invalid/v1/chat/completions"
api-key = "test-key"
model = "gpt-test"
"#,
    )
    .expect("config");
    let error = resolve_translation_config(Some(&config), None).expect_err("invalid");
    assert!(matches!(error, AppError::FetchRuntime { .. }));
}

#[test]
fn resolve_translation_config_accepts_google_free_endpoint() {
    let config: TranslationConfig = toml::from_str(
        r#"
target-lang = "zh-CN"
keys = ["title"]

[endpoints]
kind = "google-free"
"#,
    )
    .expect("config");
    let resolved = resolve_translation_config(Some(&config), None).expect("resolved");
    assert!(resolved.is_enabled());
    assert_eq!(resolved.keys(), &[TranslationKey::Title]);
}

#[test]
fn parse_translation_config_rejects_endpoint_array() {
    let parse_result = toml::from_str::<TranslationConfig>(
        r#"
target-lang = "en"
keys = ["title"]

[[endpoints]]
kind = "google-free"

[[endpoints]]
kind = "openai"
url = "https://example.invalid/v1/chat/completions"
api-key = "test-key"
model = "gpt-test"
"#,
    );
    assert!(parse_result.is_err());
}

#[tokio::test]
async fn translate_movie_skips_target_language_values() {
    let (endpoint, calls) = endpoints::StubEndpoint::with_prefix("T:");
    let translator = MovieTranslator::new_for_tests(
        "en",
        vec![TranslationKey::Title, TranslationKey::Plot],
        endpoint,
    );

    let mut movie = Movie {
        title: Some("This is an English sentence for detection.".to_string()),
        plot: Some("Hola, este es un texto en espanol.".to_string()),
        ..Movie::default()
    };
    let changed = translator
        .translate_movie(&mut movie)
        .await
        .expect("translated");
    assert!(changed);
    assert_eq!(
        movie.title,
        Some("This is an English sentence for detection.".to_string())
    );
    assert_eq!(
        movie.plot,
        Some("T:Hola, este es un texto en espanol.".to_string())
    );

    let recorded = calls.lock().expect("calls lock");
    assert_eq!(recorded.len(), 1);
    assert_eq!(
        recorded[0],
        vec!["Hola, este es un texto en espanol.".to_string()]
    );
}
