use std::fmt::{self, Display};

use buttplug::core::message::ActuatorType;
use serde::{Deserialize, Serialize};

use crate::speed::Speed;

// use crate::*;

/// Global commands on connection level, i.e. connection handling
/// or emergency stop
#[derive(Clone, Debug)]
pub enum ConnectionCommand {
    Scan,
    StopScan,
    StopAll,
    Disconect,
    GetBattery
}

#[derive(Clone, Debug)]
pub enum Task {
    Scalar(Speed),
    Pattern(Speed, ActuatorType, String),
    Linear(Speed, String),
    LinearStroke(Speed, String),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum TkConnectionType {
    InProcess,
    WebSocket(String),
    Test,
}

impl Display for TkConnectionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TkConnectionType::InProcess => write!(f, "In-Process"),
            TkConnectionType::WebSocket(host) => write!(f, "WebSocket {}", host),
            TkConnectionType::Test => write!(f, "Test"),
        }
    }
}

impl Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Task::Scalar(speed) => write!(f, "Constant({}%)", speed),
            Task::Pattern(speed, actuator, pattern) => {
                write!(f, "Pattern({}, {}, {})", speed, actuator, pattern)
            }
            Task::Linear(speed, pattern) => write!(f, "Linear({}, {})", speed, pattern),
            Task::LinearStroke(speed, _) => write!(f, "Stroke({})", speed),
        }
    }
}
