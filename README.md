# weevil

Command-line toolkit for scraping NFO metadata for movies or other video resources. This is not a
complete, out-of-the-box application; scripting runtimes (for example Lua) supply rules and the
actual scraping behavior.

## weevil-core

`weevil-core` provides the HTML tree, selector parsing, and query execution used by scripts.

### Quick start

```rust
use weevil_core::{HtmlTree, Query, QueryKind};

let html = r#"<div id="hero"><span class="title">Hello</span></div>"#;
let tree = HtmlTree::parse_checked(html)?;

let query = Query::parse("div#hero > span.title", QueryKind::Selector)?;
if let Some(node_id) = query.select_one(&tree)? {
    let text = tree.text_content(node_id);
    println!("{text}");
}
```

XPath example:

```rust
use weevil_core::{HtmlTree, XPath};

let html = r#"<div><span id="a"></span><span id="b"></span></div>"#;
let tree = HtmlTree::parse_checked(html)?;
let xpath = XPath::parse("//span[1]")?;

let first = xpath.select_one(&tree)?;
```

If you want to keep HTML parser issues instead of failing fast:

```rust
use weevil_core::HtmlTree;

let output = HtmlTree::parse_with_errors("<div>\u{0}</div>");
println!("errors: {}", output.errors.len());
```

### Output model

- Queries return `Vec<NodeId>` or `Option<NodeId>` for the first match.
- Use `HtmlTree::node(id)` to access a node, or `HtmlTree::index()` for fast lookups by id, tag, or
  class.
- Convenience helpers:
  - `HtmlTree::text_content(id)` aggregates descendant text nodes.
  - `HtmlTree::attr(id, name)` returns element attributes.
  - `HtmlTree::descendants(id)` and `HtmlTree::subtree(id)` iterate in document order.
  - `Query::select_one` / `Selector::first_match` return the first match.

### Supported CSS subset

- Type, universal, id, class, and attribute selectors.
- Combinators: descendant, child (`>`), adjacent sibling (`+`), general sibling (`~`).
- Selector lists with commas.
- `:is`, `:where`, and `:has` are supported; other pseudo-classes and all pseudo-elements are
  rejected.

### Supported XPath subset

- A single path expression.
- Axes: `child`, `descendant`, `descendant-or-self`, `self`, `parent`, `ancestor`,
  `ancestor-or-self`, `following-sibling`, `preceding-sibling`.
- Node tests: name tests (`*`, local name, namespace) and kind tests (`document`, `element`,
  `text`, `comment`, `processing-instruction`).
- Predicates: a single integer literal `[n]` (1-based).
- Function calls: `fn:root()` only.

Unsupported XPath constructs return a `QueryExecError` with a feature category, detailed message,
and a hint describing the supported subset.

### Error reporting

- HTML parsing returns `HtmlTree` by default; use `parse_checked` for a `Result` or
  `parse_with_errors` to keep the parsed tree plus any issues.
- CSS parsing returns `SelectorError` with line/column and an input snippet.
- XPath parsing returns `XPathError` with the span and a snippet.
