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

## Game Mode Combinations

Any combination of slot types is supported:

| Blue (Slot 0) | Red (Slot 1) | Setup Instructions |
|---|---|---|
| Human | Human | Both open browser, one creates PvP room, other joins via room list, click Start |
| Human | Built-in AI | Create room, leave red slot empty, click Start (auto-fills with AI) |
| Human | WASM AI | Create room, upload .wasm to red slot in waiting room, click Start |
| Human | Bot Client | Create room with `--wait`, bot joins with `cargo run -- <room_id>`, bot sends StartGame |
| Built-in AI | Built-in AI | Create room, leave both empty, click Start |
| WASM AI | Built-in AI | Create room, upload .wasm to blue slot, click Start |
| WASM AI | WASM AI | Create room, upload .wasm to both slots, click Start |
| Bot Client | Built-in AI | `cd bot-client && cargo run` (creates room, auto-starts, red fills with AI) |
| Bot Client | WASM AI | Bot creates room with `--wait`, browser uploads .wasm to red, click Start |
| Bot Client | Bot Client | Bot A: `cargo run -- --wait`, Bot B: `cargo run -- <room_id>` |

### Player Roles

| Role | Capabilities |
|------|-------------|
| `Player` | Full control — can command ALL ships by ID (`MoveShip`/`ShootFrom`) |
| `RestrictedPlayer` | Human-like — one ship at a time (`SelectNextShip`, `Move`, `Shoot`) |
| `Spectator` | Read-only — receives snapshots, cannot send commands |

### Starting a Human vs AI game (quickest)
```bash
cargo run                    # Terminal 1: start server
cd client && npm run dev     # Terminal 2: start client
# Open Chrome at localhost:3000, click "Play vs AI", click "Start Game"
```

### Starting a Bot vs Built-in AI game
```bash
cargo run                    # Terminal 1: start server
cd bot-client && cargo run   # Terminal 2: bot creates room and starts
# Open browser, click Refresh in lobby, click Watch to spectate
```

### Starting a Bot vs Bot game
```bash
cargo run                              # Terminal 1: server
cd bot-client && cargo run -- --wait   # Terminal 2: Bot A creates room, waits
# Note room ID from server logs (e.g., "abc123")
cd bot-client && cargo run -- abc123   # Terminal 3: Bot B joins and starts
# Open browser, Refresh, Watch to spectate
```

### Starting a WASM AI vs WASM AI game
```bash
cargo run                    # Terminal 1: start server
cd client && npm run dev     # Terminal 2: start client
# Build your AIs:
cd ai-template && cargo build --target wasm32-unknown-unknown --release
cd ai-pathfinding && cargo build --target wasm32-unknown-unknown --release
# In browser: Create room, upload one .wasm to blue, another to red, click Start
```

### Fair Tournament (RestrictedPlayer bots)

In a fair tournament, bots have the same constraints as humans — they can only control one ship at a time using `SelectNextShip`, `Move`, `Shoot`, etc. The server rejects `MoveShip`/`ShootFrom` commands.

**Fair mode** is enforced at the room level. When a room is created with `fair_mode: true`, ALL players in that room are restricted — no one can cheat by joining as a full `Player`.

**Allowed commands:** `SelectNextShip`, `SelectShip`, `Move`, `StopMove`, `Aim`, `Shoot`, `ToggleAutoFire`, `SetSpawnType`

**Blocked commands:** `MoveShip`, `ShootFrom` (server returns an error)

**Example fair bot:** See `fair-bot/` — a complete bot that cycles through ships, moves toward enemies, aims, and shoots, all within restricted constraints.

**Running Human vs Fair Bot:**
```bash
cargo run                          # Terminal 1: server
cd fair-bot && cargo run           # Terminal 2: fair bot creates room
cd client && npm run dev           # Terminal 3: client
# Browser: Refresh room list, click Join on the bot's room, click Start Game
```

**Running Fair Bot vs Fair Bot:**
```bash
cargo run                          # Terminal 1: server
cd fair-bot && cargo run           # Terminal 2: Bot A creates fair-mode room, waits
# Note room ID from server logs (e.g., "abc123")
cd fair-bot && cargo run -- abc123 # Terminal 3: Bot B joins and starts
# Browser: Refresh, Watch to spectate
```

**Creating a fair room from the browser:**
Check the "Fair mode" checkbox in the lobby before creating a room. Both players (human or bot) will be restricted.

**Creating a fair room from a bot:**
```json
{"type": "CreateRoom", "mode": "HumanVsHuman", "obstacles": true, "fair_mode": true}
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

## Obstacles

Toggleable via checkbox in the lobby. When enabled, the arena spawns 8 symmetric obstacles:
- **5 Static** — indestructible blockers
- **2 Destructible** — 300 HP, can be destroyed by bullets
- **1 Moving** — patrols back and forth, indestructible

Ships are pushed out of obstacles, bullets are destroyed on contact.

## Visual Effects

- **Base explosion** — 150-particle burst when a base is destroyed (team-colored with white/yellow core)
- 3-second delay before game over screen to watch the explosion

## Tech Stack

- **Server**: Rust, bevy_ecs 0.16, wtransport, wasmtime, tokio
- **Client**: TypeScript, React, WebGL2, Vite
- **Protocol**: WebTransport (QUIC/HTTP3), binary snapshots, JSON control messages

## TODO

> To resume development, load context from `2105-nopathfinding.json`

- [ ] AI pathfinding around obstacles (steering avoidance for built-in AI)
- [ ] Fog of war / vision radius
- [ ] Ship selection highlight (visual indicator on selected ship)
- [ ] Sound effects
- [ ] Leaderboard / match history
- [ ] Client UI for WASM upload
- [ ] Tournament mode (bracket system for AI vs AI)
- [ ] Replay system (record + playback matches)
