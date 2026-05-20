# STRIKE 2.5D — Protocol Specification

## Connection

Connect via **WebTransport** to `https://localhost:4433`.

The server uses a self-signed certificate. Fetch the SHA-256 hash from `http://localhost:4434/cert-hash` and pass it as `serverCertificateHashes` when connecting.

## Message Transport

### Client → Server (Commands)

Open a **bidirectional stream**, write a JSON message, then close the stream.

```
[JSON bytes] → close writable
```

### Server → Client (Events)

The server opens **unidirectional streams** to send messages. Each stream contains:

```
[4 bytes: payload length (u32 LE)] [1 byte: type prefix] [payload]
```

Type prefixes:
- `0x43` (`'C'`) — Control message (JSON)
- `0x53` (`'S'`) — Snapshot (binary, see below)

## Client → Server Messages

All messages are JSON with a `"type"` field.

### Lobby

```json
{"type": "ListRooms"}
```

```json
{"type": "CreateRoom", "mode": "HumanVsHuman"}
// mode: "HumanVsHuman" | "HumanVsAI" | "AIVsAI"
```

```json
{"type": "JoinRoom", "room_id": "abc123", "role": "Player"}
// role: "Player" | "Spectator"
```

```json
{"type": "LeaveRoom"}
```

```json
{"type": "PlayAgain"}
```

### Player Commands (Human)

```json
{"type": "Command", "command": {"type": "SelectNextShip"}}
{"type": "Command", "command": {"type": "SelectShip", "ship_id": 42}}
{"type": "Command", "command": {"type": "Move", "dx": 1.0, "dy": 0.0}}
{"type": "Command", "command": {"type": "StopMove"}}
{"type": "Command", "command": {"type": "Aim", "dx": 0.7, "dy": 0.7}}
{"type": "Command", "command": {"type": "Shoot"}}
{"type": "Command", "command": {"type": "ToggleAutoFire"}}
{"type": "Command", "command": {"type": "SetSpawnType", "ship_type": "Scout"}}
// ship_type: "Scout" | "Tank" | "Sniper"
```

### Bot Commands (Per-Ship)

Control any ship on your team directly by ID:

```json
{"type": "Command", "command": {"type": "MoveShip", "ship_id": 42, "dx": 0.5, "dy": 0.8}}
{"type": "Command", "command": {"type": "ShootFrom", "ship_id": 42, "dx": 1.0, "dy": 0.0}}
```

- `ship_id` — the entity ID from the snapshot
- `dx`, `dy` — direction (will be normalized by the server)
- Ships you command are excluded from the built-in AI
- You can send multiple commands per tick (one per ship)

## Server → Client Messages

JSON with a `"type"` field.

```json
{"type": "RoomList", "rooms": [{"id": "abc123", "mode": "HumanVsAI", "state": "Waiting", "players": 1, "spectators": 0}]}
{"type": "RoomCreated", "room_id": "abc123", "team": 0}
{"type": "RoomJoined", "room_id": "abc123", "team": 1, "role": "Player"}
{"type": "JoinError", "reason": "Room is full"}
{"type": "GameStarted", "mode": "HumanVsAI"}
{"type": "ShipSelected", "ship_id": 42}
{"type": "NoShipAvailable"}
{"type": "GameOver", "winner_team": 0}
{"type": "Error", "message": "..."}
```

- `team`: 0 = Blue (Player1), 1 = Red (Player2), 255 = Spectator

## Snapshot Binary Format

Sent at 30 Hz to all clients in a playing room.

```
Header:
  [4 bytes] tick (u32 LE)
  [4 bytes] entity_count (u32 LE)

Per entity:
  [4 bytes] id (u32 LE)
  [4 bytes] x (f32 LE)
  [4 bytes] y (f32 LE)
  [4 bytes] z (f32 LE)
  [1 byte]  entity_type (0=Ship, 1=Bullet, 2=Base)
  [1 byte]  team (0=Blue, 1=Red, 0xFF=none)
  [1 byte]  has_health (0 or 1)
  [4 bytes] health (f32 LE) — only if has_health=1
  [1 byte]  has_max_health (0 or 1)
  [4 bytes] max_health (f32 LE) — only if has_max_health=1
```

Entity size: 18 bytes minimum, 26 bytes with health+max_health.

## Game Rules

- Arena: ~3000 units wide (bases at x=-1500 and x=+1500)
- Tick rate: 30 Hz
- Ships spawn near their base every 2 seconds (max 50 per team)
- Ship types:
  - Scout: 50 HP, fire rate 0.3s, damage 8
  - Tank: 150 HP, fire rate 0.6s, damage 15
  - Sniper: 75 HP, fire rate 1.2s, damage 30
- Bullets: speed 800, lifetime 3s, range ~2400 units
- Base HP: 10,000
- Ship speed (bot-controlled): 200 units/s
- Game ends when a base reaches 0 HP

## Writing a Bot

1. Connect via WebTransport to the server
2. Send `CreateRoom` with mode `"HumanVsAI"` or join an existing room
3. Wait for `GameStarted` message
4. Each tick, receive a snapshot with all entity positions
5. Parse the snapshot, decide what to do
6. Send `MoveShip` and `ShootFrom` commands for your ships
7. Repeat until `GameOver`

Your ships are identified by `team` in the snapshot matching your assigned team (from `RoomCreated`/`RoomJoined`). Use the `id` field from the snapshot as `ship_id` in commands.
