use bevy_ecs::prelude::*;
use bevy_ecs::schedule::Schedule;
use crate::components::*;
use crate::systems::movement::movement_system;
use crate::systems::spawning::{spawn_system, SpawnConfig, SpawnTimer};
use crate::systems::shooting::shooting_system;
use crate::systems::collision::{bullet_collision_system, bullet_lifetime_system};
use crate::systems::ai::ai_movement_system;
use crate::network::messages::{GameMode, RoomState, SpawnShipType, ServerMessage, PlayerCommand};
use crate::network::protocol::{Snapshot, EntityDelta, EntityType};
use crate::wasm_ai::runner::WasmAiRunner;
use crate::wasm_ai::interface::{serialize_game_view, WasmCommand};
use crate::GameTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerSlot {
    Player1,
    Player2,
}

impl PlayerSlot {
    pub fn team(&self) -> Team {
        match self {
            PlayerSlot::Player1 => Team::Player,
            PlayerSlot::Player2 => Team::Enemy,
        }
    }

    pub fn team_id(&self) -> u8 {
        match self {
            PlayerSlot::Player1 => 0,
            PlayerSlot::Player2 => 1,
        }
    }
}

#[derive(Clone)]
pub enum SlotController {
    Human { client_id: usize },
    AI,
    Wasm { runner: std::sync::Arc<std::sync::Mutex<WasmAiRunner>> },
    Empty,
}

impl std::fmt::Debug for SlotController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Human { client_id } => write!(f, "Human({})", client_id),
            Self::AI => write!(f, "AI"),
            Self::Wasm { .. } => write!(f, "Wasm"),
            Self::Empty => write!(f, "Empty"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub controller: SlotController,
    pub selected_ship: Option<Entity>,
    pub next_spawn_type: ShipClass,
    pub aim_dx: f32,
    pub aim_dy: f32,
    pub auto_fire: bool,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            controller: SlotController::Empty,
            selected_ship: None,
            next_spawn_type: ShipClass::Scout,
            aim_dx: 1.0,
            aim_dy: 0.0,
            auto_fire: false,
        }
    }
}

/// Tags a ship with its owner slot and whether it's player-controlled
#[derive(Component, Debug, Clone, Copy)]
pub struct Owner {
    pub slot: PlayerSlot,
    pub player_controlled: bool,
    pub auto_fire: bool,
}

pub struct Room {
    pub id: String,
    pub mode: GameMode,
    pub state: RoomState,
    pub world: World,
    pub schedule: Schedule,
    pub players: [PlayerState; 2],
    pub spectators: Vec<usize>,
    pub tick_count: u32,
}

impl Room {
    pub fn new(id: String, mode: GameMode) -> Self {
        let mut world = World::new();

        world.insert_resource(SpawnConfig::default());
        world.insert_resource(SpawnTimer::default());
        world.insert_resource(crate::systems::shooting::BulletConfig::default());
        world.insert_resource(crate::systems::collision::CollisionConfig::default());
        world.insert_resource(GameTime::new());

        let mut schedule = Schedule::default();
        schedule.add_systems((
            ai_movement_system,
            movement_system,
            spawn_system,
            shooting_system,
            bullet_collision_system,
            bullet_lifetime_system,
        ));

        // Spawn bases
        world.spawn((
            Position { x: -1500.0, y: 0.0, z: 10.0 },
            Health::new(10000.0),
            Base { team: Team::Player },
        ));
        world.spawn((
            Position { x: 1500.0, y: 0.0, z: 10.0 },
            Health::new(10000.0),
            Base { team: Team::Enemy },
        ));

        let players = match mode {
            GameMode::AIVsAI => [
                PlayerState { controller: SlotController::AI, ..Default::default() },
                PlayerState { controller: SlotController::AI, ..Default::default() },
            ],
            GameMode::HumanVsAI => [
                PlayerState::default(),
                PlayerState { controller: SlotController::AI, ..Default::default() },
            ],
            GameMode::HumanVsHuman => [
                PlayerState::default(),
                PlayerState::default(),
            ],
        };

        Self {
            id,
            mode,
            state: RoomState::Waiting,
            world,
            schedule,
            players,
            spectators: Vec::new(),
            tick_count: 0,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.players.iter().all(|p| !matches!(p.controller, SlotController::Empty))
    }

    pub fn start(&mut self) {
        self.state = RoomState::Playing;
    }

    pub fn tick(&mut self) {
        if self.state != RoomState::Playing {
            return;
        }

        {
            let mut game_time = self.world.resource_mut::<GameTime>();
            game_time.tick();
        }

        // Run WASM AI for slots that have one
        for slot_idx in 0..2 {
            let slot = if slot_idx == 0 { PlayerSlot::Player1 } else { PlayerSlot::Player2 };
            if let SlotController::Wasm { runner } = &self.players[slot_idx].controller {
                let runner = runner.clone();
                let game_state = serialize_game_view(self, slot);
                if let Ok(mut runner) = runner.lock() {
                    if let Ok(commands) = runner.tick(&game_state) {
                        let team = slot.team();
                        for cmd in commands {
                            self.apply_wasm_command(cmd, team);
                        }
                    }
                }
            }
        }

        self.schedule.run(&mut self.world);
        self.tick_count += 1;
    }

    fn apply_wasm_command(&mut self, cmd: WasmCommand, team: Team) {
        match cmd {
            WasmCommand::MoveShip { ship_id, dx, dy } => {
                if let Some(entity) = self.find_owned_ship(ship_id, team) {
                    let len = (dx * dx + dy * dy).sqrt().max(0.001);
                    let speed = 200.0;
                    if let Some(mut vel) = self.world.get_mut::<Velocity>(entity) {
                        vel.x = (dx / len) * speed;
                        vel.y = (dy / len) * speed;
                    }
                    if let Some(mut owner) = self.world.get_mut::<Owner>(entity) {
                        owner.player_controlled = true;
                    }
                }
            }
            WasmCommand::ShootFrom { ship_id, dx, dy } => {
                if let Some(entity) = self.find_owned_ship(ship_id, team) {
                    if let Some(pos) = self.world.get::<Position>(entity) {
                        let len = (dx * dx + dy * dy).sqrt().max(0.001);
                        let bullet_speed = 800.0;
                        let damage = self.world.get::<Ship>(entity).map(|s| s.damage).unwrap_or(10.0);
                        self.world.spawn((
                            Position { x: pos.x, y: pos.y, z: pos.z },
                            Velocity { x: (dx / len) * bullet_speed, y: (dy / len) * bullet_speed },
                            Bullet { team, damage, lifetime: 3.0 },
                        ));
                    }
                    if let Some(mut owner) = self.world.get_mut::<Owner>(entity) {
                        owner.player_controlled = true;
                    }
                }
            }
            WasmCommand::SetSpawnType { ship_type } => {
                // Find the slot index for this team
                let slot_idx = if team == Team::Player { 0 } else { 1 };
                self.players[slot_idx].next_spawn_type = match ship_type {
                    0 => ShipClass::Scout,
                    1 => ShipClass::Tank,
                    _ => ShipClass::Sniper,
                };
            }
        }
    }

    pub fn check_game_over(&mut self) -> Option<u8> {
        let mut base_query = self.world.query::<(&Base, &Health)>();
        let bases: Vec<_> = base_query.iter(&self.world).collect();

        let player_alive = bases.iter().any(|(b, h)| b.team == Team::Player && !h.is_dead());
        let enemy_alive = bases.iter().any(|(b, h)| b.team == Team::Enemy && !h.is_dead());

        if !player_alive {
            Some(1)
        } else if !enemy_alive {
            Some(0)
        } else {
            None
        }
    }

    pub fn join_as_player(&mut self, client_id: usize) -> Option<PlayerSlot> {
        for (i, player) in self.players.iter_mut().enumerate() {
            if matches!(player.controller, SlotController::Empty) {
                player.controller = SlotController::Human { client_id };
                let slot = if i == 0 { PlayerSlot::Player1 } else { PlayerSlot::Player2 };
                return Some(slot);
            }
        }
        None
    }

    pub fn join_as_spectator(&mut self, client_id: usize) {
        if !self.spectators.contains(&client_id) {
            self.spectators.push(client_id);
        }
    }

    pub fn remove_client(&mut self, client_id: usize) {
        for player in &mut self.players {
            if matches!(player.controller, SlotController::Human { client_id: id } if id == client_id) {
                player.controller = SlotController::Empty;
                player.selected_ship = None;
            }
        }
        self.spectators.retain(|&id| id != client_id);
    }

    /// Reset the room for a new game, keeping players in their slots
    pub fn reset(&mut self) {
        self.world = bevy_ecs::world::World::new();
        self.world.insert_resource(SpawnConfig::default());
        self.world.insert_resource(SpawnTimer::default());
        self.world.insert_resource(crate::systems::shooting::BulletConfig::default());
        self.world.insert_resource(crate::systems::collision::CollisionConfig::default());
        self.world.insert_resource(GameTime::new());

        self.schedule = Schedule::default();
        self.schedule.add_systems((
            ai_movement_system,
            movement_system,
            spawn_system,
            shooting_system,
            bullet_collision_system,
            bullet_lifetime_system,
        ));

        self.world.spawn((
            Position { x: -1500.0, y: 0.0, z: 10.0 },
            Health::new(10000.0),
            Base { team: Team::Player },
        ));
        self.world.spawn((
            Position { x: 1500.0, y: 0.0, z: 10.0 },
            Health::new(10000.0),
            Base { team: Team::Enemy },
        ));

        for player in &mut self.players {
            player.selected_ship = None;
            player.auto_fire = false;
        }

        self.tick_count = 0;
        self.state = RoomState::Playing;
    }

    #[allow(dead_code)]
    pub fn get_player_slot(&self, client_id: usize) -> Option<PlayerSlot> {
        for (i, player) in self.players.iter().enumerate() {
            if matches!(player.controller, SlotController::Human { client_id: id } if id == client_id) {
                return Some(if i == 0 { PlayerSlot::Player1 } else { PlayerSlot::Player2 });
            }
        }
        None
    }

    pub fn all_client_ids(&self) -> Vec<usize> {
        let mut ids = Vec::new();
        for player in &self.players {
            if let SlotController::Human { client_id } = player.controller {
                ids.push(client_id);
            }
        }
        ids.extend_from_slice(&self.spectators);
        ids
    }

    pub fn build_snapshot(&mut self) -> Snapshot {
        let mut entities = Vec::new();

        let mut ship_query = self.world.query::<(Entity, &Position, &Ship, &Health)>();
        for (entity, pos, ship, health) in ship_query.iter(&self.world) {
            entities.push(EntityDelta {
                id: entity.to_bits() as u32,
                x: pos.x,
                y: pos.y,
                z: pos.z,
                entity_type: EntityType::Ship,
                team: Some(match ship.team {
                    Team::Player => 0,
                    Team::Enemy => 1,
                }),
                health: Some(health.current),
                max_health: Some(health.max),
            });
        }

        let mut bullet_query = self.world.query::<(Entity, &Position, &Bullet)>();
        for (entity, pos, bullet) in bullet_query.iter(&self.world) {
            entities.push(EntityDelta {
                id: entity.to_bits() as u32,
                x: pos.x,
                y: pos.y,
                z: pos.z,
                entity_type: EntityType::Bullet,
                team: Some(match bullet.team {
                    Team::Player => 0,
                    Team::Enemy => 1,
                }),
                health: None,
                max_health: None,
            });
        }

        let mut base_query = self.world.query::<(Entity, &Position, &Base, &Health)>();
        for (entity, pos, base, health) in base_query.iter(&self.world) {
            entities.push(EntityDelta {
                id: entity.to_bits() as u32,
                x: pos.x,
                y: pos.y,
                z: pos.z,
                entity_type: EntityType::Base,
                team: Some(match base.team {
                    Team::Player => 0,
                    Team::Enemy => 1,
                }),
                health: Some(health.current),
                max_health: Some(health.max),
            });
        }

        Snapshot { tick: self.tick_count, entities }
    }

    /// Handle a player command, returns an optional ServerMessage response
    pub fn handle_command(&mut self, client_id: usize, command: &PlayerCommand) -> Option<ServerMessage> {
        let slot_idx = self.players.iter().position(|p| {
            matches!(p.controller, SlotController::Human { client_id: id } if id == client_id)
        })?;

        let slot = if slot_idx == 0 { PlayerSlot::Player1 } else { PlayerSlot::Player2 };
        let team = slot.team();

        match command {
            PlayerCommand::Move { dx, dy } => {
                if let Some(entity) = self.players[slot_idx].selected_ship {
                    if let Some(mut vel) = self.world.get_mut::<Velocity>(entity) {
                        let speed = 200.0;
                        vel.x = dx * speed;
                        vel.y = dy * speed;
                    }
                    // Mark as player-controlled
                    if let Some(mut owner) = self.world.get_mut::<Owner>(entity) {
                        owner.player_controlled = true;
                    }
                }
                None
            }
            PlayerCommand::StopMove => {
                if let Some(entity) = self.players[slot_idx].selected_ship {
                    if let Some(mut vel) = self.world.get_mut::<Velocity>(entity) {
                        vel.x = 0.0;
                        vel.y = 0.0;
                    }
                }
                None
            }
            PlayerCommand::Shoot => {
                if let Some(entity) = self.players[slot_idx].selected_ship {
                    if let Some(pos) = self.world.get::<Position>(entity) {
                        let aim_dx = self.players[slot_idx].aim_dx;
                        let aim_dy = self.players[slot_idx].aim_dy;
                        let bullet_speed = 800.0;
                        let ship_team = self.world.get::<Ship>(entity).map(|s| s.team).unwrap_or(team);
                        let damage = self.world.get::<Ship>(entity).map(|s| s.damage).unwrap_or(10.0);

                        tracing::info!("Shoot: entity exists, aim=({}, {}), pos=({}, {})", aim_dx, aim_dy, pos.x, pos.y);

                        self.world.spawn((
                            Position { x: pos.x, y: pos.y, z: pos.z },
                            Velocity { x: aim_dx * bullet_speed, y: aim_dy * bullet_speed },
                            Bullet { team: ship_team, damage, lifetime: 3.0 },
                        ));
                    } else {
                        tracing::info!("Shoot: entity {:?} has no Position (dead?)", entity);
                    }
                } else {
                    tracing::info!("Shoot: no selected ship");
                }
                None
            }
            PlayerCommand::Aim { dx, dy } => {
                let len = (dx * dx + dy * dy).sqrt();
                if len > 0.0 {
                    self.players[slot_idx].aim_dx = dx / len;
                    self.players[slot_idx].aim_dy = dy / len;
                }
                None
            }
            PlayerCommand::ToggleAutoFire => {
                self.players[slot_idx].auto_fire = !self.players[slot_idx].auto_fire;
                if let Some(entity) = self.players[slot_idx].selected_ship {
                    if let Some(mut owner) = self.world.get_mut::<Owner>(entity) {
                        owner.auto_fire = self.players[slot_idx].auto_fire;
                    }
                }
                None
            }
            PlayerCommand::SelectNextShip => {
                let current = self.players[slot_idx].selected_ship;
                let mut ship_query = self.world.query::<(Entity, &Ship, &Owner)>();
                let ships: Vec<Entity> = ship_query.iter(&self.world)
                    .filter(|(_, s, o)| s.team == team && o.slot == slot)
                    .map(|(e, _, _)| e)
                    .collect();

                if ships.is_empty() {
                    self.players[slot_idx].selected_ship = None;
                    return Some(ServerMessage::NoShipAvailable);
                }

                let next = match current {
                    Some(cur) => {
                        let idx = ships.iter().position(|&e| e == cur).unwrap_or(0);
                        ships[(idx + 1) % ships.len()]
                    }
                    None => ships[0],
                };

                // Deselect old ship
                if let Some(old) = current {
                    if let Some(mut owner) = self.world.get_mut::<Owner>(old) {
                        owner.player_controlled = false;
                        owner.auto_fire = true;
                    }
                }

                self.players[slot_idx].selected_ship = Some(next);
                if let Some(mut owner) = self.world.get_mut::<Owner>(next) {
                    owner.player_controlled = true;
                    owner.auto_fire = false;
                }
                self.players[slot_idx].auto_fire = false;

                Some(ServerMessage::ShipSelected { ship_id: next.to_bits() as u32 })
            }
            PlayerCommand::SelectShip { ship_id } => {
                // Find entity by bits
                let mut ship_query = self.world.query::<(Entity, &Ship, &Owner)>();
                let found = ship_query.iter(&self.world)
                    .find(|(e, s, o)| e.to_bits() as u32 == *ship_id && s.team == team && o.slot == slot)
                    .map(|(e, _, _)| e);

                if let Some(entity) = found {
                    if let Some(old) = self.players[slot_idx].selected_ship {
                        if let Some(mut owner) = self.world.get_mut::<Owner>(old) {
                            owner.player_controlled = false;
                            owner.auto_fire = true;
                        }
                    }
                    self.players[slot_idx].selected_ship = Some(entity);
                    self.players[slot_idx].auto_fire = false;
                    if let Some(mut owner) = self.world.get_mut::<Owner>(entity) {
                        owner.player_controlled = true;
                        owner.auto_fire = false;
                    }
                    Some(ServerMessage::ShipSelected { ship_id: *ship_id })
                } else {
                    None
                }
            }
            PlayerCommand::SetSpawnType { ship_type } => {
                self.players[slot_idx].next_spawn_type = match ship_type {
                    SpawnShipType::Scout => ShipClass::Scout,
                    SpawnShipType::Tank => ShipClass::Tank,
                    SpawnShipType::Sniper => ShipClass::Sniper,
                };
                None
            }
            PlayerCommand::MoveShip { ship_id, dx, dy } => {
                if let Some(entity) = self.find_owned_ship(*ship_id, team) {
                    let speed = 200.0;
                    let len = (dx * dx + dy * dy).sqrt().max(0.001);
                    if let Some(mut vel) = self.world.get_mut::<Velocity>(entity) {
                        vel.x = (dx / len) * speed;
                        vel.y = (dy / len) * speed;
                    }
                    if let Some(mut owner) = self.world.get_mut::<Owner>(entity) {
                        owner.player_controlled = true;
                    }
                }
                None
            }
            PlayerCommand::ShootFrom { ship_id, dx, dy } => {
                if let Some(entity) = self.find_owned_ship(*ship_id, team) {
                    if let Some(pos) = self.world.get::<Position>(entity) {
                        let len = (dx * dx + dy * dy).sqrt().max(0.001);
                        let ndx = dx / len;
                        let ndy = dy / len;
                        let bullet_speed = 800.0;
                        let ship_team = self.world.get::<Ship>(entity).map(|s| s.team).unwrap_or(team);
                        let damage = self.world.get::<Ship>(entity).map(|s| s.damage).unwrap_or(10.0);

                        self.world.spawn((
                            Position { x: pos.x, y: pos.y, z: pos.z },
                            Velocity { x: ndx * bullet_speed, y: ndy * bullet_speed },
                            Bullet { team: ship_team, damage, lifetime: 3.0 },
                        ));
                    }
                    if let Some(mut owner) = self.world.get_mut::<Owner>(entity) {
                        owner.player_controlled = true;
                    }
                }
                None
            }
        }
    }

    /// Find a ship entity by its encoded ID, verifying it belongs to the given team
    fn find_owned_ship(&mut self, ship_id: u32, team: Team) -> Option<Entity> {
        let mut query = self.world.query::<(Entity, &Ship, &Owner)>();
        query.iter(&self.world)
            .find(|(e, s, _)| e.to_bits() as u32 == ship_id && s.team == team)
            .map(|(e, _, _)| e)
    }
}
