use weevil_lua::{LuaPlugin, check_script};

#[test]
fn check_requires_return_value() {
    let script = "local a = 1";
    let err = check_script(script).expect_err("should fail");
    assert!(err.to_string().contains("did not return"));
}

#[test]
fn check_requires_trusted_urls() {
    let script = "return { run = function() return nil end }";
    let err = check_script(script).expect_err("should fail");
    assert!(err.to_string().contains("trusted_urls"));
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
fn http_blocks_untrusted_urls() {
    let script = r#"
return {
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
fn from_str_requires_run() {
    let script = "return { trusted_urls = {} }";
    let err = LuaPlugin::from_str(script).err().expect("should fail");
    assert!(err.to_string().contains("run function"));
}

#[test]
fn run_can_return_none() {
    let script = r#"
return {
  trusted_urls = {},
  run = function() return nil end
}
"#;
    let plugin = LuaPlugin::from_str(script).expect("load plugin");
    let result = plugin.call(()).expect("run plugin");
    assert!(result.is_none());
}
