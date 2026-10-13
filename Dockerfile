# syntax=docker/dockerfile:1.7
# The server binary and the built SPA, and no data: the converter's output is Microsoft material, so it
# lives on a volume mounted at /data (`k8s/sync-data.sh` fills it) and never in an image layer.
# `.dockerignore` is an allow-list for the same reason.

# ── 1. the SPA ───────────────────────────────────────────────────────────────────────────────────
FROM node:24-bookworm-slim AS web
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN --mount=type=cache,id=npm,target=/root/.npm npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build && test -d dist

# ── 2. the binary ────────────────────────────────────────────────────────────────────────────────
FROM rust:1-bookworm AS build
WORKDIR /src

# All of `crates/`, not just the server's dependencies: the workspace is `crates/*`, so every member's
# manifest must be present, and copying the lot means a new member or a new `[[bin]]` cannot break it.
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

# The binary is copied out inside this RUN because `target/` is a cache mount, absent from the image.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,sharing=locked \
    cargo build --release --locked -p encarta-server --bin encarta-server \
    && cp target/release/encarta-server /usr/local/bin/encarta-server

# ── 3. the image ─────────────────────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

# curl is for the HEALTHCHECK only. `k8s/` relies on uid 10001.
RUN apt-get update && apt-get install -y --no-install-recommends curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --no-create-home --home-dir /app --uid 10001 --user-group encarta \
    && mkdir -p /data

COPY --from=build /usr/local/bin/encarta-server /usr/local/bin/encarta-server
COPY --from=web /web/dist /app/web

# Filled in by CI. Below the COPYs, so a new commit does not invalidate the build stages.
ARG SOURCE_URL=""
ARG GIT_REVISION=""
ARG BUILD_DATE=""
LABEL org.opencontainers.image.source="$SOURCE_URL" \
      org.opencontainers.image.revision="$GIT_REVISION" \
      org.opencontainers.image.created="$BUILD_DATE" \
      org.opencontainers.image.description="Encarta 2009, resurrected: the content server and web client (data not included)"

# The server opens everything under ENCARTA_OUT read-only, so /data may be (and in k8s is) a
# read-only mount. With no `extracted/` beside it there is no media fallback, which is intended.
ENV ENCARTA_OUT=/data \
    ENCARTA_STATIC=/app/web \
    ENCARTA_LISTEN=0.0.0.0:8080
VOLUME /data
EXPOSE 8080

USER encarta
WORKDIR /app

HEALTHCHECK --interval=30s --timeout=3s --start-period=20s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/healthz >/dev/null || exit 1

# Exec form, so the binary is PID 1 and gets the SIGTERM that drains in-flight requests.
CMD ["encarta-server"]
