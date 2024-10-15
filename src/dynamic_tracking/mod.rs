use std::sync::Arc;

use buttplug::client::ButtplugClientDevice;
use tokio::{sync::mpsc::UnboundedReceiver, time::Instant};

use crate::linear::LinearRange;

pub mod movements;
pub mod collision;
pub mod tracking_mirror;
pub mod util;

pub struct Margins {
    most_in: f64,
    most_out: f64
}

impl Margins {
    pub fn new( most_in: f64, most_out: f64) -> Self {
        Margins {
            most_in,
            most_out,
        }
    }
}

pub enum TrackingSignal {
    Penetration(Instant),
    OuterTurn(Instant, Margins),
    InnerTurn(Instant, Margins),
    Stop,
}

pub struct DynamicSettings {
    pub boundaries: LinearRange,
    pub move_at_start: bool,
    pub min_resolution_ms: u32,
    pub min_duration_ms: u32,
    pub default_stroke_ms: u32,
    pub default_stroke_in: f64,
    pub default_stroke_out: f64,
    pub time_window_ms: u32,
}

impl Default for DynamicSettings {
    fn default() -> Self {
        DynamicSettings {
            boundaries: LinearRange::max(),
            move_at_start: true,
            min_resolution_ms: 50,
            min_duration_ms: 200,
            default_stroke_ms: 400,
            default_stroke_in: 0.0,
            default_stroke_out: 1.0,
            time_window_ms: 3_000,
        }
    }
}

pub struct DynamicTracking {
    pub settings: DynamicSettings,
    pub signals: UnboundedReceiver<TrackingSignal>,
    pub devices: Vec<Arc<ButtplugClientDevice>>,
}

