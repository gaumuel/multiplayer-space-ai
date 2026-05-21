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

#[derive(Component, Clone, Copy, Debug)]
pub struct Obstacle {
    pub kind: ObstacleKind,
    pub radius: f32,
    /// For rectangular obstacles: half-width and half-height. If both > 0, it's a rectangle.
    pub half_w: f32,
    pub half_h: f32,
}

impl Obstacle {
    pub fn circle(kind: ObstacleKind, radius: f32) -> Self {
        Self { kind, radius, half_w: 0.0, half_h: 0.0 }
    }

    pub fn rect(kind: ObstacleKind, half_w: f32, half_h: f32) -> Self {
        Self { kind, radius: half_w.max(half_h), half_w, half_h }
    }

    pub fn is_rect(&self) -> bool {
        self.half_w > 0.0 && self.half_h > 0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObstacleKind {
    Static,
    Destructible,
    Moving,
}

/// For moving obstacles: defines patrol behavior
#[derive(Component, Clone, Copy, Debug)]
pub struct PatrolMovement {
    pub start_x: f32,
    pub start_y: f32,
    pub end_x: f32,
    pub end_y: f32,
    pub speed: f32,
    pub progress: f32, // 0.0 to 1.0
    pub direction: f32, // 1.0 or -1.0
}
