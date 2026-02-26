use super::parse_launch_options;
use crate::lua::browser_codec::{json_to_lua, parse_cookie_inputs};
use mlua::{Lua, Value};

#[test]
fn parse_launch_options_defaults() {
    let parsed = parse_launch_options(None).expect("default options");
    assert!(parsed.headless);
    assert_eq!(parsed.executable_path, None);
    assert!(!parsed.no_sandbox);
    assert!(parsed.args.is_empty());
}

#[test]
fn parse_launch_options_from_table() {
    let lua = Lua::new();
    let table = lua.create_table().expect("table");
    table.set("headless", false).expect("headless");
    table
        .set("executable_path", "/opt/browser/chrome")
        .expect("path");
    table.set("no_sandbox", true).expect("no_sandbox");
    let args = lua
        .create_sequence_from(["--disable-gpu", "--window-size=1200,800"])
        .expect("args");
    table.set("args", args).expect("set args");

    let parsed = parse_launch_options(Some(Value::Table(table))).expect("parsed");
    assert!(!parsed.headless);
    assert_eq!(
        parsed.executable_path.as_deref(),
        Some("/opt/browser/chrome")
    );
    assert!(parsed.no_sandbox);
    assert_eq!(
        parsed.args,
        vec![
            "--disable-gpu".to_string(),
            "--window-size=1200,800".to_string()
        ]
    );
}

#[test]
fn parse_launch_options_rejects_non_table() {
    let err = parse_launch_options(Some(Value::Boolean(true)))
        .err()
        .expect("non-table should fail");
    assert!(err.to_string().contains("browser options must be a table"));
}

#[test]
fn parse_launch_options_rejects_invalid_field_types() {
    let lua = Lua::new();
    let table = lua.create_table().expect("table");
    table.set("headless", "yes").expect("headless");
    let err = parse_launch_options(Some(Value::Table(table)))
        .err()
        .expect("invalid field should fail");
    assert!(
        err.to_string()
            .contains("browser option headless must be a boolean")
    );
}

#[test]
fn parse_launch_options_rejects_non_array_args() {
    let lua = Lua::new();
    let table = lua.create_table().expect("table");
    let args = lua.create_table().expect("args");
    args.set("first", "--disable-gpu").expect("set arg");
    table.set("args", args).expect("set args");
    let err = parse_launch_options(Some(Value::Table(table)))
        .err()
        .expect("non-array args should fail");
    assert!(
        err.to_string()
            .contains("browser option args must be an array")
    );
}

#[test]
fn parse_launch_options_rejects_non_string_arg_entry() {
    let lua = Lua::new();
    let table = lua.create_table().expect("table");
    let args = lua.create_sequence_from([1]).expect("args");
    table.set("args", args).expect("set args");
    let err = parse_launch_options(Some(Value::Table(table)))
        .err()
        .expect("non-string args should fail");
    assert!(
        err.to_string()
            .contains("browser option args entry 1 must be a string")
    );
}

#[test]
fn parse_cookie_inputs_accepts_valid_array() {
    let lua = Lua::new();
    let cookies = lua.create_table().expect("cookies");

    let first = lua.create_table().expect("cookie");
    first.set("name", "sid").expect("name");
    first.set("value", "abc").expect("value");
    first.set("domain", "example.invalid").expect("domain");
    first.set("path", "/").expect("path");
    first.set("secure", true).expect("secure");
    first.set("http_only", true).expect("http_only");
    first.set("expires", 10.5).expect("expires");
    cookies.raw_set(1, first).expect("set first");

    let parsed = parse_cookie_inputs(Value::Table(cookies)).expect("parsed cookies");
    assert_eq!(parsed.len(), 1);
    let cookie = &parsed[0];
    assert_eq!(cookie.name, "sid");
    assert_eq!(cookie.value, "abc");
    assert_eq!(cookie.domain.as_deref(), Some("example.invalid"));
    assert_eq!(cookie.path.as_deref(), Some("/"));
    assert_eq!(cookie.secure, Some(true));
    assert_eq!(cookie.http_only, Some(true));
    assert_eq!(cookie.expires, Some(10.5));
}

#[test]
fn parse_cookie_inputs_rejects_non_array_table() {
    let lua = Lua::new();
    let cookies = lua.create_table().expect("cookies");
    cookies.set("first", "oops").expect("set");
    let err = parse_cookie_inputs(Value::Table(cookies))
        .err()
        .expect("should fail");
    assert!(err.to_string().contains("browser cookies must be an array"));
}

#[test]
fn parse_cookie_inputs_rejects_missing_name() {
    let lua = Lua::new();
    let cookies = lua.create_table().expect("cookies");
    let first = lua.create_table().expect("cookie");
    first.set("value", "abc").expect("value");
    cookies.raw_set(1, first).expect("set first");

    let err = parse_cookie_inputs(Value::Table(cookies))
        .err()
        .expect("should fail");
    assert!(err.to_string().contains("missing field name"));
}

#[test]
fn json_to_lua_maps_object_and_array() {
    let lua = Lua::new();
    let value = serde_json::json!({"name":"neo","list":[1,true,null]});
    let value = json_to_lua(&lua, value).expect("convert json");
    let table = match value {
        Value::Table(table) => table,
        other => panic!("expected table, got {other:?}"),
    };
    let name: String = table.get("name").expect("name");
    assert_eq!(name, "neo");

    let list: mlua::Table = table.get("list").expect("list");
    let first: i64 = list.raw_get(1).expect("first");
    let second: bool = list.raw_get(2).expect("second");
    let third: Value = list.raw_get(3).expect("third");
    assert_eq!(first, 1);
    assert!(second);
    assert!(matches!(third, Value::Nil));
}
