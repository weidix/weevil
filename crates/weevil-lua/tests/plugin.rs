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
fn http_rejects_non_string_header_name() {
    let script = r#"
return {
  trusted_urls = { "https://example.com/" },
  run = function()
    return weevil.http.get_with_headers("https://example.com/", { [1] = "value" })
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
  trusted_urls = { "https://example.com/" },
  run = function()
    return weevil.http.get_with_headers("https://example.com/", { ["user-agent"] = 123 })
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

#[test]
fn json_encode_decode_roundtrip() {
    let script = r#"
return {
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
