use bevy_ecs::prelude::*;
use crate::components::{
    Position, Health, Ship, Bullet, Base,
};
use crate::GameTime;

#[derive(Resource)]
pub struct CollisionConfig {
    pub bullet_hit_radius: f32,
    pub ship_collision_radius: f32,
}

impl Default for CollisionConfig {
    fn default() -> Self {
        Self {
            bullet_hit_radius: 10.0,
            ship_collision_radius: 15.0,
        }
    }
}

pub fn bullet_collision_system(
    mut commands: Commands,
    bullets: Query<(Entity, &Bullet, &Position)>,
    mut ships: Query<(Entity, &mut Ship, &mut Health, &Position), Without<Bullet>>,
    mut bases: Query<(Entity, &Base, &mut Health, &Position), (Without<Bullet>, Without<Ship>)>,
) {
    let mut bullets_to_remove = Vec::new();
    let mut dead_entities: Vec<Entity> = Vec::new();

    for (bullet_entity, bullet, bullet_pos) in bullets.iter() {
        if bullets_to_remove.contains(&bullet_entity) {
            continue;
        }

        let mut hit = false;

        for (ship_entity, _ship, mut health, ship_pos) in ships.iter_mut() {
            if dead_entities.contains(&ship_entity) {
                continue;
            }
            if _ship.team == bullet.team {
                continue;
            }
            if distance(bullet_pos, &ship_pos) < 20.0 {
                health.damage(bullet.damage);
                if health.is_dead() {
                    dead_entities.push(ship_entity);
                }
                hit = true;
                break;
            }
        }

        if !hit {
            for (_base_entity, base, mut health, base_pos) in bases.iter_mut() {
                if base.team != bullet.team {
                    if distance(bullet_pos, &base_pos) < 50.0 {
                        health.damage(bullet.damage);
                        hit = true;
                        break;
                    }
                }
            }
        }

        if hit {
            bullets_to_remove.push(bullet_entity);
        }
    }

    for entity in dead_entities {
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

fn distance(a: &Position, b: &Position) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    (dx * dx + dy * dy).sqrt()
}
