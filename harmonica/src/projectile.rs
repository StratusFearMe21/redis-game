use emath::{Pos2, Vec2};

pub struct Projectile {
    pos: Pos2,
    vel: Vec2,
    acc: Vec2,
}

pub const GRAVITY: Vec2 = Vec2 { x: 0.0, y: -9.81 };
pub const TERMINAL_GRAVITY: Vec2 = Vec2 { x: 0.0, y: 9.81 };

impl Projectile {
    pub fn new(intial_position: Pos2, initial_velocity: Vec2, initial_acceleration: Vec2) -> Self {
        Self {
            pos: intial_position,
            vel: initial_velocity,
            acc: initial_acceleration,
        }
    }

    pub fn update(&mut self, delta_time: f32) -> Pos2 {
        self.pos += self.vel * delta_time;
        self.vel += self.acc * delta_time;

        return self.pos;
    }

    pub fn position(&self) -> Pos2 {
        self.pos
    }

    pub fn velocity(&self) -> Vec2 {
        self.vel
    }

    pub fn acceleration(&self) -> Vec2 {
        self.acc
    }
}
