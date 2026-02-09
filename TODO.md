# TODO

## weevil-app
- [x] `name` / `file` / `dir` / `watch` CLI modes.
- [x] `--input-name-rule` pipeline (legacy token, regex, replace).
- [x] Output template rendering and multi-folder strategy (`first` / `hard-link` / `soft-link`).
- [x] Video + subtitle organization (rename, move, language suffix matching).
- [x] Directory batch processing and continuous watch (stability check + retry).
- [ ] Support loading options from config file (e.g., output location).
- [ ] Support saving images to local storage.
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
