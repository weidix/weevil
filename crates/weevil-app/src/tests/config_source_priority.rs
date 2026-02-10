use super::*;

#[test]
fn resolve_file_mode_source_priority_from_mode_over_shared() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "library/{title}"

[shared.source-priority]
images = ["source.shared"]
details = ["source.shared"]

[file.source-priority]
details = ["source.file"]
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_file_mode_with(&ModeCliOverrides::default())
        .expect("resolved");

    assert_eq!(resolved.source_priority.details(), ["source.file"]);
    assert_eq!(resolved.source_priority.images(), ["source.shared"]);
}

#[test]
fn resolve_name_mode_source_priority_from_shared() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "from-config.nfo"

[shared.source-priority]
images = ["source.images"]
details = ["source.details"]
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_name_with(&NameCliOverrides::default())
        .expect("resolved");

    assert_eq!(resolved.source_priority.details(), ["source.details"]);
    assert_eq!(resolved.source_priority.images(), ["source.images"]);
}

#[test]
fn resolve_file_mode_source_priority_defaults_empty() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "library/{title}"
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_file_mode_with(&ModeCliOverrides::default())
        .expect("resolved");

    assert!(resolved.source_priority.details().is_empty());
    assert!(resolved.source_priority.images().is_empty());
}
