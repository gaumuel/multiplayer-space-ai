# STRIKE 2.5D — AI Template

A starter template for writing your own AI in Rust that compiles to WASM.

## Quick Start

```bash
# Install the WASM target (one-time)
rustup target add wasm32-unknown-unknown

# Build your AI
cargo build --target wasm32-unknown-unknown --release

# Your .wasm file is at:
# target/wasm32-unknown-unknown/release/strike_ai_template.wasm
```

## How It Works

The game server calls your `on_tick` function 30 times per second with the current game state. You return a list of commands for your ships.

Edit the `think()` function in `src/lib.rs` to implement your strategy.

## Available Commands

- `MoveShip { ship_id, dx, dy }` — move a ship in a direction (normalized)
- `ShootFrom { ship_id, dx, dy }` — fire a bullet from a ship in a direction

## Game State

Each tick you receive:
- Your ships (id, position, health, class)
- Enemy ships (id, position, health)
- Both bases (position, health)
- Bullet positions and velocities

## Tips

- Ships move at 200 units/sec
- Bullets travel at 800 units/sec with 3s lifetime (2400 unit range)
- Bases are at x=-1500 (blue) and x=+1500 (red)
- Scout: fast fire (0.3s), low damage (8), 50 HP
- Tank: medium fire (0.6s), medium damage (15), 150 HP
- Sniper: slow fire (1.2s), high damage (30), 75 HP
