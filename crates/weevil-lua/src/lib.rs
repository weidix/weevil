//! Lua runtime bindings for weevil.

#[cfg(feature = "browser")]
mod browser;
mod error;
mod http;
mod lua;
mod plugin;

#[cfg(feature = "browser")]
pub use crate::browser::{
    BrowserCookie, BrowserCookieInput, BrowserLaunchOptions, BrowserPage, BrowserSession,
};
pub use crate::error::LuaPluginError;
pub use crate::http::{HttpClient, HttpRequestOptions, TrustedUrl};
pub use crate::lua::{HttpMode, install_module};
pub use crate::plugin::{
    LuaPlugin, LuaPluginSpec, check_script, dedupe_script_paths_by_alias, script_alias,
    script_alias_file,
};
