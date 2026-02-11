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
- Process one or multiple video files (`file` mode):
  - Call Lua script with normalized input name and source file path.
  - Generate NFO from Lua output.
  - Localize `thumb` / `fanart` image URLs into local files next to output NFO.
  - Move/rename video files.
  - Support unified migration and naming for split parts (`.part01`, `.part02`, ...).
  - Detect and move matching subtitle files with normalized language suffixes.
- Batch process a directory (`dir` mode) with optional max traversal depth.
- Continuous folder watching (`watch` mode) for newly completed video files.
- Auto-detect split video parts in the same directory in `dir` / `watch`.
- Multi-output routing by template + optional hard-link/soft-link fan-out.
- Multi-threaded fetching for `dir` / `watch` (cross-file concurrency only; no multi-script concurrency inside one file).

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
  alias = "source.example",
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

- `alias`: required unique identifier for this script source.
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

- Shared defaults: `script`/`scripts`, `output`, `input-name-rule`, `folder-multi`, `max-depth`, `multi-source`, `save-images`, `multi-source-max-sources`, `node-mapping-csv`
  - Optional nested: `[shared.source-priority]` with `images` and `details`
- Fetch scheduling defaults: `fetch-threads`, `throttle-same-script`, `script-throttle-base-ms`
- Mode defaults:
  - `[name]`: `script`/`scripts`, `output`, `multi-source`, `save-images`, `multi-source-max-sources`, `node-mapping-csv`, optional `[name.source-priority]`
  - `[file]`: `script`/`scripts`, `output`, `input-name-rule`, `folder-multi`, `multi-source`, `save-images`, `multi-source-max-sources`, `node-mapping-csv`, optional `[file.source-priority]`
  - `[dir]`: `input`, `script`/`scripts`, `output`, `input-name-rule`, `folder-multi`, `max-depth`, `fetch-threads`, `throttle-same-script`, `script-throttle-base-ms`, `multi-source`, `save-images`, `multi-source-max-sources`, `node-mapping-csv`, optional `[dir.source-priority]`
  - `[watch]`: `input`, `script`/`scripts`, `output`, `input-name-rule`, `folder-multi`, `max-depth`, `fetch-threads`, `throttle-same-script`, `script-throttle-base-ms`, `multi-source`, `save-images`, `multi-source-max-sources`, `node-mapping-csv`, optional `[watch.source-priority]`

Config loading order:

- `--config <FILE>` if provided
- otherwise `./weevil.toml` if present
- otherwise empty config (then required fields must still be satisfiable)

Example `weevil.toml`:

```toml
[shared]
scripts = ["demo_lua/source_alpha/lua/source_alpha.lua", "demo_lua/source_alpha/lua/source_alpha_mirror.lua"]
output = "./library/{title}"
input-name-rule = ["1080p,WEB-DL", "regex:\\[[^\\]]+\\]", "replace:_=> "]
folder-multi = "first"
max-depth = -1
fetch-threads = 1
throttle-same-script = false
script-throttle-base-ms = 1000
multi-source = false
save-images = false
multi-source-max-sources = 2
node-mapping-csv = "./node_mapping.csv"

[shared.source-priority]
# Optional per-field-group source priority (by script alias)
# images: poster/fanart related fields
# details: title/runtime/premiered and most textual metadata fields
images = ["source.alpha", "source.alpha.mirror"]
details = ["source.alpha"]

[name]
output = "./sample.nfo"
save-images = false

[file]
folder-multi = "first"

[dir]
input = "./videos"

[watch]
input = "./incoming"
```

### Multi-thread fetch options (`dir` / `watch`)

- `fetch-threads`
  - `1`: serial processing
  - `>1`: limited concurrency by configured worker count
  - `0`: unlimited concurrency (up to task count)
- `throttle-same-script`
  - `true`: different tasks will not execute the same script concurrently; a random delay is inserted between script runs
  - `false`: no extra script-level throttling
- `script-throttle-base-ms`
  - base random delay in milliseconds for same-script throttling
  - actual wait is roughly `base ± 100ms` (minimum 0)
  - when `base = 0`, random delay is disabled (always `0ms`)
  - when `base < 100`, delay uses absolute value (`abs(base + offset)`)

Important behavior:

- Multi-threading is file-level only (different files in parallel), not parallel scripts within one file task.
- When multi-threading is enabled (`fetch-threads = 0` or `> 1`), startup preflight rejects Lua scripts using synchronous HTTP APIs (`weevil.http.get` / `weevil.http.post`). Use async APIs (`weevil.http.get_async` / `weevil.http.post_async`) instead.

Quick mixed usage example (CLI overrides only one field):

```bash
# script/output/input come from config
# but this run overrides max-depth to 1
cargo run -p weevil-app -- dir --max-depth 1
```

### Multi-script and multi-source behavior

- `--script` can be repeated to pass multiple scripts (order matters).
- When `multi-source=false` (default): scripts run in order and stop at the first successful script.
- When `multi-source=true`: scripts run in order, successful results are merged, and fetching stops when reaching `multi-source-max-sources` (or end of scripts).
- `--multi-source` is a boolean flag (default off); passing it enables aggregation.
- `--save-images` is a boolean flag (default off); passing it enables local image saving.
- `--throttle-same-script` is a boolean flag (default off); passing it enables same-script throttling.
- `multi-source-max-sources=0` means no source-count limit.
- Aggregation merges empty fields from later sources and combines `tag` / `genre` / `actor` / `fanart.thumb` / cross-platform `ratings` without duplicate entries.
- `--node-mapping-csv` (or `node-mapping-csv` in config) applies CSV value mapping before final output.
- Mapping supports `many-to-one` rules; final dedupe uses mapped values (especially useful for `genre`, `tag`, and `actor`).
- Optional source-priority config for selected field groups:
  - `[*.source-priority].images = ["alias_a", "alias_b"]`
  - `[*.source-priority].details = ["alias_x", "alias_y"]`
  - `images` fields: `thumb`, `fanart.thumb`
  - `details` fields: `title`, `originaltitle`, `sorttitle`, `year`, `premiered`, `runtime`, `director`, `credits`, `genre`, `tag`, `plot`, `outline`, `tagline`, `ratings`, `userrating`, `uniqueid`, `studio`, `country`, `set` (`name` / `overview`), `actor`, `trailer`, `fileinfo`, `dateadded`
  - `*` can be `shared`, `name`, `file`, `dir`, `watch`
  - group-level precedence remains `mode > shared`
  - when a group list is configured (non-empty), that group only uses matched aliases from the list
  - when a group list is empty, that group uses default script order
  - if `multi-source=false` and source-priority is configured, each group uses only the first source in its selected sequence
  - if source-priority is not configured, behavior stays default script order

### Node Mapping CSV

`node-mapping-csv` expects UTF-8 CSV with 3 columns:

- `node,to,from...` (3+ columns)
- optional header row (`node,to,from...`)
- legacy header row (`node,from,to`) keeps the old order
- lines starting with `#` are ignored
- `from` values are literal; `|` has no special meaning
- multiple CSVs are allowed; later CSVs override earlier ones for the same `node` + `from`

Example:

```csv
node,to,from1,from2
genre,GenreA,from_a,from_b
tag,TagA,tag_a,tag_b
actor,ActorA,actor_a,actor_b
```

Notes:

- Node matching is case-insensitive.
- `actor` and `actor.name` both affect actor name mapping.
- After mapping, list values and actor names are deduped by mapped value.

### 1) `name`

Generate one NFO by title string.

```bash
cargo run -p weevil-app -- name \
  --name "sample title" \
  --script demo_lua/source_alpha/lua/source_alpha.lua \
  --script demo_lua/source_alpha/lua/source_alpha_mirror.lua \
  --multi-source \
  --save-images \
  --multi-source-max-sources 2 \
  --output ./sample.nfo
```

Or provide a custom config file:

```bash
cargo run -p weevil-app -- --config ./weevil.toml name --name "sample title"
```

### 2) `file`

Generate NFO from one or multiple video files, then rename/move related assets.

```bash
cargo run -p weevil-app -- file \
  --input ./videos/sample-video.mp4 \
  --input ./videos/sample-video-CD2.mp4 \
  --script demo_lua/source_alpha/lua/source_alpha.lua \
  --output "./library/{title}" \
  --input-name-rule "1080p,WEB-DL" \
  --input-name-rule "regex:\\[[^\\]]+\\]" \
  --input-name-rule "replace:_=> " \
  --folder-multi first
```

When repeated `--input` values are split parts in one invocation (for example `CD1` / `CD2`),
they are migrated as one group and renamed with `.part01`, `.part02`, ... suffixes.

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
  --max-depth -1 \
  --fetch-threads 4 \
  --throttle-same-script
```

Watch behavior today:

- Processes `Create`/`Modify`/`Remove` file events.
- Auto-groups split video parts from the same directory before processing ready files.

### 5) `scripts`

List script metadata (path, alias, trusted URL count, run availability, and duplicate-alias status).

```bash
# use explicit scripts from CLI
cargo run -p weevil-app -- scripts \
  --script ./scripts/a.lua \
  --script ./scripts/b.lua

# or load script paths from config
cargo run -p weevil-app -- --config ./weevil.toml scripts
```

Behavior:

- If `--script` is provided, list those scripts only.
- Otherwise, collect script paths from config sections (`[shared]`, `[name]`, `[file]`, `[dir]`, `[watch]`).
- Duplicate aliases are preserved in output but marked as `ignored-duplicate-alias` for later entries.
- When a duplicate alias is detected, a warning log is emitted and only the earliest script remains active.
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

### Split-part detection

Split parts are detected when filename stems end with one of the following prefixes plus an index:

- `CD1`, `CD2`, ...
- `Disc1`, `Disc2`, ...
- `Disk1`, `Disk2`, ...
- `Part1`, `Part2`, ...
- `Pt1`, `Pt2`, ...

Detected split parts are migrated with unified output suffixes `.part01`, `.part02`, ...

### Subtitle file extensions

`srt`, `ass`, `ssa`, `vtt`, `sub`, `idx`, `sup`

Subtitle matching currently supports:

- Name normalization and noise-token filtering (e.g. resolution/codec tags).
- Language suffix normalization (examples: `zh_CN -> zh-CN`, `en_US -> en-US`, `pt_br -> pt-BR`).
- Carrying extra subtitle suffix tokens (for example `forced`).
- For split-part groups:
  - Part subtitles (e.g. `Movie-CD1.zh.srt`) are mapped to the corresponding part output.
  - Group subtitles (e.g. `Movie.zh.srt`) are mapped once to the group output (without `.partXX`).

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

## Known Boundaries / Non-goals

- No built-in website adapters in the Rust binary.
- No browser automation/captcha solving pipeline in current release.
- Lua scripts own anti-bot strategy, request pacing, and field reconciliation.
