use bevy_ecs::prelude::*;
use crate::components::{Position, Velocity};
use crate::GameTime;

pub fn movement_system(mut query: Query<(&mut Position, &Velocity)>, time: Res<GameTime>) {
    let dt = time.delta_secs();
    for (mut pos, vel) in query.iter_mut() {
        pos.x += vel.x * dt;
        pos.y += vel.y * dt;
    }
}
