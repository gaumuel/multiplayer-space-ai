use bevy_ecs::prelude::*;
use crate::components::{Position, Velocity, Ship, Base, Team};

const SHIP_SPEED: f32 = 120.0;

pub fn ai_movement_system(
    mut ships: Query<(&Ship, &Position, &mut Velocity)>,
    bases: Query<(&Base, &Position), Without<Ship>>,
) {
    let enemy_base_pos: Vec<(Team, f32, f32)> = bases.iter()
        .map(|(b, p)| (b.team, p.x, p.y))
        .collect();

    for (ship, pos, mut vel) in ships.iter_mut() {
        // Move toward the enemy base
        let target = enemy_base_pos.iter()
            .find(|(team, _, _)| *team != ship.team);

        if let Some((_, tx, ty)) = target {
            let dx = tx - pos.x;
            let dy = ty - pos.y;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist > 100.0 {
                vel.x = (dx / dist) * SHIP_SPEED;
                vel.y = (dy / dist) * SHIP_SPEED;
            } else {
                vel.x = 0.0;
                vel.y = 0.0;
            }
        }
    }
}
