# syntax=docker/dockerfile:1.7

FROM rust:1.92-bookworm AS builder

ARG TARGETARCH

RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        ca-certificates \
        clang \
        cmake \
        make \
        musl-tools \
        perl \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
    CC_x86_64_unknown_linux_musl=musl-gcc \
    CC_aarch64_unknown_linux_musl=musl-gcc

RUN case "${TARGETARCH}" in \
        amd64) RUST_TARGET=x86_64-unknown-linux-musl ;; \
        arm64) RUST_TARGET=aarch64-unknown-linux-musl ;; \
        *) echo "unsupported TARGETARCH: ${TARGETARCH}" && exit 1 ;; \
    esac \
    && rustup target add "${RUST_TARGET}" \
    && cargo build --release -p weevil-app --target "${RUST_TARGET}" \
    && install -D "target/${RUST_TARGET}/release/weevil" /out/weevil

FROM scratch AS runtime

COPY --from=builder /out/weevil /usr/local/bin/weevil
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt

ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
USER 65532:65532

ENTRYPOINT ["/usr/local/bin/weevil"]
CMD ["watch"]
