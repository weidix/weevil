//! Lua runtime bindings for weevil.

mod error;
mod http;
mod lua;
mod plugin;

pub use crate::error::LuaPluginError;
pub use crate::http::{HttpClient, HttpRequestOptions, TrustedUrl};
pub use crate::lua::{HttpMode, install_module};
pub use crate::plugin::{
    LuaPlugin, LuaPluginSpec, check_script, dedupe_script_paths_by_alias, script_alias,
    script_alias_file, script_uses_only_async_http, script_uses_only_async_http_file,
};
