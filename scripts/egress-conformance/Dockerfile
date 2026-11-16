# syntax=docker/dockerfile:1
# egress-conformance — the R6.45k real-client conformance image.
#   builder: the subject tree's hh-egress-mediator + hh-cell-relay,
#            built inside ubuntu:noble for the image arch (the task
#            images' family), pinned to the repo's rust-toolchain.
#   run:     a plain ubuntu:noble carrying the two binaries, the
#            rendered phase policies, the extracted shapes and the
#            in-container orchestrator. No podman-in-podman.
FROM ubuntu:noble AS builder
ARG RUST_CHANNEL=1.94
RUN apt-get update \
 && apt-get install -y --no-install-recommends curl ca-certificates build-essential \
 && rm -rf /var/lib/apt/lists/*
RUN curl -LsSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
ENV PATH=/root/.cargo/bin:/root/.rustup/toolchains/${RUST_CHANNEL}-*/bin:$PATH
RUN rustup toolchain install ${RUST_CHANNEL} --profile minimal
WORKDIR /src
COPY src/ /src/
# The relay builds statically linked for the matching musl target
# (AR-05); the mediator is a noble-gnu binary like the window's.
RUN set -eux; \
    arch="$(uname -m)"; \
    case "$arch" in x86_64) musl=x86_64-unknown-linux-musl;; aarch64) musl=aarch64-unknown-linux-musl;; *) exit 2;; esac; \
    rustup target add "$musl"; \
    cargo +${RUST_CHANNEL} build --release -p hharness-env --bin hh-egress-mediator; \
    cargo +${RUST_CHANNEL} build --release -p hharness-helper --bin hh-cell-relay --target "$musl"; \
    mkdir -p /opt/hh/bin; \
    cp "target/release/hh-egress-mediator" /opt/hh/bin/; \
    cp "target/${musl}/release/hh-cell-relay" /opt/hh/bin/

FROM ubuntu:noble AS run
# python3 carries walcheck.py; curl+ca-certificates the deny probes.
# pip/npm are installed BY the shapes via apt (the bootstrap itself is
# the exercised path — never pre-baked).
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates curl python3 \
 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /opt/hh/bin/ /opt/hh/bin/
COPY policies/ /opt/hh/policies/
COPY derived/ /opt/hh/derived/
COPY container-run.sh walcheck.py /opt/hh/
RUN chmod +x /opt/hh/container-run.sh
WORKDIR /app
ENTRYPOINT ["/bin/bash", "/opt/hh/container-run.sh"]
