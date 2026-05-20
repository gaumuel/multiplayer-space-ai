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
    #[allow(dead_code)]
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
