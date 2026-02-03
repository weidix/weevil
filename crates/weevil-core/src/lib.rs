//! Core HTML parsing and selector execution for weevil.
//!
//! # Quick start
//! ```rust,no_run
//! use weevil_core::{HtmlTree, Selector};
//!
//! let html = r#"<div id="hero"><span class="title">Hello</span></div>"#;
//! let tree = HtmlTree::parse_checked(html)?;
//! let selector = Selector::parse("div#hero > span.title")?;
//! let _first = selector.select_one(&tree)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! CSS and XPath support a curated subset (see README). HTML parsing is lenient by default; use
//! `HtmlTree::parse_checked` to surface parser issues or `HtmlTree::parse_with_errors` to keep the
//! tree plus any warnings.
pub mod html;
pub mod node;
pub mod query;
pub mod selector;
pub mod xpath;

pub use html::{
    Children, Descendants, HtmlIndex, HtmlParseError, HtmlParseIssue, HtmlParseOutput, HtmlTree,
    Subtree,
};
pub use node::{ElementData, Node, NodeId, NodeKind};
pub use query::{Query, QueryContext, QueryExecError, QueryExecFeature};
pub use selector::{Selector, SelectorError, SelectorErrorKind, SelectorLocation};
pub use xpath::{XPath, XPathError};
