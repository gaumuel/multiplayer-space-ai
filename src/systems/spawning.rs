use bevy_ecs::prelude::*;
use rand::Rng;
use crate::components::{
    Position, Velocity, Health, Ship, Team, ShipClass, Base,
};
use crate::room::{Owner, PlayerSlot};
use crate::GameTime;

#[derive(Resource)]
pub struct SpawnConfig {
    pub max_ships_per_team: usize,
    pub spawn_interval: f64,
    pub arena_half_width: f32,
    pub arena_half_height: f32,
    pub base_spawn_distance: f32,
}

impl Default for SpawnConfig {
    fn default() -> Self {
        Self {
            max_ships_per_team: 50,
            spawn_interval: 2.0,
            arena_half_width: 2000.0,
            arena_half_height: 2000.0,
            base_spawn_distance: 200.0,
        }
    }
}

#[derive(Resource, Default)]
pub struct SpawnTimer {
    pub player_next: f64,
    pub enemy_next: f64,
}

pub fn spawn_system(
    mut commands: Commands,
    time: Res<GameTime>,
    mut spawn_timer: ResMut<SpawnTimer>,
    config: Res<SpawnConfig>,
    ships: Query<&Ship>,
    bases: Query<(&Base, &Position)>,
) {
    let now = time.elapsed_secs_f64();

    let player_count = ships.iter().filter(|s| s.team == Team::Player).count();
    let enemy_count = ships.iter().filter(|s| s.team == Team::Enemy).count();

    let player_base = bases.iter().find(|(b, _)| b.team == Team::Player).map(|(_, p)| *p);
    let enemy_base = bases.iter().find(|(b, _)| b.team == Team::Enemy).map(|(_, p)| *p);

    if now >= spawn_timer.player_next && player_count < config.max_ships_per_team {
        if let Some(base_pos) = player_base {
            spawn_ship(&mut commands, Team::Player, &base_pos, &config);
        }
        spawn_timer.player_next = now + config.spawn_interval;
    }

    if now >= spawn_timer.enemy_next && enemy_count < config.max_ships_per_team {
        if let Some(base_pos) = enemy_base {
            spawn_ship(&mut commands, Team::Enemy, &base_pos, &config);
        }
        spawn_timer.enemy_next = now + config.spawn_interval;
    }
}

fn spawn_ship(
    commands: &mut Commands,
    team: Team,
    base_pos: &Position,
    config: &SpawnConfig,
) {
    let mut rng = rand::thread_rng();
    let angle: f32 = rng.gen_range(0.0..std::f32::consts::TAU);
    let distance: f32 = rng.gen_range(50.0..config.base_spawn_distance);

    let x = base_pos.x + angle.cos() * distance;
    let y = base_pos.y + angle.sin() * distance;

    let classes = [ShipClass::Scout, ShipClass::Tank, ShipClass::Sniper];
    let class = classes[rng.gen_range(0..3)];

    let health = match class {
        ShipClass::Scout => 50.0,
        ShipClass::Tank => 150.0,
        ShipClass::Sniper => 75.0,
    };

    let slot = match team {
        Team::Player => PlayerSlot::Player1,
        Team::Enemy => PlayerSlot::Player2,
    };

    commands.spawn((
        Position { x, y, z: 10.0 },
        Velocity { x: 0.0, y: 0.0 },
        Health::new(health),
        Ship::new(team, class),
        Owner { slot, player_controlled: false, auto_fire: true },
    ));
}
