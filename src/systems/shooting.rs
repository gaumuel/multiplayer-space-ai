use bevy_ecs::prelude::*;
use crate::components::{
    Position, Velocity, Ship, Bullet, Team, Base,
};
use crate::GameTime;

#[derive(Resource)]
pub struct BulletConfig {
    pub speed: f32,
    pub lifetime: f32,
}

impl Default for BulletConfig {
    fn default() -> Self {
        Self {
            speed: 800.0,
            lifetime: 3.0,
        }
    }
}

#[derive(Clone, Copy)]
struct ShipData {
    entity: Entity,
    team: Team,
    damage: f32,
    fire_rate: f32,
    last_fire_time: f64,
    x: f32,
    y: f32,
    z: f32,
}

pub fn shooting_system(
    mut commands: Commands,
    time: Res<GameTime>,
    bullet_config: Res<BulletConfig>,
    mut params: ParamSet<(
        Query<(Entity, &Ship, &Position)>,
        Query<(Entity, &mut Ship)>,
    )>,
    bases: Query<(&Base, &Position), Without<Ship>>,
) {
    let now = time.elapsed_secs_f64();

    let ship_data: Vec<ShipData> = params.p0().iter().map(|(e, s, p)| ShipData {
        entity: e,
        team: s.team,
        damage: s.damage,
        fire_rate: s.fire_rate,
        last_fire_time: s.last_fire_time,
        x: p.x,
        y: p.y,
        z: p.z,
    }).collect();

    for ship in &ship_data {
        if now - ship.last_fire_time < ship.fire_rate as f64 {
            continue;
        }

        let target = find_nearest_enemy(ship.team, ship.x, ship.y, &ship_data, &bases);
        if let Some((tx, ty, _is_base)) = target {
            if let Ok((_, mut s)) = params.p1().get_mut(ship.entity) {
                s.last_fire_time = now;
            }

            let dx = tx - ship.x;
            let dy = ty - ship.y;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist > 0.0 {
                let vx = (dx / dist) * bullet_config.speed;
                let vy = (dy / dist) * bullet_config.speed;

                commands.spawn((
                    Position { x: ship.x, y: ship.y, z: ship.z },
                    Velocity { x: vx, y: vy },
                    Bullet {
                        team: ship.team,
                        damage: ship.damage,
                        lifetime: bullet_config.lifetime,
                    },
                ));
            }
        }
    }
}

fn find_nearest_enemy(
    team: Team,
    x: f32,
    y: f32,
    ship_data: &[ShipData],
    bases: &Query<(&Base, &Position), Without<Ship>>,
) -> Option<(f32, f32, bool)> {
    let mut nearest: Option<(f32, f32, f32, bool)> = None;

    for s in ship_data {
        if s.team != team {
            let dx = x - s.x;
            let dy = y - s.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if nearest.map_or(true, |(_, _, best, _)| dist < best) {
                nearest = Some((s.x, s.y, dist, false));
            }
        }
    }

    for (base, bp) in bases.iter() {
        if base.team != team {
            let dx = x - bp.x;
            let dy = y - bp.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if nearest.map_or(true, |(_, _, best, _)| dist < best) {
                nearest = Some((bp.x, bp.y, dist, true));
            }
        }
    }

    nearest.map(|(x, y, _, is_base)| (x, y, is_base))
}
