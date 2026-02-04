//! wasm plugin runtime for weevil scraping scripts.
//!
//! # Overview
//! - Host-side runtime: load a wasm plugin and exchange JSON messages via a small ABI.
//! - Guest-side SDK: implement `Plugin` and export it with `export_plugin!`.
//! - The ABI is intentionally minimal and capability-oriented to keep plugins safe and portable.
//! - HTTP requests are executed by the host and gated by the plugin's URL whitelist.
//!
//! # Host usage
//! ```rust,no_run
//! # #[cfg(feature = "host")]
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use weevil_script::host::WasmPlugin;
//! use serde_json::Value;
//! use weevil_script::{ScrapeContext, ScrapeOutcome};
//!
//! let engine = wasmtime::Engine::default();
//! let wasm = std::fs::read("plugin.wasm")?;
//! let mut plugin = WasmPlugin::load(&engine, wasm)?;
//!
//! let mut context = ScrapeContext::new();
//! context
//!     .context
//!     .insert("start_url".to_string(), Value::String("https://example.com".to_string()));
//! let outcome = plugin.run(&context)?;
//! match outcome {
//!     ScrapeOutcome::Completed { response } => {
//!         println!("records: {}", response.records.len());
//!     }
//!     ScrapeOutcome::NeedInput { request } => {
//!         println!("need input: {}", request.prompt);
//!     }
//! }
//! # Ok(())
//! # }
//! # #[cfg(not(feature = "host"))]
//! # fn main() {}
//! ```
//!
//! # Guest usage
//! ```rust,no_run
//! # #[cfg(any(feature = "guest", target_arch = "wasm32"))]
//! # {
//! use serde_json::Value;
//! use weevil_script::{export_plugin, guest::{http, Plugin}};
//! use weevil_script::{
//!     HttpRequest, PluginDescriptor, Record, ScrapeContext, ScrapeOutcome, ScrapeResponse,
//! };
//!
//! struct Demo;
//!
//! impl Plugin for Demo {
//!     fn new() -> Self {
//!         Self
//!     }
//!
//!     fn describe(&self) -> PluginDescriptor {
//!         let mut descriptor = PluginDescriptor::new("demo", "0.1.0");
//!         descriptor
//!             .http_whitelist
//!             .push("https://example.com/*".to_string());
//!         descriptor
//!     }
//!
//!     fn scrape(
//!         &mut self,
//!         context: ScrapeContext,
//!     ) -> Result<ScrapeOutcome, weevil_script::PluginError> {
//!         let start_url = context
//!             .context
//!             .get("start_url")
//!             .and_then(|value| value.as_str())
//!             .unwrap_or("https://example.com");
//!         let http_response = http::request(HttpRequest::get(start_url))?;
//!         let mut response = ScrapeResponse::new();
//!         let mut record = Record::new("example");
//!         record
//!             .fields
//!             .insert(
//!                 "bytes".to_string(),
//!                 Value::String(http_response.body.len().to_string()),
//!             );
//!         response.records.push(record);
//!         Ok(ScrapeOutcome::Completed { response })
//!     }
//! }
//!
//! export_plugin!(Demo);
//! # }
//! ```

pub mod abi;
#[cfg(any(feature = "guest", target_arch = "wasm32", test))]
pub mod guest;
#[cfg(feature = "host")]
pub mod host;
pub mod model;

pub use model::*;

/// Re-export weevil-core for plugin authors.
pub use weevil_core as core;
