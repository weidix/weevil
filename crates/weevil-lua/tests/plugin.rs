use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, Once, OnceLock};

use tracing_subscriber::prelude::*;
use weevil_lua::{LuaPlugin, check_script, script_alias, script_uses_only_async_http};

struct FieldLayer {
    records: Arc<Mutex<Vec<HashMap<String, String>>>>,
}

impl<S> tracing_subscriber::Layer<S> for FieldLayer
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if event.metadata().target() != "weevil.lua" {
            return;
        }
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);
        if !visitor.fields.is_empty() {
            let mut records = self.records.lock().expect("log lock");
            records.push(visitor.fields);
        }
    }
}

#[derive(Default)]
struct FieldVisitor {
    fields: HashMap<String, String>,
}

impl tracing::field::Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        self.record(field, format!("{value:?}"));
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.record(field, value.to_string());
    }
}

impl FieldVisitor {
    fn record(&mut self, field: &tracing::field::Field, value: String) {
        let name = field.name();
        if name != "task_id" && name != "task_type" {
            return;
        }
        let cleaned = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(&value)
            .to_string();
        self.fields.insert(name.to_string(), cleaned);
    }
}

static LOG_RECORDS: OnceLock<Arc<Mutex<Vec<HashMap<String, String>>>>> = OnceLock::new();
static INIT_TRACING: Once = Once::new();

fn init_test_tracing() {
    INIT_TRACING.call_once(|| {
        let records = Arc::new(Mutex::new(Vec::new()));
        LOG_RECORDS.set(records.clone()).expect("set log records");
        let layer = FieldLayer { records };
        let subscriber = tracing_subscriber::registry().with(layer);
        tracing::subscriber::set_global_default(subscriber).expect("set subscriber");
    });
}

fn drain_logs() -> Vec<HashMap<String, String>> {
    let Some(records) = LOG_RECORDS.get() else {
        return Vec::new();
    };
    let mut guard = records.lock().expect("log lock");
    let snapshot = guard.clone();
    guard.clear();
    snapshot
}

#[test]
fn check_requires_return_value() {
    let script = "local a = 1";
    let err = check_script(script).expect_err("should fail");
    assert!(err.to_string().contains("did not return"));
}

#[test]
fn check_requires_trusted_urls() {
    let script = "return { alias = \"demo.alias\", run = function() return nil end }";
    let err = check_script(script).expect_err("should fail");
    assert!(err.to_string().contains("trusted_urls"));
}

#[test]
fn check_requires_alias() {
    let script = "return { trusted_urls = {}, run = function() return nil end }";
    let err = check_script(script).expect_err("should fail");
    assert!(err.to_string().contains("missing alias"));
}

#[test]
fn check_rejects_empty_alias() {
    let script = "return { alias = \"   \", trusted_urls = {}, run = function() return nil end }";
    let err = check_script(script).expect_err("should fail");
    assert!(err.to_string().contains("alias cannot be empty"));
}

#[test]
fn script_alias_reads_alias_value() {
    let script =
        "return { alias = \"provider.demo\", trusted_urls = {}, run = function() return nil end }";
    let alias = script_alias(script).expect("alias");
    assert_eq!(alias, "provider.demo");
}

#[test]
fn check_rejects_non_table_return() {
    let script = "return 1";
    let err = check_script(script).expect_err("should fail");
    assert!(err.to_string().contains("expected a table"));
}

#[test]
fn lua_can_use_core_features() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = {},
  run = function()
    local tree = weevil.html.parse("<div id='hero'><span class='title'>Hello</span></div>")
    local selector = weevil.selector.parse('div#hero > span.title')
    local node = selector:select_one(tree)
    if not node then return nil end
    return tree:text(node)
  end
}
"#;
    let plugin = LuaPlugin::from_str(script).expect("load plugin");
    let result = plugin.call(()).expect("run plugin");
    let value = result.expect("missing value");
    let text = match value {
        mlua::Value::String(value) => value.to_str().expect("utf8").to_string(),
        _ => panic!("expected string"),
    };
    assert_eq!(text, "Hello");
}

#[test]
fn lua_logs_include_task_context() {
    init_test_tracing();
    drain_logs();
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = {},
  run = function()
    weevil.log.info("start", 1)
    weevil.log.warn("slow response")
    return "ok"
  end
}
"#;
    let plugin = LuaPlugin::from_str(script).expect("load plugin");
    plugin.set_log_context("task-123", "demo");
    let result = plugin.call(()).expect("run plugin");
    let value = result.expect("missing value");
    let text = match value {
        mlua::Value::String(value) => value.to_str().expect("utf8").to_string(),
        _ => panic!("expected string"),
    };
    assert_eq!(text, "ok");
    let logs = drain_logs();
    assert!(logs.iter().any(|entry| {
        entry
            .get("task_id")
            .is_some_and(|value| value == "task-123")
    }));
    assert!(
        logs.iter()
            .any(|entry| entry.get("task_type").is_some_and(|value| value == "demo"))
    );
}

#[test]
fn http_blocks_untrusted_urls() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = { "https://allowed.example/" },
  run = function()
    return weevil.http.get("https://blocked.example/")
  end
}
"#;
    let plugin = LuaPlugin::from_str(&script).expect("load plugin");
    let err = plugin.call(()).expect_err("should fail");
    assert!(err.to_string().contains("trusted list"));
}

#[test]
fn http_post_blocks_untrusted_urls() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = { "https://allowed.example/" },
  run = function()
    return weevil.http.post("https://blocked.example/", "{}")
  end
}
"#;
    let plugin = LuaPlugin::from_str(&script).expect("load plugin");
    let err = plugin.call(()).expect_err("should fail");
    assert!(err.to_string().contains("trusted list"));
}

#[test]
fn http_rejects_non_string_header_name() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = { "https://example.com/" },
  run = function()
    return weevil.http.get("https://example.com/", { headers = { [1] = "value" } })
  end
}
"#;
    let plugin = LuaPlugin::from_str(&script).expect("load plugin");
    let err = plugin.call(()).expect_err("should fail");
    assert!(
        err.to_string()
            .contains("HTTP header name must be a string")
    );
}

#[test]
fn http_post_rejects_non_string_header_name() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = { "https://example.com/" },
  run = function()
    return weevil.http.post("https://example.com/", "{}", { headers = { [1] = "value" } })
  end
}
"#;
    let plugin = LuaPlugin::from_str(&script).expect("load plugin");
    let err = plugin.call(()).expect_err("should fail");
    assert!(
        err.to_string()
            .contains("HTTP header name must be a string")
    );
}

#[test]
fn http_rejects_non_string_header_value() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = { "https://example.com/" },
  run = function()
    return weevil.http.get("https://example.com/", { headers = { ["user-agent"] = 123 } })
  end
}
"#;
    let plugin = LuaPlugin::from_str(&script).expect("load plugin");
    let err = plugin.call(()).expect_err("should fail");
    assert!(
        err.to_string()
            .contains("HTTP header user-agent must be a string value")
    );
}

#[test]
fn http_rejects_non_string_version() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = { "https://example.com/" },
  run = function()
    return weevil.http.get("https://example.com/", { version = 2 })
  end
}
"#;
    let plugin = LuaPlugin::from_str(&script).expect("load plugin");
    let err = plugin.call(()).expect_err("should fail");
    assert!(err.to_string().contains("HTTP version must be a string"));
}

#[test]
fn http_post_rejects_non_string_version() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = { "https://example.com/" },
  run = function()
    return weevil.http.post("https://example.com/", "{}", { version = 2 })
  end
}
"#;
    let plugin = LuaPlugin::from_str(&script).expect("load plugin");
    let err = plugin.call(()).expect_err("should fail");
    assert!(err.to_string().contains("HTTP version must be a string"));
}

#[test]
fn http_rejects_unsupported_version() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = { "https://example.com/" },
  run = function()
    return weevil.http.get("https://example.com/", { version = "3" })
  end
}
"#;
    let plugin = LuaPlugin::from_str(&script).expect("load plugin");
    let err = plugin.call(()).expect_err("should fail");
    assert!(err.to_string().contains("HTTP version 3 is not supported"));
}

#[test]
fn from_str_requires_run() {
    let script = "return { alias = \"demo.alias\", trusted_urls = {} }";
    let err = LuaPlugin::from_str(script).err().expect("should fail");
    assert!(err.to_string().contains("run function"));
}

#[test]
fn run_can_return_none() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = {},
  run = function() return nil end
}
"#;
    let plugin = LuaPlugin::from_str(script).expect("load plugin");
    let result = plugin.call(()).expect("run plugin");
    assert!(result.is_none());
}

#[test]
fn json_encode_decode_roundtrip() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = {},
  run = function()
    local input = {
      name = "Neo",
      active = true,
      age = 33,
      tags = { "a", "b" },
      none = weevil.json.null,
    }
    local encoded = weevil.json.encode(input)
    local decoded = weevil.json.decode(encoded)
    return {
      encoded = encoded,
      name = decoded.name,
      active = decoded.active,
      age = decoded.age,
      tag = decoded.tags[2],
      is_null = (decoded.none == weevil.json.null),
    }
  end
}
"#;
    let plugin = LuaPlugin::from_str(script).expect("load plugin");
    let result = plugin.call(()).expect("run plugin").expect("missing value");
    let table = match result {
        mlua::Value::Table(table) => table,
        _ => panic!("expected table"),
    };
    let encoded: String = table.get("encoded").expect("encoded");
    let name: String = table.get("name").expect("name");
    let active: bool = table.get("active").expect("active");
    let age: i64 = table.get("age").expect("age");
    let tag: String = table.get("tag").expect("tag");
    let is_null: bool = table.get("is_null").expect("is_null");
    assert!(!encoded.is_empty());
    assert_eq!(name, "Neo");
    assert!(active);
    assert_eq!(age, 33);
    assert_eq!(tag, "b");
    assert!(is_null);
}

#[test]
fn json_decode_preserves_null_in_arrays() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = {},
  run = function()
    local obj = weevil.json.decode('{"list":[1,2,null],"flag":false}')
    return {
      len = #obj.list,
      second = obj.list[2],
      null_ok = (obj.list[3] == weevil.json.null),
      flag = obj.flag,
    }
  end
}
"#;
    let plugin = LuaPlugin::from_str(script).expect("load plugin");
    let result = plugin.call(()).expect("run plugin").expect("missing value");
    let table = match result {
        mlua::Value::Table(table) => table,
        _ => panic!("expected table"),
    };
    let len: i64 = table.get("len").expect("len");
    let second: i64 = table.get("second").expect("second");
    let null_ok: bool = table.get("null_ok").expect("null_ok");
    let flag: bool = table.get("flag").expect("flag");
    assert_eq!(len, 3);
    assert_eq!(second, 2);
    assert!(null_ok);
    assert!(!flag);
}

#[test]
fn json_encode_rejects_functions() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = {},
  run = function()
    return weevil.json.encode(function() end)
  end
}
"#;
    let plugin = LuaPlugin::from_str(script).expect("load plugin");
    let err = plugin.call(()).expect_err("should fail");
    assert!(err.to_string().contains("unsupported Lua value function"));
}

#[test]
fn async_http_preflight_accepts_async_calls() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = { "https://example.com/" },
  run = function()
    return weevil.http.get_async("https://example.com/")
  end
}
"#;
    let ok = script_uses_only_async_http(script).expect("check script");
    assert!(ok);
}

#[test]
fn async_http_preflight_rejects_blocking_get() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = { "https://example.com/" },
  run = function()
    return weevil.http.get("https://example.com/")
  end
}
"#;
    let ok = script_uses_only_async_http(script).expect("check script");
    assert!(!ok);
}

#[test]
fn async_http_preflight_rejects_blocking_post() {
    let script = r#"
return {
  alias = "test.alias",
  trusted_urls = { "https://example.com/" },
  run = function()
    return weevil.http.post("https://example.com/", "{}")
  end
}
"#;
    let ok = script_uses_only_async_http(script).expect("check script");
    assert!(!ok);
}
