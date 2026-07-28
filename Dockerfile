FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /
COPY harmonica harmonica
WORKDIR /app
COPY schemas schemas
RUN wget --progress=dot:giga https://github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-x86_64-unknown-linux-gnu.tgz \
    && tar -xvf cargo-binstall-x86_64-unknown-linux-gnu.tgz \
    && cp cargo-binstall /usr/local/cargo/bin
RUN cargo binstall -y trunk && \
  rustup target add wasm32-unknown-unknown

FROM chef AS backend-planner
WORKDIR /app/redis-game
COPY redis-game .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS frontend-planner
COPY harmonica harmonica
WORKDIR /app/redis-game-front
COPY redis-game-front .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS backend-builder
WORKDIR /app/redis-game
COPY --from=backend-planner /app/redis-game/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY redis-game .
RUN cargo build --release

FROM chef AS frontend-builder
COPY harmonica harmonica
WORKDIR /app/redis-game-front
COPY --from=frontend-planner /app/redis-game-front/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json --target wasm32-unknown-unknown
# Build application
COPY redis-game-front .
RUN trunk build --release --public-url /redis-game

# We do not need the Rust toolchain to run the binary!
FROM debian:trixie-slim AS runtime
WORKDIR /app
COPY --from=backend-builder /app/redis-game/target/release/redis-game /usr/local/bin
COPY --from=frontend-builder /app/redis-game-front/dist dist/redis-game
ENTRYPOINT ["/usr/local/bin/redis-game"]
