use std::path::Path;

use super::*;
use tempfile::tempdir;

#[tokio::test]
async fn load_default_when_missing_file() {
    let config = AppConfig::load(Some(Path::new("not-exists-weevil.toml"))).await;
    assert!(matches!(config, Err(AppError::ConfigRead { .. })));
}

#[tokio::test]
async fn parse_name_mode_from_name_section() {
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
        .await
        .expect("expected resolved name");
    assert_eq!(resolved.scripts, vec![Path::new("scripts/name.lua")]);
    assert_eq!(resolved.output, Path::new("outputs/name.nfo"));
    assert!(!resolved.multi_source);
    assert!(!resolved.save_images);
    assert_eq!(resolved.multi_source_max_sources, 2);
}

#[tokio::test]
async fn resolve_name_mode_uses_shared_output() {
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
        .await
        .expect("expected resolved name");
    assert_eq!(resolved.scripts, vec![Path::new("scripts/shared.lua")]);
    assert_eq!(resolved.output, Path::new("outputs/name.nfo"));
}

#[tokio::test]
async fn resolve_file_mode_prefers_mode_over_shared() {
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
        .await
        .expect("expected resolved file config");
    assert_eq!(resolved.scripts, vec![Path::new("scripts/file.lua")]);
    assert_eq!(resolved.output, "library/file/{title}");
    assert_eq!(resolved.input_name_rules, vec!["regex:\\[[^\\]]+\\]"]);
    assert_eq!(resolved.folder_multi, FolderMultiStrategy::SoftLink);
    assert_eq!(resolved.fetch_threads, 1);
    assert!(!resolved.throttle_same_script);
    assert_eq!(resolved.script_throttle_base_ms, 1000);
    assert!(!resolved.multi_source);
    assert!(!resolved.save_images);
    assert_eq!(resolved.multi_source_max_sources, 2);
}

#[tokio::test]
async fn resolve_dir_mode_uses_shared_defaults() {
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
        .await
        .expect("expected resolved dir config");
    assert_eq!(resolved.input, Path::new("incoming"));
    assert_eq!(resolved.mode.scripts, vec![Path::new("scripts/shared.lua")]);
    assert_eq!(resolved.mode.output, "library/{title}");
    assert_eq!(resolved.mode.input_name_rules, vec!["replace:_=> "]);
    assert_eq!(resolved.mode.folder_multi, FolderMultiStrategy::HardLink);
    assert_eq!(resolved.mode.fetch_threads, 1);
    assert!(!resolved.mode.throttle_same_script);
    assert_eq!(resolved.mode.script_throttle_base_ms, 1000);
    assert!(!resolved.mode.save_images);
    assert_eq!(resolved.max_depth, 2);
}

#[tokio::test]
async fn resolve_watch_mode_uses_mode_depth_override() {
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
        .await
        .expect("expected resolved watch config");
    assert_eq!(resolved.input, Path::new("incoming"));
    assert_eq!(resolved.max_depth, 1);
}

#[tokio::test]
async fn resolve_dir_mode_missing_input() {
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
        .await
        .expect_err("expected missing input");
    assert!(matches!(
        err,
        AppError::ConfigMissingField {
            mode: "dir",
            field: "input"
        }
    ));
}

#[tokio::test]
async fn resolve_file_mode_missing_script() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
output = "library/{title}"
"#,
    )
    .expect("expected config");

    let err = config
        .resolve_file_mode_with(&ModeCliOverrides::default())
        .await
        .expect_err("expected missing script");
    assert!(matches!(
        err,
        AppError::ConfigMissingField {
            mode: "file",
            field: "script"
        }
    ));
}

#[tokio::test]
async fn resolve_file_mode_missing_output() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
"#,
    )
    .expect("expected config");

    let err = config
        .resolve_file_mode_with(&ModeCliOverrides::default())
        .await
        .expect_err("expected missing output");
    assert!(matches!(
        err,
        AppError::ConfigMissingField {
            mode: "file",
            field: "output"
        }
    ));
}

#[tokio::test]
async fn parse_string_or_array_input_name_rules() {
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
        .await
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
        .await
        .expect("expected resolved config");
    assert_eq!(resolved_array.input_name_rules, vec!["1080p", "WEB-DL"]);
}

#[tokio::test]
async fn resolve_file_mode_cli_overrides_config() {
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
            scripts: vec!["scripts/cli.lua".into()],
            output: Some("library/cli/{title}".to_string()),
            input_name_rules: vec!["replace:_=> ".to_string()],
            folder_multi: Some(FolderMultiStrategy::SoftLink),
            fetch_threads: Some(4),
            throttle_same_script: Some(true),
            script_throttle_base_ms: Some(1500),
            multi_source: Some(true),
            save_images: Some(true),
            multi_source_max_sources: Some(3),
            node_mapping_csv: Vec::new(),
        })
        .await
        .expect("expected resolved config");

    assert_eq!(resolved.scripts, vec![Path::new("scripts/cli.lua")]);
    assert_eq!(resolved.output, "library/cli/{title}");
    assert_eq!(resolved.input_name_rules, vec!["replace:_=> "]);
    assert_eq!(resolved.folder_multi, FolderMultiStrategy::SoftLink);
    assert_eq!(resolved.fetch_threads, 4);
    assert!(resolved.throttle_same_script);
    assert_eq!(resolved.script_throttle_base_ms, 1500);
    assert!(resolved.multi_source);
    assert!(resolved.save_images);
    assert_eq!(resolved.multi_source_max_sources, 3);
}

#[tokio::test]
async fn resolve_dir_mode_cli_overrides_config() {
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
                scripts: vec!["scripts/cli.lua".into()],
                output: Some("library/cli/{title}".to_string()),
                input_name_rules: vec![],
                folder_multi: None,
                fetch_threads: Some(2),
                throttle_same_script: Some(true),
                script_throttle_base_ms: Some(1800),
                multi_source: Some(true),
                save_images: Some(true),
                multi_source_max_sources: Some(4),
                node_mapping_csv: Vec::new(),
            },
            max_depth: Some(1),
        })
        .await
        .expect("expected resolved config");

    assert_eq!(resolved.input, Path::new("from-cli"));
    assert_eq!(resolved.mode.scripts, vec![Path::new("scripts/cli.lua")]);
    assert_eq!(resolved.mode.output, "library/cli/{title}");
    assert_eq!(resolved.mode.fetch_threads, 2);
    assert!(resolved.mode.throttle_same_script);
    assert_eq!(resolved.mode.script_throttle_base_ms, 1800);
    assert!(resolved.mode.multi_source);
    assert!(resolved.mode.save_images);
    assert_eq!(resolved.mode.multi_source_max_sources, 4);
    assert_eq!(resolved.max_depth, 1);
}

#[tokio::test]
async fn resolve_dir_mode_multithread_from_config() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "library/{title}"
fetch-threads = 6
script-throttle-base-ms = 1350

[dir]
input = "incoming"
throttle-same-script = true
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_dir_mode_with(&DirCliOverrides::default())
        .await
        .expect("expected resolved dir config");

    assert_eq!(resolved.mode.fetch_threads, 6);
    assert!(resolved.mode.throttle_same_script);
    assert_eq!(resolved.mode.script_throttle_base_ms, 1350);
    assert!(!resolved.mode.multi_source);
    assert!(!resolved.mode.save_images);
    assert_eq!(resolved.mode.multi_source_max_sources, 2);
}

#[tokio::test]
async fn resolve_name_mode_cli_overrides_config() {
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
            scripts: vec!["scripts/cli.lua".into()],
            output: Some("from-cli.nfo".into()),
            multi_source: Some(true),
            save_images: Some(true),
            multi_source_max_sources: Some(5),
            node_mapping_csv: Vec::new(),
        })
        .await
        .expect("expected resolved name");

    assert_eq!(resolved.scripts, vec![Path::new("scripts/cli.lua")]);
    assert_eq!(resolved.output, Path::new("from-cli.nfo"));
    assert!(resolved.multi_source);
    assert!(resolved.save_images);
    assert_eq!(resolved.multi_source_max_sources, 5);
}

#[tokio::test]
async fn resolve_name_mode_save_images_defaults_to_false() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "from-config.nfo"
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_name_with(&NameCliOverrides::default())
        .await
        .expect("resolved");

    assert!(!resolved.save_images);
}

#[tokio::test]
async fn resolve_file_mode_uses_scripts_list_from_config() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
scripts = ["scripts/a.lua", "scripts/b.lua"]
output = "library/{title}"
multi-source = true
save-images = true
multi-source-max-sources = 6
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_file_mode_with(&ModeCliOverrides::default())
        .await
        .expect("resolved");

    assert_eq!(
        resolved.scripts,
        vec![Path::new("scripts/a.lua"), Path::new("scripts/b.lua")]
    );
    assert!(resolved.multi_source);
    assert!(resolved.save_images);
    assert_eq!(resolved.multi_source_max_sources, 6);
}

#[tokio::test]
async fn resolve_mode_node_mapping_csv_uses_cli_over_config() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "library/{title}"
node-mapping-csv = "maps/shared.csv"

[file]
node-mapping-csv = "maps/file.csv"
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_file_mode_with(&ModeCliOverrides {
            node_mapping_csv: vec!["maps/cli.csv".into()],
            ..ModeCliOverrides::default()
        })
        .await
        .expect("resolved");

    assert_eq!(
        resolved.node_mapping_csv,
        vec![Path::new("maps/cli.csv").to_path_buf()]
    );
}

#[tokio::test]
async fn resolve_name_node_mapping_csv_uses_mode_then_shared() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "shared.nfo"
node-mapping-csv = "maps/shared.csv"

[name]
output = "name.nfo"
node-mapping-csv = "maps/name.csv"
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_name_with(&NameCliOverrides::default())
        .await
        .expect("resolved");

    assert_eq!(
        resolved.node_mapping_csv,
        vec![Path::new("maps/name.csv").to_path_buf()]
    );
}

#[tokio::test]
async fn resolve_node_mapping_csv_accepts_list() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
script = "scripts/shared.lua"
output = "shared.nfo"
node-mapping-csv = ["maps/one.csv", "maps/two.csv"]

[name]
output = "name.nfo"
"#,
    )
    .expect("expected config");

    let resolved = config
        .resolve_name_with(&NameCliOverrides::default())
        .await
        .expect("resolved");

    assert_eq!(
        resolved.node_mapping_csv,
        vec![
            Path::new("maps/one.csv").to_path_buf(),
            Path::new("maps/two.csv").to_path_buf(),
        ]
    );
}

#[tokio::test]
async fn resolve_file_mode_save_images_defaults_to_false() {
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
        .await
        .expect("resolved");

    assert!(!resolved.save_images);
}

#[tokio::test]
async fn resolve_file_mode_expands_script_glob_from_config() {
    let dir = tempdir().expect("temp dir");
    let scripts_dir = dir.path().join("scripts");
    tokio::fs::create_dir_all(scripts_dir.join("nested"))
        .await
        .expect("create scripts dir");

    let alpha = scripts_dir.join("alpha.lua");
    let beta = scripts_dir.join("nested").join("beta.lua");
    tokio::fs::write(&alpha, "return {} ")
        .await
        .expect("write alpha");
    tokio::fs::write(&beta, "return {} ")
        .await
        .expect("write beta");

    let config: AppConfig = toml::from_str(&format!(
        r#"
[shared]
scripts = ["{}/*.lua", "{}/**/*.lua"]
output = "library/{{title}}"
"#,
        scripts_dir.display(),
        scripts_dir.display()
    ))
    .expect("config");

    let resolved = config
        .resolve_file_mode_with(&ModeCliOverrides::default())
        .await
        .expect("resolve config");

    assert_eq!(resolved.scripts, vec![alpha, beta]);
}

#[tokio::test]
async fn resolve_file_mode_keeps_unmatched_glob_literal() {
    let config: AppConfig = toml::from_str(
        r#"
[shared]
scripts = "scripts/missing/*.lua"
output = "library/{title}"
"#,
    )
    .expect("config");

    let resolved = config
        .resolve_file_mode_with(&ModeCliOverrides::default())
        .await
        .expect("resolve config");

    assert_eq!(resolved.scripts, vec![Path::new("scripts/missing/*.lua")]);
}
