use bevy_ecs::prelude::*;
use rand::Rng;
use crate::components::*;
use crate::GameTime;

/// Spawn symmetric random obstacles in the arena
pub fn spawn_obstacles(world: &mut World) {
    let mut rng = rand::thread_rng();

    // Walls: long rectangular obstacles that force pathfinding
    // 2 horizontal walls near the center
    let wall_y = rng.gen_range(150.0..400.0);
    spawn_rect_obstacle(world, 0.0, wall_y, ObstacleKind::Static, 300.0, 20.0);
    spawn_rect_obstacle(world, 0.0, -wall_y, ObstacleKind::Static, 300.0, 20.0);

    // 2 vertical walls on each side
    let wall_x = rng.gen_range(500.0..900.0);
    let wall_y2 = rng.gen_range(-200.0..200.0);
    spawn_rect_obstacle(world, wall_x, wall_y2, ObstacleKind::Static, 20.0, 200.0);
    spawn_rect_obstacle(world, -wall_x, wall_y2, ObstacleKind::Static, 20.0, 200.0);

    // 1 long wall near each base (forces flanking)
    let base_wall_x = rng.gen_range(1000.0..1300.0);
    let base_wall_y = rng.gen_range(-100.0..100.0);
    spawn_rect_obstacle(world, base_wall_x, base_wall_y, ObstacleKind::Static, 20.0, 250.0);
    spawn_rect_obstacle(world, -base_wall_x, -base_wall_y, ObstacleKind::Static, 20.0, 250.0);

    // 2 destructible circle obstacles (can be cleared)
    let dx = rng.gen_range(300.0..700.0);
    let dy = rng.gen_range(-300.0..300.0);
    spawn_circle_obstacle(world, dx, dy, ObstacleKind::Destructible, 70.0);
    spawn_circle_obstacle(world, -dx, -dy, ObstacleKind::Destructible, 70.0);

    // 1 moving obstacle (patrols a gap)
    let mx = rng.gen_range(200.0..600.0);
    spawn_moving_obstacle(world, mx, 0.0, 80.0, 300.0);
    spawn_moving_obstacle(world, -mx, 0.0, 80.0, 300.0);
}

fn spawn_rect_obstacle(world: &mut World, x: f32, y: f32, kind: ObstacleKind, half_w: f32, half_h: f32) {
    let mut entity = world.spawn((
        Position { x, y, z: 5.0 },
        Obstacle::rect(kind, half_w, half_h),
    ));
    if kind == ObstacleKind::Destructible {
        entity.insert(Health::new(300.0));
    }
}

fn spawn_circle_obstacle(world: &mut World, x: f32, y: f32, kind: ObstacleKind, radius: f32) {
    let mut entity = world.spawn((
        Position { x, y, z: 5.0 },
        Obstacle::circle(kind, radius),
    ));
    if kind == ObstacleKind::Destructible {
        entity.insert(Health::new(300.0));
    }
}

fn spawn_moving_obstacle(world: &mut World, x: f32, y: f32, radius: f32, patrol_range: f32) {
    world.spawn((
        Position { x, y, z: 5.0 },
        Obstacle::circle(ObstacleKind::Moving, radius),
        PatrolMovement {
            start_x: x,
            start_y: y - patrol_range,
            end_x: x,
            end_y: y + patrol_range,
            speed: 60.0,
            progress: 0.5,
            direction: 1.0,
        },
    ));
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
            if obs.is_rect() {
                // Rectangle collision
                let dx = ship_pos.x - obs_pos.x;
                let dy = ship_pos.y - obs_pos.y;
                let clamped_x = dx.clamp(-obs.half_w, obs.half_w);
                let clamped_y = dy.clamp(-obs.half_h, obs.half_h);
                let nearest_x = obs_pos.x + clamped_x;
                let nearest_y = obs_pos.y + clamped_y;
                let diff_x = ship_pos.x - nearest_x;
                let diff_y = ship_pos.y - nearest_y;
                let dist = (diff_x * diff_x + diff_y * diff_y).sqrt();
                let min_dist = 15.0; // ship radius

                if dist < min_dist && dist > 0.0 {
                    let nx = diff_x / dist;
                    let ny = diff_y / dist;
                    ship_pos.x = nearest_x + nx * min_dist;
                    ship_pos.y = nearest_y + ny * min_dist;
                    let dot = ship_vel.x * nx + ship_vel.y * ny;
                    if dot < 0.0 {
                        ship_vel.x -= dot * nx;
                        ship_vel.y -= dot * ny;
                    }
                } else if dist == 0.0 && (dx.abs() <= obs.half_w && dy.abs() <= obs.half_h) {
                    // Ship is inside the rectangle, push out the shortest way
                    let push_x = obs.half_w - dx.abs();
                    let push_y = obs.half_h - dy.abs();
                    if push_x < push_y {
                        ship_pos.x += push_x * dx.signum().max(1.0);
                    } else {
                        ship_pos.y += push_y * dy.signum().max(1.0);
                    }
                }
            } else {
                // Circle collision
                let dx = ship_pos.x - obs_pos.x;
                let dy = ship_pos.y - obs_pos.y;
                let dist = (dx * dx + dy * dy).sqrt();
                let min_dist = obs.radius + 15.0;

                if dist < min_dist && dist > 0.0 {
                    let nx = dx / dist;
                    let ny = dy / dist;
                    ship_pos.x = obs_pos.x + nx * min_dist;
                    ship_pos.y = obs_pos.y + ny * min_dist;
                    let dot = ship_vel.x * nx + ship_vel.y * ny;
                    if dot < 0.0 {
                        ship_vel.x -= dot * nx;
                        ship_vel.y -= dot * ny;
                    }
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
            let hit = if obs.is_rect() {
                let dx = (bullet_pos.x - obs_pos.x).abs();
                let dy = (bullet_pos.y - obs_pos.y).abs();
                dx <= obs.half_w && dy <= obs.half_h
            } else {
                let dx = bullet_pos.x - obs_pos.x;
                let dy = bullet_pos.y - obs_pos.y;
                (dx * dx + dy * dy).sqrt() < obs.radius
            };

            if hit {
                bullets_to_remove.push(bullet_entity);
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
