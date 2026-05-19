use bevy_ecs::component::Component;

#[derive(Component, Clone, Copy, Debug)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Default for Position {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, z: 0.0 }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Velocity {
    pub x: f32,
    pub y: f32,
}

impl Default for Velocity {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }

    pub fn damage(&mut self, amount: f32) {
        self.current = (self.current - amount).max(0.0);
    }

    pub fn heal(&mut self, amount: f32) {
        self.current = (self.current + amount).min(self.max);
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Team {
    Player,
    Enemy,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Ship {
    pub team: Team,
    pub class: ShipClass,
    pub fire_rate: f32,
    pub damage: f32,
    pub last_fire_time: f64,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShipClass {
    Scout,
    Tank,
    Sniper,
}

impl Ship {
    pub fn new(team: Team, class: ShipClass) -> Self {
        let (fire_rate, damage) = match class {
            ShipClass::Scout => (0.3, 8.0),
            ShipClass::Tank => (0.6, 15.0),
            ShipClass::Sniper => (1.2, 30.0),
        };
        Self {
            team,
            class,
            fire_rate,
            damage,
            last_fire_time: 0.0,
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Base {
    pub team: Team,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Bullet {
    pub team: Team,
    pub damage: f32,
    pub lifetime: f32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Pickup {
    pub kind: PickupKind,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickupKind {
    Damage,
    FireRate,
    ShieldRegen,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Resource {
    pub amount: f32,
    pub regen_rate: f32,
}

impl Resource {
    pub fn new(amount: f32, regen_rate: f32) -> Self {
        Self { amount, regen_rate }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub struct Obstacle {
    pub radius: f32,
}

#[derive(Component, Clone, Copy, Debug)]
pub struct VisionRadius {
    pub radius: f32,
}
