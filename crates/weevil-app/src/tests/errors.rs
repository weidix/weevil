use std::sync::Arc;

use super::*;

#[test]
fn extract_lua_frame_prefers_lua_paths() {
    let trace = "stack traceback:\n\t/path/to/script.lua:42: in function 'run'\n\t[C]: in ?";
    let frame = extract_lua_frame(trace).expect("expected frame");
    assert_eq!(frame, "/path/to/script.lua:42:");
}

#[test]
fn format_callback_error_includes_first_frame() {
    let error = mlua::Error::CallbackError {
        traceback: "stack traceback:\n\tscript.lua:7: in function 'run'".to_string(),
        cause: Arc::new(mlua::Error::RuntimeError("boom".to_string())),
    };
    let message = format_lua_error(&error);
    assert!(message.contains("Lua traceback (first frame): script.lua:7:"));
}
