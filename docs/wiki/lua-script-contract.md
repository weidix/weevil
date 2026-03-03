# Lua Script Contract

A script must return a table:

```lua
return {
  alias = "source.example",
  trusted_urls = {
    "https://example.com/"
  },
  run = function(...)
    -- return a Lua table compatible with NFO movie schema
    -- or return raw XML string
  end
}
```

## Required Fields

- `alias`: unique source id
- `trusted_urls`: request allowlist (can be empty list)
- `run`: metadata extraction function

## run Arguments by Mode

- `name`: `run(name)`
- `file` / `dir` / `watch`: `run(input_name, input_path)`

## run Return Values

- Lua table: decode to NFO model and serialize to XML
- Lua string: treat as raw NFO XML
- `nil`: treated as error

Schema implementation lives in `crates/weevil-app/src/nfo.rs`.

## Runtime APIs (Summary)

Global object: `weevil`

- `weevil.html`
- `weevil.selector`
- `weevil.xpath`
- `weevil.http` (`get` / `post`, optional async variants)
- `weevil.browser` (when browser feature enabled)
- `weevil.json`
- `weevil.log`
