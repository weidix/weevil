# Changelog

All notable changes to this project are documented in this file.

The format is based on Keep a Changelog,
and this project follows Semantic Versioning.

## [0.1.0] - Unreleased

This is the initial release track and has not been published yet.
The section below lists the full current capability set for `0.1.0`.

### Added
- Workspace structure with three crates:
  - `weevil-app` (CLI flows)
  - `weevil-lua` (Lua runtime bridge)
  - `weevil-core` (HTML/CSS/XPath query engine)
- CLI modes:
  - `name`: generate NFO by title
  - `file`: process one video file and related assets
  - `dir`: batch process a directory
  - `watch`: continuously process newly completed files
- Input name normalization pipeline via `--input-name-rule`:
  - legacy token rule support (including single-arg CSV token list)
  - `literal:<text>`
  - `regex:<pattern>`
  - `replace:<from>=><to>`
  - `regex-replace:<pattern>=><to>`
- Output template rendering with scalar/list/actor field support and path expansion.
- Multi-output routing strategies:
  - `first`
  - `hard-link`
  - `soft-link`
- Subtitle detection and move/rename handling with language suffix normalization.
- Directory traversal with max depth controls and watch-mode retry/stability checks.
- Lua plugin contract and runtime APIs:
  - `weevil.html`, `weevil.selector`, `weevil.xpath`
  - `weevil.http`, `weevil.json`, `weevil.log`
  - trusted URL policy for HTTP access
- Core parsing/query capabilities:
  - HTML parsing (lenient/strict/with errors)
  - CSS selector execution (supported subset)
  - XPath execution (supported subset)

### Included Libraries and Versions

#### Workspace
- Set workspace package version to `0.1.0`.
- Set edition to Rust `2024`.

#### weevil-core
- Added/paralleled core parser/query dependencies:
  - `cssparser 0.36`
  - `html5ever 0.38`
  - `selectors 0.35`
  - `xot 0.31`
  - `xee-xpath-ast 0.1`
  - `precomputed-hash 0.1`
  - `rustc-hash 2.1`
- Dev dependencies:
  - `robotstxt 0.3`
  - `ureq 3`

#### weevil-lua
- Added runtime/bridge dependencies:
  - `mlua 0.11` (with `lua55`, `vendored`)
  - `reqwest 0.13` (blocking + http2 + rustls, default features disabled)
  - `tracing 0.1`
  - `url 2`
  - `thiserror 2`
- Added optional JSON dependency:
  - `serde_json 1` (feature: `json`)
- Dev dependency:
  - `tracing-subscriber 0.3`

#### weevil-app
- Added CLI/application dependencies:
  - `clap 4`
  - `fs2 0.4`
  - `mlua 0.11` (with `lua55`, `vendored`, `serde`)
  - `notify 6`
  - `quick-xml 0.37` (feature: `serialize`)
  - `serde 1` (feature: `derive`)
  - `tracing 0.1`
  - `tracing-subscriber 0.3`
  - `weevil-lua` (workspace path dependency)
- Included dependency for input-name rule engine:
  - `regex 1`
- Dev dependency:
  - `tempfile 3`

### Changed

### Notes
- This is an early-stage foundational release focused on script-driven extensibility.
