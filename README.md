# Redis Game

A neat little game to showcase the capabilities of Redis

- Click your name to gain points
- Click other people's names to make them lose points
- Hold Ctrl and click your friends to help them
- Press shift to use powerups when the bar is full

## Building

- Download <https://trunkrs.dev/>
- Download <https://rustup.rs/>

```shell
cd redis-game-front
trunk build --release
cd ..
cd redis-game
cargo build --release
```

## Running

1. Compile the web frontend with Trunk

```shell
cd redis-game-front
trunk build --release
```

2. Move the resulting `dist/` directory in the same directory you're running the `redis-game` binary

3. Run a Redis binary on your server and run the `redis-game` binary on that same server

```shell
cd redis-game-front
# The working directory should have the `dist/` directory in it
cargo run --release --manifest-path ../redis-game/Cargo.toml
```
