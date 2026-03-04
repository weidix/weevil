# syntax=docker/dockerfile:1.7

FROM --platform=$BUILDPLATFORM rust:1.92-bookworm AS builder

ARG TARGETARCH

RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        ca-certificates \
        clang \
        cmake \
        curl \
        make \
        perl \
        pkg-config \
        xz-utils \
    && rm -rf /var/lib/apt/lists/*

RUN set -eux; \
    ZIG_VERSION=0.13.0; \
    case "$(dpkg --print-architecture)" in \
        amd64) ZIG_ARCH=x86_64 ;; \
        arm64) ZIG_ARCH=aarch64 ;; \
        *) echo "unsupported build architecture" && exit 1 ;; \
    esac; \
    curl -fsSL "https://ziglang.org/download/${ZIG_VERSION}/zig-linux-${ZIG_ARCH}-${ZIG_VERSION}.tar.xz" \
      | tar -xJ -C /opt; \
    ln -sf "/opt/zig-linux-${ZIG_ARCH}-${ZIG_VERSION}/zig" /usr/local/bin/zig; \
    cargo install --locked cargo-zigbuild

WORKDIR /workspace

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN case "${TARGETARCH}" in \
        amd64) RUST_TARGET=x86_64-unknown-linux-musl ;; \
        arm64) RUST_TARGET=aarch64-unknown-linux-musl ;; \
        *) echo "unsupported TARGETARCH: ${TARGETARCH}" && exit 1 ;; \
    esac \
    && rustup target add "${RUST_TARGET}" \
    && cargo zigbuild --release -p weevil-app --target "${RUST_TARGET}" \
    && install -D "target/${RUST_TARGET}/release/weevil" /out/weevil

FROM scratch AS runtime

COPY --from=builder /out/weevil /usr/local/bin/weevil
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
WORKDIR /app
USER 65532:65532

ENTRYPOINT ["/usr/local/bin/weevil"]
CMD ["--config", "/app/weevil.toml", "watch"]
