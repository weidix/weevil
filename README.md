# weevil

`weevil` is a script-driven toolkit for scraping metadata and generating NFO files.

It is intentionally **not** a built-in full scraper product. Source-specific behavior
(search rules, parsing, anti-bot logic, field mapping) lives in scripts (currently Lua).

## Version

- CLI (`weevil-app`) stable line: `1.0.0`
- Internal libs `weevil-core` and `weevil-lua` are still independent `0.x` lines and are not semver-stable as standalone public libraries yet.

## Quick Start (Local)

```bash
# build
cargo build -p weevil-app --release

# show help
cargo run -p weevil-app -- --help
```

Typical commands:

```bash
# single title -> one NFO
cargo run -p weevil-app -- name --name "sample title" --script ./scripts/source.lua --output ./sample.nfo

# single file flow
cargo run -p weevil-app -- file --input ./videos/sample.mp4 --script ./scripts/source.lua --output "./library/{title}"

# directory batch flow
cargo run -p weevil-app -- dir --input ./videos --script ./scripts/source.lua --output "./library/{title}"

# watch mode (continuous)
cargo run -p weevil-app -- watch --input ./incoming --script ./scripts/source.lua --output "./library/{title}"
```

## Quick Start (Docker)

The official image is published to GHCR and defaults to `watch` mode.

```bash
docker run --rm ghcr.io/weidix/weevil:v1.0.0 --help
```

Minimal mounted run:

```bash
docker run --rm \
  -v "$(pwd)/weevil.toml:/app/weevil.toml:ro" \
  -v "$(pwd)/scripts:/app/scripts:ro" \
  -v "$(pwd)/incoming:/data/incoming" \
  -v "$(pwd)/library:/data/library" \
  ghcr.io/weidix/weevil:v1.0.0
```

## Documentation / Wiki

To keep this README focused, detailed docs moved to wiki-style pages:

- [Wiki Index](docs/wiki/README.md)
- [Normal Usage Guide](docs/wiki/normal-usage.md)
- [Docker Guide](docs/wiki/docker.md)
- [Config and Modes](docs/wiki/config-reference.md)
- [Lua Script Contract](docs/wiki/lua-script-contract.md)

## Workspace Layout

- `crates/weevil-app`: CLI binary (`weevil`)
- `crates/weevil-lua`: Lua runtime bridge and `weevil.*` APIs
- `crates/weevil-core`: HTML tree, selector, XPath execution core

## Development

From workspace root:

```bash
cargo fmt
cargo check
cargo test --workspace --all-targets
```
