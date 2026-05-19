# Design Document: AI Arena Space Shooter

## 1. Project Overview
A high-performance, browser-based 2.5D space shooter where humans and AIs compete in real-time. The platform serves as both a competitive game and a sandbox for training, testing, and deploying reinforcement learning agents.

### Core Vision
- **AI-First**: Every game mechanic is designed to provide meaningful decision spaces for AI agents.
- **Massive Scale**: Support thousands of entities per match via a headless Rust simulation server.
- **Open Ecosystem**: Users can submit custom AI bots, train them via a provided SDK, and watch them battle in a spectator arena.

---

## 2. Architecture

### 2.1 Simulation Server (Rust)
The authoritative source of truth. Runs the game logic headlessly without graphics.
- **ECS Engine**: `bevy_ecs` or `hecs` for high-performance, data-oriented entity management.
- **Concurrency**: `Tokio` runtime to run thousands of matches in parallel across CPU cores.
- **AI Inference**: `ort` crate for ONNX runtime, allowing sub-millisecond inference for RL agents.
- **State Serialization**: Deterministic simulation with seed-based RNG for replayability.

### 2.2 Client (Browser)
A "dumb" renderer that receives state updates from the server.
- **Framework**: React + TypeScript + Vite.
- **Rendering**: WebGL (or WebGPU) point-sprite rendering for massive entity counts.
- **Networking**: WebTransport for low-latency, unreliable state updates.
- **Spectator Mode**: Interpolates server snapshots to render smooth 60fps gameplay.

### 2.3 AI SDK
A TypeScript/Python package for users to build and submit bots.
```typescript
// Example Bot SDK
import { Agent, GameState, Action } from 'strike25d-sdk';

export class MyBot extends Agent {
  onTick(state: GameState): Action {
    return {
      move: { x: 0.5, y: -0.2 },
      target: 'enemy_base',
      fire: true
    };
  }
}
```

---

## 3. Gameplay Mechanics

### 3.1 Core Loop
- **Objective**: Destroy the enemy base while defending your own.
- **Ships**: Auto-spawn based on resource economy. Players/AI control movement, targeting priority, and firing.
- **Win Condition**: Enemy base HP reaches 0.

### 3.2 AI-Driven Mechanics
To create a rich environment for AI training, the game includes:

| Mechanic | Description | AI Decision Space |
|----------|-------------|-------------------|
| **Pickups** | Damage, Fire Rate, Shield regen scattered on the map | Risk/Reward: Push for powerup or stay defensive? |
| **Resource Economy** | Ships cost resources; resources regenerate over time or via pickups | Economy: Save for a big wave vs constant pressure? |
| **Map Obstacles** | Asteroids/debris block movement and line-of-sight | Pathfinding: Navigate chokepoints, use cover |
| **Ship Classes** | Scout (fast), Tank (high HP), Sniper (long range) | Composition: Which ships to spawn and when? |
| **Fog of War** | Limited vision radius around ships/bases | Exploration: Scout enemy positions vs defend blind |

### 3.3 State Space for RL
Structured data fed to AI agents (~15-20 floats):
- Player/Ship position & velocity
- Nearest enemies (distance, angle, class)
- Nearest pickups (distance, type)
- Base health deltas
- Resource count
- Ship counts by class

---

## 4. Tech Stack

| Layer | Technology | Reason |
|-------|------------|--------|
| **Server Language** | Rust | Performance, memory safety, multi-threading |
| **Server Framework** | Axum + Tokio | Async web ecosystem, WebTransport support |
| **ECS** | `bevy_ecs` | Industry-standard, cache-friendly ECS |
| **AI Inference** | `ort` (ONNX Runtime) | Fast, cross-platform ML inference |
| **Protocol** | WebTransport | UDP-like datagrams, low latency, browser-native |
| **Serialization** | Protobuf / MessagePack | Compact binary payloads |
| **Client UI** | React + TypeScript | Fast iteration, rich ecosystem |
| **Client Rendering** | WebGL + bitECS | Proven high-performance sprite rendering |
| **AI SDK** | TypeScript / Python | Accessible to web and ML developers |

---

## 5. Netcode Architecture

### 5.1 Protocol: WebTransport
- **Unreliable Datagrams**: For high-frequency state updates (positions, velocities).
- **Reliable Streams**: For control messages (bot registration, match start/end).
- **Why not WebSockets?**: TCP head-of-line blocking causes lag spikes with high entity counts.

### 5.2 Data Flow
1. **Server Tick**: Runs ECS simulation at fixed timestep (e.g., 30Hz).
2. **Delta Compression**: Only sends changed entities to clients.
3. **Interest Management**: Sends only entities within a client's view radius.
4. **Client Interpolation**: Receives snapshots at ~20Hz, interpolates to 60fps rendering.

### 5.3 Packet Structure
```protobuf
message Snapshot {
  uint32 tick = 1;
  repeated EntityDelta entities = 2;
}

message EntityDelta {
  uint32 id = 1;
  float x = 2;
  float y = 3;
  float z = 4;
  // ... other fields
}
```

---

## 6. Development Phases

### Phase 1: Core Simulation (Rust)
- [ ] Set up Rust project with `bevy_ecs` and `tokio`
- [ ] Implement headless ECS: Position, Velocity, Health, Base, Ship
- [ ] Implement systems: Movement, Collision, Spawning
- [ ] Add WebTransport server with basic state broadcasting

### Phase 2: Client Renderer
- [ ] Set up React + Vite + WebGL renderer
- [ ] Implement WebTransport client to receive snapshots
- [ ] Add interpolation and entity rendering
- [ ] Basic HUD: Health, Score, Base status

### Phase 3: Gameplay Mechanics
- [ ] Add Pickups, Resource Economy, Ship Classes
- [ ] Implement Map Obstacles and Fog of War
- [ ] Refine collision and targeting systems

### Phase 4: AI SDK & Bot Support
- [ ] Create TypeScript/Python SDK for bot development
- [ ] Implement bot registration and action routing
- [ ] Add AI inference server support (`ort` crate)

### Phase 5: Arena & Spectator Features
- [ ] Matchmaking system
- [ ] Replay recording and playback
- [ ] Leaderboard and ELO ratings
- [ ] Spectator UI with camera controls

---

## 7. Future Considerations
- **WebAssembly Client**: Compile Rust ECS to Wasm for client-side prediction
- **Cloud Training**: Server-side parallel training pipeline for user-submitted bots
- **Modding**: Allow users to submit custom maps and game rules
- **WebGPU Renderer**: Future-proof rendering for even higher entity counts
