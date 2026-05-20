use bevy_ecs::prelude::*;
use rand::Rng;
use crate::components::*;
use crate::GameTime;

/// Spawn symmetric random obstacles in the arena
pub fn spawn_obstacles(world: &mut World) {
    let mut rng = rand::thread_rng();

    // Generate 4 obstacle pairs (mirrored) = 8 total
    // Layout: 5 static, 2 destructible, 1 moving (split across pairs)
    let configs: Vec<(ObstacleKind, f32)> = vec![
        (ObstacleKind::Static, rng.gen_range(60.0..120.0)),
        (ObstacleKind::Static, rng.gen_range(60.0..120.0)),
        (ObstacleKind::Destructible, rng.gen_range(50.0..90.0)),
        (ObstacleKind::Moving, rng.gen_range(70.0..100.0)),
    ];

    for (i, (kind, radius)) in configs.iter().enumerate() {
        // Generate position in the right half (x: 200..1200, y: -800..800)
        let x = rng.gen_range(200.0..1200.0);
        let y = rng.gen_range(-800.0..800.0);

        // Spawn on positive x side
        spawn_obstacle(world, x, y, *kind, *radius, i);
        // Mirror to negative x side
        spawn_obstacle(world, -x, y, *kind, *radius, i + 4);
    }

    // Add one more static pair for variety
    let radius = rng.gen_range(60.0..120.0);
    let x = rng.gen_range(400.0..1000.0);
    let y = rng.gen_range(-600.0..600.0);
    spawn_obstacle(world, x, y, ObstacleKind::Static, radius, 8);
    spawn_obstacle(world, -x, -y, ObstacleKind::Static, radius, 9);
}

fn spawn_obstacle(world: &mut World, x: f32, y: f32, kind: ObstacleKind, radius: f32, _idx: usize) {
    let mut entity = world.spawn((
        Position { x, y, z: 5.0 },
        Obstacle { kind, radius },
    ));

    if kind == ObstacleKind::Destructible {
        entity.insert(Health::new(300.0));
    }

    if kind == ObstacleKind::Moving {
        let patrol_range = 200.0;
        entity.insert(PatrolMovement {
            start_x: x,
            start_y: y - patrol_range,
            end_x: x,
            end_y: y + patrol_range,
            speed: 60.0,
            progress: 0.5,
            direction: 1.0,
        });
    }
}

/// System: move patrol obstacles back and forth
pub fn obstacle_movement_system(
    mut query: Query<(&mut Position, &mut PatrolMovement), With<Obstacle>>,
    time: Res<GameTime>,
) {
    let dt = time.delta_secs();
    for (mut pos, mut patrol) in query.iter_mut() {
        patrol.progress += patrol.direction * patrol.speed * dt / 
            ((patrol.end_x - patrol.start_x).powi(2) + (patrol.end_y - patrol.start_y).powi(2)).sqrt().max(1.0);

        if patrol.progress >= 1.0 {
            patrol.progress = 1.0;
            patrol.direction = -1.0;
        } else if patrol.progress <= 0.0 {
            patrol.progress = 0.0;
            patrol.direction = 1.0;
        }

        pos.x = patrol.start_x + (patrol.end_x - patrol.start_x) * patrol.progress;
        pos.y = patrol.start_y + (patrol.end_y - patrol.start_y) * patrol.progress;
    }
}

/// System: ships collide with obstacles (pushed out)
pub fn ship_obstacle_collision_system(
    mut ships: Query<(&mut Position, &mut Velocity), (With<Ship>, Without<Obstacle>)>,
    obstacles: Query<(&Position, &Obstacle), Without<Ship>>,
) {
    for (mut ship_pos, mut ship_vel) in ships.iter_mut() {
        for (obs_pos, obs) in obstacles.iter() {
            let dx = ship_pos.x - obs_pos.x;
            let dy = ship_pos.y - obs_pos.y;
            let dist = (dx * dx + dy * dy).sqrt();
            let min_dist = obs.radius + 15.0; // ship radius ~15

            if dist < min_dist && dist > 0.0 {
                // Push ship out
                let nx = dx / dist;
                let ny = dy / dist;
                ship_pos.x = obs_pos.x + nx * min_dist;
                ship_pos.y = obs_pos.y + ny * min_dist;

                // Cancel velocity toward obstacle
                let dot = ship_vel.x * nx + ship_vel.y * ny;
                if dot < 0.0 {
                    ship_vel.x -= dot * nx;
                    ship_vel.y -= dot * ny;
                }
            }
        }
    }
}

/// System: bullets collide with obstacles (destroyed, damage destructible)
pub fn bullet_obstacle_collision_system(
    mut commands: Commands,
    bullets: Query<(Entity, &Position, &Bullet), Without<Obstacle>>,
    mut obstacles: Query<(Entity, &Position, &Obstacle, Option<&mut Health>)>,
) {
    let mut bullets_to_remove = Vec::new();

    for (bullet_entity, bullet_pos, bullet) in bullets.iter() {
        if bullets_to_remove.contains(&bullet_entity) { continue; }

        for (obs_entity, obs_pos, obs, health) in obstacles.iter_mut() {
            let dx = bullet_pos.x - obs_pos.x;
            let dy = bullet_pos.y - obs_pos.y;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist < obs.radius {
                bullets_to_remove.push(bullet_entity);

                // Damage destructible obstacles
                if obs.kind == ObstacleKind::Destructible {
                    if let Some(mut h) = health {
                        h.damage(bullet.damage);
                        if h.is_dead() {
                            commands.entity(obs_entity).try_despawn();
                        }
                    }
                }
                break;
            }
        }
    }

    for entity in bullets_to_remove {
        commands.entity(entity).try_despawn();
    }
}
