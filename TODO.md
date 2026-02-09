# TODO

## weevil-app
- [x] `name` / `file` / `dir` / `watch` CLI modes.
- [x] `--input-name-rule` pipeline (legacy token, regex, replace).
- [x] Output template rendering and multi-folder strategy (`first` / `hard-link` / `soft-link`).
- [x] Video + subtitle organization (rename, move, language suffix matching).
- [x] Directory batch processing and continuous watch (stability check + retry).
- [x] Support config-file option policy: keep invocation-unique args in CLI (e.g., `name --name`, `file --input` for file path), and persist reusable defaults in config (e.g., `dir/watch --input` for directory, plus shared `--script` / `--output` / `--input-name-rule` / `--folder-multi` / `--max-depth`).
- [x] Support saving images to local storage.
- [x] Support multi-threaded fetching.
- [ ] Support multi-source data aggregation.
- [ ] Support passing multiple input files in `file` mode for unified migration and naming of split parts.
- [ ] Support auto-detecting split video parts in the same directory in `dir` and `watch` modes, then applying unified migration and naming.
- [ ] Support `tag` and other node-name mapping.

## weevil-lua
- [x] Expose `weevil.html` / `selector` / `xpath` / `http` / `json` / `log` APIs.
- [x] Trusted URL policy and Lua plugin contract (`run(...)`).
- [ ] Add translation capability and expose it via Lua API.

## weevil-core
- [x] HTML parsing (lenient / strict / with error collection).
- [x] CSS selector execution (supported subset).
- [x] XPath execution (supported subset).
- [x] Clear errors and hints for unsupported features.
