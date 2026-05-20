mod components;
mod systems;
mod network;

use bevy_ecs::prelude::*;
use bevy_ecs::schedule::Schedule;
use tracing::info;
use tracing_subscriber;

use components::*;
use systems::movement::movement_system;
use systems::spawning::{spawn_system, SpawnConfig, SpawnTimer};
use systems::shooting::shooting_system;
use systems::collision::{bullet_collision_system, bullet_lifetime_system};
use systems::ai::ai_movement_system;
use network::server::WtServer;
use network::protocol::{Snapshot, EntityDelta, EntityType};

#[derive(bevy_ecs::resource::Resource)]
struct GameTime {
    elapsed: std::time::Duration,
    delta: std::time::Duration,
    last_tick: std::time::Instant,
}

impl GameTime {
    fn new() -> Self {
        Self {
            elapsed: std::time::Duration::ZERO,
            delta: std::time::Duration::ZERO,
            last_tick: std::time::Instant::now(),
        }
    }

    fn tick(&mut self) {
        let now = std::time::Instant::now();
        self.delta = now.duration_since(self.last_tick);
        self.elapsed += self.delta;
        self.last_tick = now;
    }

    fn elapsed_secs_f64(&self) -> f64 {
        self.elapsed.as_secs_f64()
    }

    fn delta_secs(&self) -> f32 {
        self.delta.as_secs_f32()
    }
}

fn build_snapshot(world: &mut World, tick: u32) -> Snapshot {
    let mut entities = Vec::new();

    let mut ship_query = world.query::<(Entity, &Position, &Ship, &Health)>();
    for (entity, pos, ship, health) in ship_query.iter(world) {
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

    let mut bullet_query = world.query::<(Entity, &Position, &Bullet)>();
    for (entity, pos, bullet) in bullet_query.iter(world) {
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

    let mut base_query = world.query::<(Entity, &Position, &Base, &Health)>();
    for (entity, pos, base, health) in base_query.iter(world) {
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

    Snapshot { tick, entities }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    info!("Starting Space AI vs AI Simulation Server");

    let (wt_server, _snapshot_rx) = WtServer::new();
    if let Err(e) = wt_server.start(4433).await {
        tracing::error!("Failed to start WebTransport server: {}", e);
        return;
    }

    let mut world = World::new();

    world.insert_resource(SpawnConfig::default());
    world.insert_resource(SpawnTimer::default());
    world.insert_resource(systems::shooting::BulletConfig::default());
    world.insert_resource(systems::collision::CollisionConfig::default());
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

    let tick_rate = 30.0;
    let tick_duration = std::time::Duration::from_secs_f64(1.0 / tick_rate);
    let mut tick_count: u32 = 0;
    let wt_server = wt_server;

    info!("Starting simulation at {} Hz", tick_rate);

    loop {
        let tick_start = std::time::Instant::now();

        {
            let mut game_time = world.resource_mut::<GameTime>();
            game_time.tick();
        }

        schedule.run(&mut world);

        tick_count += 1;

        if tick_count % 100 == 0 {
            let entity_count = world.entities().len();
            info!("Tick {} | Entities: {}", tick_count, entity_count);

            let mut base_query2 = world.query::<(Entity, &Position, &Base, &Health)>();
            for (entity, pos, base, health) in base_query2.iter(&world) {
                if base.team == Team::Enemy {
                    info!("  Enemy base: id_bits={} x={} health={}/{}", entity.to_bits(), pos.x, health.current, health.max);
                }
            }
        }

        let snapshot = build_snapshot(&mut world, tick_count);

        if tick_count <= 3 {
            for e in &snapshot.entities {
                if matches!(e.entity_type, network::protocol::EntityType::Base) {
                    info!("Base entity id={} team={:?} health={:?} max_health={:?} x={}", e.id, e.team, e.health, e.max_health, e.x);
                }
            }
        }

        wt_server.push_snapshot(snapshot).await;

        let mut base_query = world.query::<(&Base, &Health)>();
        let bases: Vec<_> = base_query.iter(&world).collect();
        let player_alive = bases.iter().any(|(b, h)| b.team == Team::Player && !h.is_dead());
        let enemy_alive = bases.iter().any(|(b, h)| b.team == Team::Enemy && !h.is_dead());

        if !player_alive {
            info!("Enemy team wins!");
            break;
        }
        if !enemy_alive {
            info!("Player team wins!");
            break;
        }

        let elapsed = tick_start.elapsed();
        if elapsed < tick_duration {
            tokio::time::sleep(tick_duration - elapsed).await;
        }
    }

    info!("Simulation ended after {} ticks", tick_count);
}
