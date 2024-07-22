use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ScalarScaling {
    // Note: currently unused
    Linear,            // f(x) = x
    Quadratic,         // f(x) = x^2
    QuadraticFraction, // f(x) = x^(1/2)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScalarRange {
    pub min_speed: i64,
    pub max_speed: i64,
    pub factor: f64,
    pub scaling: ScalarScaling,
}

impl Default for ScalarRange {
    fn default() -> Self {
        Self {
            min_speed: 0,
            max_speed: 100,
            factor: 1.0,
            scaling: ScalarScaling::Linear,
        }
    }
}
