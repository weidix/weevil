use std::path::Path;

use super::*;

#[test]
fn load_default_when_missing_file() {
    let config = AppConfig::load(Some(Path::new("/tmp/not-exists-weevil.toml")));
    assert!(matches!(config, Err(AppError::ConfigRead { .. })));
}

#[test]
fn parse_name_mode_from_name_section() {
    let config: AppConfig = toml::from_str(
        r#"
[name]
script = "scripts/name.lua"
output = "outputs/name.nfo"
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_name_with(&NameCliOverrides::default())
        .expect("expected resolved name");
    assert_eq!(resolved.script, Path::new("scripts/name.lua"));
    assert_eq!(resolved.output, Path::new("outputs/name.nfo"));
}

#[test]
fn resolve_name_mode_uses_shared_output() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "outputs/name.nfo"
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_name_with(&NameCliOverrides::default())
        .expect("expected resolved name");
    assert_eq!(resolved.script, Path::new("scripts/shared.lua"));
    assert_eq!(resolved.output, Path::new("outputs/name.nfo"));
}

#[test]
fn resolve_file_mode_prefers_mode_over_shared() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "library/{title}"
input-name-rule = ["1080p"]
folder-multi = "hard-link"

[file]
script = "scripts/file.lua"
output = "library/file/{title}"
input-name-rule = ["regex:\\[[^\\]]+\\]"]
folder-multi = "soft-link"
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_file_mode_with(&ModeCliOverrides::default())
        .expect("expected resolved file config");
    assert_eq!(resolved.script, Path::new("scripts/file.lua"));
    assert_eq!(resolved.output, "library/file/{title}");
    assert_eq!(resolved.input_name_rules, vec!["regex:\\[[^\\]]+\\]"]);
    assert_eq!(resolved.folder_multi, FolderMultiStrategy::SoftLink);
}

#[test]
fn resolve_dir_mode_uses_shared_defaults() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "library/{title}"
input-name-rule = ["replace:_=> "]
folder-multi = "hard-link"
max-depth = 2

[dir]
input = "incoming"
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_dir_mode_with(&DirCliOverrides::default())
        .expect("expected resolved dir config");
    assert_eq!(resolved.input, Path::new("incoming"));
    assert_eq!(resolved.mode.script, Path::new("scripts/shared.lua"));
    assert_eq!(resolved.mode.output, "library/{title}");
    assert_eq!(resolved.mode.input_name_rules, vec!["replace:_=> "]);
    assert_eq!(resolved.mode.folder_multi, FolderMultiStrategy::HardLink);
    assert_eq!(resolved.max_depth, 2);
}

#[test]
fn resolve_watch_mode_uses_mode_depth_override() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "library/{title}"
max-depth = 3

[watch]
input = "incoming"
max-depth = 1
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_watch_mode_with(&DirCliOverrides::default())
        .expect("expected resolved watch config");
    assert_eq!(resolved.input, Path::new("incoming"));
    assert_eq!(resolved.max_depth, 1);
}

#[test]
fn resolve_dir_mode_missing_input() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "library/{title}"
"#,
    )
    .expect("expected config");

    let err = config
        .resolve_dir_mode_with(&DirCliOverrides::default())
        .expect_err("expected missing input");
    assert!(matches!(
        err,
        AppError::ConfigMissingField {
            mode: "dir",
            field: "input"
        }
    ));
}

#[test]
fn resolve_file_mode_missing_script() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
output = "library/{title}"
"#,
    )
    .expect("expected config");

    let err = config
        .resolve_file_mode_with(&ModeCliOverrides::default())
        .expect_err("expected missing script");
    assert!(matches!(
        err,
        AppError::ConfigMissingField {
            mode: "file",
            field: "script"
        }
    ));
}

#[test]
fn resolve_file_mode_missing_output() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
"#,
    )
    .expect("expected config");

    let err = config
        .resolve_file_mode_with(&ModeCliOverrides::default())
        .expect_err("expected missing output");
    assert!(matches!(
        err,
        AppError::ConfigMissingField {
            mode: "file",
            field: "output"
        }
    ));
}

#[test]
fn parse_string_or_array_input_name_rules() {
    let config_string: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "library/{title}"
input-name-rule = "1080p,WEB-DL"
"#,
    )
    .expect("expected config");

    let resolved_string = config_string
        .resolve_file_mode_with(&ModeCliOverrides::default())
        .expect("expected resolved config");
    assert_eq!(resolved_string.input_name_rules, vec!["1080p,WEB-DL"]);

    let config_array: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "library/{title}"
input-name-rule = ["1080p", "WEB-DL"]
"#,
    )
    .expect("expected config");

    let resolved_array = config_array
        .resolve_file_mode_with(&ModeCliOverrides::default())
        .expect("expected resolved config");
    assert_eq!(resolved_array.input_name_rules, vec!["1080p", "WEB-DL"]);
}

#[test]
fn resolve_file_mode_cli_overrides_config() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "library/{title}"
input-name-rule = ["1080p"]
folder-multi = "hard-link"
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_file_mode_with(&ModeCliOverrides {
            script: Some("scripts/cli.lua".into()),
            output: Some("library/cli/{title}".to_string()),
            input_name_rules: vec!["replace:_=> ".to_string()],
            folder_multi: Some(FolderMultiStrategy::SoftLink),
        })
        .expect("expected resolved config");

    assert_eq!(resolved.script, Path::new("scripts/cli.lua"));
    assert_eq!(resolved.output, "library/cli/{title}");
    assert_eq!(resolved.input_name_rules, vec!["replace:_=> "]);
    assert_eq!(resolved.folder_multi, FolderMultiStrategy::SoftLink);
}

#[test]
fn resolve_dir_mode_cli_overrides_config() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "library/{title}"
max-depth = 3

[dir]
input = "from-config"
max-depth = 2
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_dir_mode_with(&DirCliOverrides {
            input: Some("from-cli".into()),
            mode: ModeCliOverrides {
                script: Some("scripts/cli.lua".into()),
                output: Some("library/cli/{title}".to_string()),
                input_name_rules: vec![],
                folder_multi: None,
            },
            max_depth: Some(1),
        })
        .expect("expected resolved config");

    assert_eq!(resolved.input, Path::new("from-cli"));
    assert_eq!(resolved.mode.script, Path::new("scripts/cli.lua"));
    assert_eq!(resolved.mode.output, "library/cli/{title}");
    assert_eq!(resolved.max_depth, 1);
}

#[test]
fn resolve_name_mode_cli_overrides_config() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "from-config.nfo"

[name]
output = "from-name-config.nfo"
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_name_with(&NameCliOverrides {
            script: Some("scripts/cli.lua".into()),
            output: Some("from-cli.nfo".into()),
        })
        .expect("expected resolved name");

    assert_eq!(resolved.script, Path::new("scripts/cli.lua"));
    assert_eq!(resolved.output, Path::new("from-cli.nfo"));
}
