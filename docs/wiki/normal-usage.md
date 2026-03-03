# Normal Usage Guide

## 1. Prepare

1. Build binary:

```bash
cargo build -p weevil-app --release
```

2. Prepare config and scripts:

```bash
cp weevil.toml.example weevil.toml
```

3. Update `weevil.toml` paths (`script`/`scripts`, `input`, `output`) for your environment.

## 2. Core Modes

### name

Generate a single NFO by title.

```bash
cargo run -p weevil-app -- name \
  --name "sample title" \
  --script ./scripts/source.lua \
  --output ./sample.nfo
```

### file

Process one or multiple explicit files.

```bash
cargo run -p weevil-app -- file \
  --input ./videos/sample-01.mp4 \
  --script ./scripts/source.lua \
  --output "./library/{title}"
```

### dir

Scan directory and execute file flow per item.

```bash
cargo run -p weevil-app -- dir \
  --input ./videos \
  --script ./scripts/source.lua \
  --output "./library/{genre}/{title}" \
  --max-depth 2
```

### watch

Watch directory continuously for completed files.

```bash
cargo run -p weevil-app -- watch \
  --input ./incoming \
  --script ./scripts/source.lua \
  --output "./library/{title}" \
  --fetch-threads 4
```

## 3. Common Workflow

1. Start with `name` mode to validate script output.
2. Move to `file` mode for deterministic single-file behavior.
3. Use `dir` for one-shot batch migration.
4. Use `watch` for long-running ingestion.

## 4. Debugging Tips

- Use `--help` on each mode to confirm flags.
- Start with one script and one input before enabling multi-source.
- Keep output templates simple first, then add extra placeholders.
