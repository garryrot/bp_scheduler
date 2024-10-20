use linear::LinearRange;
use scalar::ScalarRange;
use serde::{Deserialize, Serialize};

pub mod actions;
pub mod actuators;
pub mod connection;
pub mod client;
pub mod linear;
pub mod logging;
pub mod read;
pub mod scalar;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub enum ActuatorLimits {
    #[default]
    None,
    Scalar(ScalarRange),
    Linear(LinearRange),
}
