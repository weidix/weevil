# weevil

`weevil` is a script-driven toolkit for scraping metadata and producing NFO files for videos.

It is intentionally **not** a fully built-in scraper product. Site-specific logic (search rules,
parsing, anti-bot handling, field mapping) is provided by scripts, currently via Lua.

## Workspace Layout

- `crates/weevil-app`: CLI binary (`weevil`) for name/file/dir/watch flows.
- `crates/weevil-lua`: Lua runtime bridge (`weevil.*` APIs, trusted HTTP client, logging, JSON).
- `crates/weevil-core`: HTML tree, CSS selector engine, XPath subset executor.

## Current Capabilities (2026)

- Generate NFO by title (`name` mode).
- Process a single video file (`file` mode):
  - Call Lua script with normalized input name and source file path.
  - Generate NFO from Lua output.
  - Localize `thumb` / `fanart` image URLs into local files next to output NFO.
  - Move/rename video file.
  - Detect and move matching subtitle files with normalized language suffixes.
- Batch process a directory (`dir` mode) with optional max traversal depth.
- Continuous folder watching (`watch` mode) for newly completed video files.
- Multi-output routing by template + optional hard-link/soft-link fan-out.

## Build and Run

From workspace root:

```bash
cargo build -p weevil-app --release
cargo run -p weevil-app -- --help
```

Show subcommand help:

```bash
cargo run -p weevil-app -- file --help
cargo run -p weevil-app -- dir --help
cargo run -p weevil-app -- watch --help
```

## Lua Script Contract

A script must return a table:

```lua
return {
  trusted_urls = {
    "https://example.com/"
  },
  run = function(...)
    -- return a Lua table matching NFO movie schema
    -- OR return a raw XML string
  end
}
```

### Required fields

- `trusted_urls`: required (can be an empty array).
- `run`: required function.

### `run` arguments by mode

- `name`: `run(name)`
- `file` / `dir` / `watch`: `run(input_name, input_path)`

`input_name` is the file stem after applying configured `input-name-rule` rules.

`input-name-rule` supports both legacy and rule-based syntax:

- Legacy rule token: `1080p` (or `1080p,WEB-DL` in one argument).
- Literal remove: `literal:UNCUT`.
- Regex remove: `regex:\\[[^\\]]+\\]`.
- Literal replace: `replace:_=> `.
- Regex replace: `regex-replace:\\s+=> `.

Rules are applied in order and can be configured as a single string or a string array.

### `run` return value

- Lua `table`: decoded into NFO movie schema and serialized as XML.
- Lua `string`: treated as raw NFO XML.
- `nil`: treated as an error in CLI flows.

Reference schema type: `crates/weevil-app/src/nfo.rs`.

## CLI Modes

## Config-File Option Policy

CLI and config can be used together. Precedence is:

- **CLI args > mode config > shared config**

In other words, for the same field:

- CLI flag wins immediately
- if CLI flag is missing, use mode section (`[name]` / `[file]` / `[dir]` / `[watch]`)
- if mode section is missing, fallback to `[shared]`

CLI supports full options for each mode; config provides reusable defaults.

Reusable defaults live in config (`weevil.toml`):

- Shared defaults: `script`, `output`, `input-name-rule`, `folder-multi`, `max-depth`
- Mode defaults:
  - `[name]`: `script`, `output`
  - `[file]`: `script`, `output`, `input-name-rule`, `folder-multi`
  - `[dir]`: `input`, `script`, `output`, `input-name-rule`, `folder-multi`, `max-depth`
  - `[watch]`: `input`, `script`, `output`, `input-name-rule`, `folder-multi`, `max-depth`

Config loading order:

- `--config <FILE>` if provided
- otherwise `./weevil.toml` if present
- otherwise empty config (then required fields must still be satisfiable)

Example `weevil.toml`:

```toml
[shared]
script = "demo_lua/source_alpha/lua/source_alpha.lua"
output = "./library/{title}"
input-name-rule = ["1080p,WEB-DL", "regex:\\[[^\\]]+\\]", "replace:_=> "]
folder-multi = "first"
max-depth = -1

[name]
output = "./sample.nfo"

[file]
folder-multi = "first"

[dir]
input = "./videos"

[watch]
input = "./incoming"
```

Quick mixed usage example (CLI overrides only one field):

```bash
# script/output/input come from config
# but this run overrides max-depth to 1
cargo run -p weevil-app -- dir --max-depth 1
```

### 1) `name`

Generate one NFO by title string.

```bash
cargo run -p weevil-app -- name \
  --name "sample title" \
  --script demo_lua/source_alpha/lua/source_alpha.lua \
  --output ./sample.nfo
```

Or provide a custom config file:

```bash
cargo run -p weevil-app -- --config ./weevil.toml name --name "sample title"
```

### 2) `file`

Generate NFO from one video file, then rename/move related assets.

```bash
cargo run -p weevil-app -- file \
  --input ./videos/sample-video.mp4 \
  --script demo_lua/source_alpha/lua/source_alpha.lua \
  --output "./library/{title}" \
  --input-name-rule "1080p,WEB-DL" \
  --input-name-rule "regex:\\[[^\\]]+\\]" \
  --input-name-rule "replace:_=> " \
  --folder-multi first
```

### 3) `dir`

Scan directory video files and run `file` flow per file.

```bash
cargo run -p weevil-app -- dir \
  --input ./videos \
  --script demo_lua/source_alpha/lua/source_alpha.lua \
  --output "./library/{genre}/{title}" \
  --max-depth 2
```

`max-depth = -1` in config means unlimited traversal.

### 4) `watch`

Watch directory continuously and process newly completed videos.

```bash
cargo run -p weevil-app -- watch \
  --input ./incoming \
  --script demo_lua/source_alpha/lua/source_alpha.lua \
  --output "./library/{title}" \
  --max-depth -1
```

Watch behavior today:

- Processes `Create`/`Modify`/`Remove` file events.
- Waits for file size stability and lock availability before processing.
- Retries failed items with backoff.

## Output Template Rules (`output` in config)

`output` is a path template **without extension**.

### Supported fields

- Scalars:
  - `{title}` `{originaltitle}` `{sorttitle}`
  - `{year}` `{premiered}` `{runtime}` `{director}` `{studio}`
  - `{tagline}` `{plot}` `{outline}` `{fileinfo}` `{trailer}` `{dateadded}`
  - `{userrating}`
  - `{set.name}` `{set.overview}`
  - `{uniqueid}` `{uniqueid.<type>}`
- Lists:
  - `{genre}` `{tag}` `{country}` `{credits}`
- Actor fields:
  - `{actor}` / `{actor.name}` / `{actor.gender}` / `{actor.role}` / `{actor.order}`
  - Filter examples:
    - `{actor[gender=female]}`
    - `{actor.role[order=1]}`
    - `{actor[name='Alice',gender=female]}`

### Expansion behavior

- List and actor fields can expand to multiple output paths (cartesian expansion where applicable).
- `folder-multi` controls extra paths:
  - `first`: keep only first path.
  - `hard-link`: keep first as real files, link extras via hard links.
  - `soft-link`: keep first as real files, link extras via symlinks.

### Path sanitization

- Illegal path chars (`/ \ : * ? " < > |`) are replaced with `_`.
- Repeated whitespace is collapsed.
- Empty, `.` and `..` path segments are rejected.

## Video and Subtitle Handling

### Video file extensions

`mkv`, `mp4`, `avi`, `mov`, `m4v`, `wmv`, `flv`, `webm`, `ts`, `m2ts`, `mts`, `mpg`, `mpeg`

### Subtitle file extensions

`srt`, `ass`, `ssa`, `vtt`, `sub`, `idx`, `sup`

Subtitle matching currently supports:

- Name normalization and noise-token filtering (e.g. resolution/codec tags).
- Language suffix normalization (examples: `zh_CN -> zh-CN`, `en_US -> en-US`, `pt_br -> pt-BR`).
- Carrying extra subtitle suffix tokens (for example `forced`).

## Image Localization

- For NFO `thumb` and `fanart.thumb` fields, only remote `https` URLs are downloaded to local files in the primary output folder (`http` is rejected).
- After a local image file exists, reruns reuse that local file and do not fetch the same remote URL again.
- In `hard-link` / `soft-link` multi-folder mode, localized images are linked into extra output folders along with video/NFO/subtitles.

## Lua Runtime API (`weevil-lua`)

Available in script as global `weevil`:

- `weevil.html`
  - `parse`, `parse_checked`, `parse_with_errors`
  - byte variants: `parse_bytes`, `parse_bytes_checked`, `parse_bytes_with_errors`
- `weevil.selector`
  - `parse(css)` -> selector object
- `weevil.xpath`
  - `parse(xpath)` -> xpath object
- selector/xpath objects
  - `find`, `find_in`, `find_first`, `find_first_in`, `select_one`, `first_match`
- `weevil.http`
  - `get(url, options?)`, `post(url, body, options?)`
  - async-enabled builds also provide `get_async`, `post_async`
  - `options.version` supports `1.1`/`2` (and aliases like `http/2`)
- `weevil.json`
  - `encode(value)`, `decode(string)`, `null`
- `weevil.log`
  - `debug`, `info`, `warn`, `error`

### Trusted URL policy

- Requests are allowed only if URL matches `trusted_urls`.
- Scheme must be `http` or `https`.
- Host must match.
- Path behavior:
  - no `*`: prefix match
  - with `*`: wildcard path segment match (does not cross `/`)

## `weevil-core` Query Capabilities

### HTML parser

- Lenient parse: `HtmlTree::parse`
- Strict parse: `HtmlTree::parse_checked`
- Parse with issue collection: `HtmlTree::parse_with_errors`
- Fast indexes by `id`, `tag`, `class`

### CSS selector support

- Type, universal, id, class, attribute selectors
- Combinators: descendant, child (`>`), adjacent sibling (`+`), general sibling (`~`)
- Selector groups with commas
- `:is`, `:where`, `:has`
- Other pseudo-classes and pseudo-elements are rejected

### XPath support (curated subset)

- Single path expression only
- Axes: `child`, `descendant`, `descendant-or-self`, `self`, `parent`, `ancestor`,
  `ancestor-or-self`, `following-sibling`, `preceding-sibling`
- Node tests: name tests + kind tests (`document`, `element`, `text`, `comment`,
  `processing-instruction`)
- Predicates:
  - integer literal `[n]` (1-based)
  - attribute existence (`[@id]`)
  - string comparisons on `@attr`, `text()`, `.`
  - boolean `and` / `or`
  - `contains()` / `starts-with()` string arguments
- Function support in path: `fn:root()`

Unsupported constructs return `QueryExecError` with feature category and hint.

## Demo Assets

- Sample Lua scripts and fixtures: `demo_lua/`
  - `demo_lua/source_alpha/lua/source_alpha.lua`
  - `demo_lua/source_beta/lua/source_beta.lua`
  - `demo_lua/source_gamma/lua/source_gamma.lua`

## Known Boundaries / Non-goals

- No built-in website adapters in the Rust binary.
- No browser automation/captcha solving pipeline in current release.
- Lua scripts own anti-bot strategy, request pacing, and field reconciliation.
