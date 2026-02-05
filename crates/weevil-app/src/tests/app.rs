use super::*;

#[test]
fn render_nfo_from_table() {
    let lua = mlua::Lua::new();
    let value: Value = lua
        .load(r#"{ title = "Spirited Away" }"#)
        .eval()
        .expect("expected value");
    let xml = render_nfo_output(Some(value), &lua).expect("expected xml");
    assert!(xml.contains("<movie>"));
    assert!(xml.contains("<title>Spirited Away</title>"));
    assert!(xml.contains("\n"));
    assert!(xml.contains("\n  <title>Spirited Away</title>\n"));
}

#[test]
fn render_nfo_from_string() {
    let lua = mlua::Lua::new();
    let text = lua.create_string("<movie />").expect("expected lua string");
    let xml = render_nfo_output(Some(Value::String(text)), &lua).expect("expected xml");
    assert_eq!(xml, "<movie />");
}
