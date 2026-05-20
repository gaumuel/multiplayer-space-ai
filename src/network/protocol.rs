use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct Snapshot {
    pub tick: u32,
    pub entities: Vec<EntityDelta>,
}

#[derive(Serialize, Clone, Debug)]
pub struct EntityDelta {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub entity_type: EntityType,
    pub team: Option<u8>,
    pub health: Option<f32>,
    pub max_health: Option<f32>,
}

#[allow(dead_code)]
#[derive(Serialize, Clone, Debug)]
pub enum EntityType {
    Ship,
    Bullet,
    Base,
    Pickup,
    Obstacle,
}
