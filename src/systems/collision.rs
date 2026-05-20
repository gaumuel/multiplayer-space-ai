use bevy_ecs::prelude::*;
use crate::components::{Position, Health, Ship, Bullet, Base};
use crate::spatial::SpatialGrid;
use crate::GameTime;

#[derive(Resource)]
#[allow(dead_code)]
pub struct CollisionConfig {
    pub bullet_hit_radius: f32,
    pub ship_collision_radius: f32,
}

impl Default for CollisionConfig {
    fn default() -> Self {
        Self {
            bullet_hit_radius: 20.0,
            ship_collision_radius: 15.0,
        }
    }
}

pub fn bullet_collision_system(
    mut commands: Commands,
    bullets: Query<(Entity, &Bullet, &Position)>,
    mut ships: Query<(Entity, &Ship, &mut Health, &Position), Without<Bullet>>,
    mut bases: Query<(Entity, &Base, &mut Health, &Position), (Without<Bullet>, Without<Ship>)>,
) {
    // Build spatial grid of ships and bases
    let mut grid = SpatialGrid::default();

    for (entity, _ship, _health, pos) in ships.iter() {
        grid.insert(entity, pos.x, pos.y);
    }
    for (entity, _base, _health, pos) in bases.iter() {
        grid.insert(entity, pos.x, pos.y);
    }

    let mut bullets_to_remove = Vec::new();
    let mut dead_ships: Vec<Entity> = Vec::new();

    for (bullet_entity, bullet, bullet_pos) in bullets.iter() {
        if bullets_to_remove.contains(&bullet_entity) {
            continue;
        }

        let mut hit = false;

        // Query nearby entities within collision radius
        for (nearby_entity, _, _) in grid.query_radius(bullet_pos.x, bullet_pos.y, 50.0) {
            if hit { break; }

            // Check if it's a ship
            if let Ok((ship_entity, ship, mut health, ship_pos)) = ships.get_mut(nearby_entity) {
                if dead_ships.contains(&ship_entity) { continue; }
                if ship.team == bullet.team { continue; }

                let dx = bullet_pos.x - ship_pos.x;
                let dy = bullet_pos.y - ship_pos.y;
                if dx * dx + dy * dy < 20.0 * 20.0 {
                    health.damage(bullet.damage);
                    if health.is_dead() {
                        dead_ships.push(ship_entity);
                    }
                    hit = true;
                }
            }
            // Check if it's a base
            else if let Ok((_base_entity, base, mut health, base_pos)) = bases.get_mut(nearby_entity) {
                if base.team == bullet.team { continue; }

                let dx = bullet_pos.x - base_pos.x;
                let dy = bullet_pos.y - base_pos.y;
                if dx * dx + dy * dy < 50.0 * 50.0 {
                    health.damage(bullet.damage);
                    hit = true;
                }
            }
        }

        if hit {
            bullets_to_remove.push(bullet_entity);
        }
    }

    for entity in dead_ships {
        commands.entity(entity).try_despawn();
    }
    for entity in bullets_to_remove {
        commands.entity(entity).try_despawn();
    }
}

pub fn bullet_lifetime_system(
    mut commands: Commands,
    mut bullets: Query<(Entity, &mut Bullet)>,
    time: Res<GameTime>,
) {
    let dt = time.delta_secs();
    for (entity, mut bullet) in bullets.iter_mut() {
        bullet.lifetime -= dt;
        if bullet.lifetime <= 0.0 {
            commands.entity(entity).try_despawn();
        }
    }
}
