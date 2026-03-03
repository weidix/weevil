# Docker Guide

## Image

- Registry: `ghcr.io/weidix/weevil`
- Multi-arch: `linux/amd64`, `linux/arm64`
- Default container command: `watch`

## Run Existing Image

Quick check:

```bash
docker run --rm ghcr.io/weidix/weevil:v1.0.0 --help
```

Typical mounted run:

```bash
docker run --rm \
  -v "$(pwd)/weevil.toml:/app/weevil.toml:ro" \
  -v "$(pwd)/scripts:/app/scripts:ro" \
  -v "$(pwd)/incoming:/data/incoming" \
  -v "$(pwd)/library:/data/library" \
  ghcr.io/weidix/weevil:v1.0.0
```

## Override Command

Because image default is `watch`, pass an explicit subcommand when needed:

```bash
docker run --rm ghcr.io/weidix/weevil:v1.0.0 name --name "sample" --script /app/scripts/source.lua --output /data/library/sample.nfo
```

## Build Locally

```bash
docker build -t weevil:local .
```

## Verify Multi-Arch Manifest

```bash
docker buildx imagetools inspect ghcr.io/weidix/weevil:v1.0.0
```
