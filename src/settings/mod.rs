use linear::LinearRange;
use scalar::ScalarRange;
use serde::{Deserialize, Serialize};

pub mod devices;
pub mod linear;
pub mod scalar;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub enum ActuatorSettings {
    #[default]
    None,
    Scalar(ScalarRange),
    Linear(LinearRange),
}
