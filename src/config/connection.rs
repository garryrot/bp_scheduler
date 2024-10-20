use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum ConnectionType {
    InProcess,
    WebSocket(String),
    Test,
}
