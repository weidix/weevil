use mlua::Value;

use crate::config::AppConfig;
use crate::script_info;
use crate::source_runner;

#[tokio::test]
async fn list_script_infos_from_cli_paths() {
    let dir = tempfile::tempdir().expect("temp dir");
    let first = dir.path().join("a.lua");
    let second = dir.path().join("b.lua");

    tokio::fs::write(
        &first,
        r#"return { alias = "alpha", trusted_urls = {}, run = function() return nil end }"#,
    )
    .await
    .expect("write first");
    tokio::fs::write(
        &second,
        r#"return { alias = "alpha", trusted_urls = {"https://example.com/"}, run = function() return nil end }"#,
    )
    .await
    .expect("write second");

    let config: AppConfig = toml::from_str("").expect("config");
    let infos = script_info::list_script_infos(&config, vec![first, second])
        .await
        .expect("list");
    assert_eq!(infos.len(), 2);
    assert_eq!(infos[0].alias, "alpha");
    assert!(infos[0].trusted_urls.is_empty());
    assert_eq!(infos[1].trusted_urls, vec!["https://example.com/"]);
    assert!(!infos[0].duplicate_alias_ignored);
    assert!(infos[1].duplicate_alias_ignored);
}

#[tokio::test]
async fn dedupe_script_aliases_with_warning_keeps_earliest_alias() {
    let dir = tempfile::tempdir().expect("temp dir");
    let first = dir.path().join("a.lua");
    let second = dir.path().join("b.lua");

    tokio::fs::write(
        &first,
        r#"return { alias = "same.alias", trusted_urls = {}, run = function() return nil end }"#,
    )
    .await
    .expect("write first");
    tokio::fs::write(
        &second,
        r#"return { alias = "same.alias", trusted_urls = {}, run = function() return nil end }"#,
    )
    .await
    .expect("write second");

    let deduped = super::dedupe_script_aliases_with_warning(vec![first.clone(), second])
        .await
        .expect("dedupe scripts");
    assert_eq!(deduped, vec![first]);
}

#[tokio::test]
async fn list_script_infos_from_config_paths() {
    let dir = tempfile::tempdir().expect("temp dir");
    let shared_script = dir.path().join("shared.lua");
    let file_script = dir.path().join("file.lua");

    tokio::fs::write(
        &shared_script,
        r#"return { alias = "shared.alias", trusted_urls = {}, run = function() return nil end }"#,
    )
    .await
    .expect("write shared");
    tokio::fs::write(
        &file_script,
        r#"return { alias = "file.alias", trusted_urls = {}, run = function() return nil end }"#,
    )
    .await
    .expect("write file");

    let content = format!(
        "[shared]\nscript = {:?}\n\n[file]\nscript = {:?}\n",
        shared_script, file_script
    );
    let config: AppConfig = toml::from_str(&content).expect("config");
    let infos = script_info::list_script_infos(&config, Vec::new())
        .await
        .expect("list");

    assert_eq!(infos.len(), 2);
    assert_eq!(infos[0].alias, "shared.alias");
    assert_eq!(infos[1].alias, "file.alias");
}

#[test]
fn render_nfo_from_table() {
    let lua = mlua::Lua::new();
    let value: Value = lua
        .load(r#"{ title = "Spirited Away", actor = { { name = "Chihiro", gender = "female" } } }"#)
        .eval()
        .expect("expected value");
    let xml = source_runner::render_nfo_output(Some(value), &lua).expect("expected xml");
    assert!(xml.contains("<movie>"));
    assert!(xml.contains("<title>Spirited Away</title>"));
    assert!(xml.contains("<gender>female</gender>"));
    assert!(xml.contains("\n"));
    assert!(xml.contains("\n  <title>Spirited Away</title>\n"));
}

#[test]
fn render_nfo_from_string() {
    let lua = mlua::Lua::new();
    let text = lua.create_string("<movie />").expect("expected lua string");
    let xml =
        source_runner::render_nfo_output(Some(Value::String(text)), &lua).expect("expected xml");
    assert_eq!(xml, "<movie />");
}
