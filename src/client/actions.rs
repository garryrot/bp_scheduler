// actions/*.json

use buttplug::core::message::ActuatorType;
use serde::{Deserialize, Serialize};

use crate::speed::Speed;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Actions(Vec<Action>);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StrokeRange {
    pub min_ms: i64,
    pub max_ms: i64,
    pub min_pos: f64,
    pub max_pos: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Action {
    pub name: String,
    pub speed: Speed,
    pub control: Control,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Control {
    Scalar(Vec<ScalarActuators>),
    ScalarPattern(String, Vec<ScalarActuators>),
    Stroke(StrokeRange),
    StrokePattern(String),
}

impl Control {
    pub fn get_actuators(&self) -> Vec<ActuatorType> {
        match self {
            Control::Scalar(y) => y.iter().map(|x| x.clone().into()).collect(),
            Control::ScalarPattern(_, y) => y.iter().map(|x| x.clone().into()).collect(),
            Control::Stroke(_) => vec![ActuatorType::Position],
            Control::StrokePattern(_) => vec![ActuatorType::Position],
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ScalarActuators {
    Vibrate,
    Oscillate,
    Constrict,
    Inflate,
}

impl From<ScalarActuators> for buttplug::core::message::ActuatorType {
    fn from(val: ScalarActuators) -> Self {
        match val {
            ScalarActuators::Vibrate => ActuatorType::Vibrate,
            ScalarActuators::Oscillate => ActuatorType::Oscillate,
            ScalarActuators::Constrict => ActuatorType::Constrict,
            ScalarActuators::Inflate => ActuatorType::Inflate,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BodyParts {
    All,
    Tags(Vec<String>),
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::speed::Speed;

    use super::*;

    pub fn read_config(config_dir: String) -> Actions {
        let mut results = vec![];

        if let Ok(dir) = fs::read_dir(config_dir) {
            for entry in dir.into_iter().flatten() {
                if entry.path().is_file() && entry.path()
                                                    .extension()
                                                    .and_then(|x| x.to_str())
                                                    .map(|x| x.eq_ignore_ascii_case("json"))
                                                    .unwrap_or(false) {

                    if let Some(actions) = fs::read_to_string(entry.path()).ok().and_then( |x| serde_json::from_str::<Actions>(&x).ok() ) {
                        results.append(&mut actions.0.clone());
                    }
                }
            }
        }

        Actions(results)
    }

    pub fn build_config() {
        let default_actions = Actions(vec![
            Action {
                name: "vibrate".into(),
                speed: Speed::new(100),
                control: Control::Scalar(vec![ScalarActuators::Vibrate]),
            },
            Action {
                name: "constrict".into(),
                speed: Speed::new(100),
                control: Control::Scalar(vec![ScalarActuators::Constrict]),
            },
            Action {
                name: "inflate".into(),
                speed: Speed::new(100),
                control: Control::Scalar(vec![ScalarActuators::Constrict]),
            },
            Action {
                name: "scalar".into(),
                speed: Speed::new(100),
                control: Control::Scalar(vec![
                    ScalarActuators::Vibrate,
                    ScalarActuators::Constrict,
                    ScalarActuators::Oscillate,
                    ScalarActuators::Inflate,
                ]),
            },
        ]);

        serde_json::to_string_pretty(&default_actions).unwrap();
    }
}
