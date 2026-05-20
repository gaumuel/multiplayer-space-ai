# STRIKE 2.5D — AI Arena Space Shooter

A real-time multiplayer 2.5D space shooter where humans and AIs compete. Build your own AI bot, upload a WASM module, or play with keyboard controls.

## Game Modes

| Mode | Description |
|------|-------------|
| **Human vs AI** | Play against the built-in AI with keyboard controls |
| **Human vs Human** | Two players in the same room |
| **AI vs AI** | Watch two AIs battle (spectator mode) |
| **Custom AI** | Upload a WASM module or connect a bot client |

## Quick Start

### Server
```bash
cargo run
```
Starts the game server on WebTransport port 4433 and cert-hash HTTP on port 4434.

### Client (Browser)
```bash
cd client
npm install
npm run dev
```
Open Chrome at `http://localhost:3000`. Click "Play vs AI" to start.

### Controls
| Key | Action |
|-----|--------|
| W/A/S/E | Move selected ship |
| Mouse | Aim direction |
| Space | Shoot |
| Tab | Select next ship |
| Z | Toggle auto-fire |
| 1/2/3 | Set spawn type (Scout/Tank/Sniper) |

## Build Your Own AI

There are two ways to create a custom AI:

### Option 1: Bot Client (any language)

Write a program that connects via WebTransport and sends commands. See `bot-client/` for a Rust example.

```bash
cd bot-client
cargo run
```

Your bot receives game snapshots at 30Hz and sends `MoveShip`/`ShootFrom` commands for each of your ships. See [PROTOCOL.md](PROTOCOL.md) for the full wire format.

### Option 2: WASM Module (uploaded to server)

Write AI logic in Rust, compile to WASM, upload to the server. The server runs your module each tick. See `ai-template/` for a starter.

```bash
cd ai-template
cargo build --target wasm32-unknown-unknown --release
# Upload the .wasm file via the client
```

## Architecture

```
┌─────────────────────────────────────────────────────┐
│ Game Server (Rust)                                   │
│                                                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐          │
│  │  Room 1  │  │  Room 2  │  │  Room N  │          │
│  │ ECS World│  │ ECS World│  │ ECS World│          │
│  └──────────┘  └──────────┘  └──────────┘          │
│                                                      │
│  WebTransport (port 4433)                            │
│  Cert Hash HTTP (port 4434)                          │
└──────────────────────────────────────────────────────┘
        │                    │                │
   Browser Client      Bot Client       WASM AI
   (React + WebGL)     (any language)   (in-process)
```

- **Server**: Rust + bevy_ecs + wtransport. Runs physics, collisions, spawning at 30Hz per room.
- **Client**: React + WebGL2 + WebTransport. Renders entities as point sprites.
- **Rooms**: Each game runs in an isolated room with its own ECS world.
- **Spatial Hash**: Collision detection uses a grid for O(n) performance.

## Project Structure

```
├── src/                    # Game server
│   ├── main.rs             # Game loop, message routing
│   ├── room.rs             # Room (per-game ECS world + state machine)
│   ├── room_manager.rs     # Room lifecycle management
│   ├── components/         # ECS components (Position, Ship, Bullet, etc.)
│   ├── systems/            # ECS systems (AI, movement, shooting, collision)
│   ├── network/            # WebTransport server, protocol, messages
│   ├── spatial.rs          # Spatial hash grid for collisions
│   └── wasm_ai/            # WASM AI runtime (wasmtime)
├── client/                 # Browser client (React + WebGL2)
├── bot-client/             # Example bot client (Rust)
├── ai-template/            # Starter WASM AI template (Rust)
├── PROTOCOL.md             # Wire protocol documentation
└── design.md               # Original design document
```

## Game Rules

- Arena: 3000 units wide, bases at x=±1500
- Tick rate: 30 Hz
- Ships spawn every 2s (max 50 per team)
- **Scout**: 50 HP, 0.3s fire rate, 8 damage
- **Tank**: 150 HP, 0.6s fire rate, 15 damage
- **Sniper**: 75 HP, 1.2s fire rate, 30 damage
- Bullets: 800 speed, 3s lifetime, ~2400 range
- Base HP: 10,000
- Win condition: destroy the enemy base

## Tech Stack

- **Server**: Rust, bevy_ecs 0.16, wtransport, wasmtime, tokio
- **Client**: TypeScript, React, WebGL2, Vite
- **Protocol**: WebTransport (QUIC/HTTP3), binary snapshots, JSON control messages
