# Revenant: one container. The Rust API serves the API, the sandboxed previews, and the built web app.
#
#   docker build -t revenant .
#   docker run --rm -p 8080:8080 revenant
#
# Three stages: build the web export, build the Rust binary, then copy both into a small image.

FROM node:22-slim AS web
WORKDIR /src/apps/web
RUN npm install --global pnpm@11
COPY apps/web/package.json apps/web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY apps/web ./
RUN pnpm build

FROM rust:1.95-slim-bookworm AS api
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY services ./services
RUN cargo build --release -p revenant-api

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home revenant
WORKDIR /app
COPY --from=api /src/target/release/revenant-api /app/revenant-api
COPY --from=web /src/apps/web/out /app/web
COPY fixtures /app/fixtures

ENV PORT=8080 \
    REVENANT_FIXTURES=/app/fixtures \
    REVENANT_WEB_DIR=/app/web \
    RUST_LOG=revenant_api=info

USER revenant
EXPOSE 8080
ENTRYPOINT ["/app/revenant-api"]
