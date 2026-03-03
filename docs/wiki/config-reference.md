# Config and Modes

## Config Loading Order

1. `--config <FILE>` if provided
2. `./weevil.toml` if present
3. empty config

## Precedence

For overlapping fields:

- `CLI args > mode config > shared config`

## Recommended Structure

- `[shared]`: common defaults (`scripts`, `output`, `input-name-rule`, fetch behavior)
- `[name]`, `[file]`, `[dir]`, `[watch]`: mode-specific overrides

Reference template: [`weevil.toml.example`](../../weevil.toml.example)

## Important Fields

- `scripts`: one or more Lua scripts
- `output`: path template without extension
- `input-name-rule`: normalize/clean incoming names before script execution
- `fetch-threads`: concurrency for `dir`/`watch`
- `multi-source`: merge multiple script outputs
- `save-images`: localize remote images to output folder
- `node-mapping-csv`: CSV mapping before final output

## Output Template Notes

- Supports placeholders like `{title}`, `{genre}`, `{actor.name}`.
- Multi-value fields can expand into multiple output paths.
- `folder-multi` controls extra path behavior (`first`, `hard-link`, `soft-link`).

## Suggested Rollout

1. Keep `multi-source = false` first.
2. Validate output naming and folder structure.
3. Enable multi-source, mapping, translation progressively.
