use serde::{Deserialize, Serialize};

use crate::speed::Speed;

use super::ActuatorSettings;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum LinearSpeedScaling {
    Linear,         // f(x) = x
    Parabolic(i32), // f(x) = 1 - (1 - x)^n
}

impl LinearSpeedScaling {
    pub fn apply(&self, speed: Speed) -> Speed {
        match self {
            LinearSpeedScaling::Linear => speed,
            LinearSpeedScaling::Parabolic(n) => {
                let mut x = speed.as_float();
                x = 1.0 - (1.0 - x).powi(*n);
                Speed::from_float(x)
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LinearRange {
    pub min_ms: i64,
    pub max_ms: i64,
    pub min_pos: f64,
    pub max_pos: f64,
    pub invert: bool,
    pub scaling: LinearSpeedScaling,
}

impl LinearRange {
    pub fn max() -> Self {
        Self {
            min_ms: 50,
            max_ms: 10_000,
            min_pos: 0.0,
            max_pos: 1.0,
            invert: false,
            scaling: LinearSpeedScaling::Linear,
        }
    }
}

impl Default for LinearRange {
    fn default() -> Self {
        Self {
            min_ms: 300,
            max_ms: 3000,
            min_pos: 0.0,
            max_pos: 1.0,
            invert: false,
            scaling: LinearSpeedScaling::Linear,
        }
    }
}

impl ActuatorSettings {
    pub fn linear_or_max(&self) -> LinearRange {
        if let ActuatorSettings::Linear(settings) = self {
            return settings.clone();
        }
        LinearRange::max()
    }
}
