use bevy_ecs::prelude::Entity;
use std::collections::HashMap;

const CELL_SIZE: f32 = 100.0;

#[derive(Default)]
pub struct SpatialGrid {
    cells: HashMap<(i32, i32), Vec<(Entity, f32, f32)>>,
}

impl SpatialGrid {
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        for cell in self.cells.values_mut() {
            cell.clear();
        }
    }

    pub fn insert(&mut self, entity: Entity, x: f32, y: f32) {
        let key = Self::cell_key(x, y);
        self.cells.entry(key).or_default().push((entity, x, y));
    }

    pub fn query_radius(&self, x: f32, y: f32, radius: f32) -> impl Iterator<Item = (Entity, f32, f32)> + '_ {
        let r_cells = (radius / CELL_SIZE).ceil() as i32;
        let cx = (x / CELL_SIZE).floor() as i32;
        let cy = (y / CELL_SIZE).floor() as i32;
        let r2 = radius * radius;

        (cx - r_cells..=cx + r_cells)
            .flat_map(move |gx| (cy - r_cells..=cy + r_cells).map(move |gy| (gx, gy)))
            .filter_map(|key| self.cells.get(&key))
            .flatten()
            .filter(move |(_, ex, ey)| {
                let dx = x - ex;
                let dy = y - ey;
                dx * dx + dy * dy <= r2
            })
            .map(|&(e, ex, ey)| (e, ex, ey))
    }

    fn cell_key(x: f32, y: f32) -> (i32, i32) {
        ((x / CELL_SIZE).floor() as i32, (y / CELL_SIZE).floor() as i32)
    }
}
