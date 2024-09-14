use linear::LinearRange;
use scalar::ScalarRange;
use serde::{Deserialize, Serialize};

pub mod devices;
pub mod linear;
pub mod scalar;
pub mod actions;
pub mod read;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub enum ActuatorSettings {
    #[default]
    None,
    Scalar(ScalarRange),
    Linear(LinearRange),
}
