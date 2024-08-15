// actions/*.json

use std::fs;

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

pub fn read_config(config_dir: String) -> Actions {
    let mut results = vec![];
    if let Ok(dir) = fs::read_dir(config_dir) {
        for entry in dir.into_iter().flatten() {
            if entry.path().is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.eq_ignore_ascii_case("json"))
                    .unwrap_or(false)
            {
                if let Some(actions) = fs::read_to_string(entry.path())
                    .ok()
                    .and_then(|x| serde_json::from_str::<Actions>(&x).ok())
                {
                    results.append(&mut actions.0.clone());
                }
            }
        }
    }
    Actions(results)
}

#[cfg(test)]
mod tests {
    use crate::{client::settings::settings_tests::*, speed::Speed};

    use super::*;

    #[test]
    pub fn build_default_actions() {
        let actions = Actions(vec![
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
            Action {
                name: "stroke.linear".into(), 
                speed: Speed::new(100), 
                control: Control::Stroke( StrokeRange { min_ms: 100, max_ms: 1500, min_pos: 0.0, max_pos: 1.0 } )
            },
            Action {
                name: "stroke.oscillate".into(), 
                speed: Speed::new(100), 
                control: Control::Scalar(vec![ ScalarActuators::Oscillate ])
            }
        ]);

        let json = serde_json::to_string_pretty(&actions).unwrap();

        println!("{}", json);
    }

    #[test]
    pub fn serialize_and_deserialize_actions() {
        let a1 = Actions(vec![
            Action {
                name: "1".into(),
                speed: Speed::new(100),
                control: Control::Scalar(vec![]),
            },
            Action {
                name: "2".into(),
                speed: Speed::new(100),
                control: Control::Scalar(vec![ScalarActuators::Constrict]),
            },
        ]);
        let s1 = serde_json::to_string_pretty(&a1).unwrap();
        let a2 = Actions(vec![
            Action {
                name: "3".into(),
                speed: Speed::new(100),
                control: Control::Scalar(vec![ScalarActuators::Constrict]),
            },
            Action {
                name: "4".into(),
                speed: Speed::new(100),
                control: Control::Scalar(vec![
                    ScalarActuators::Vibrate
                ]),
            },
        ]);
        let s2 = serde_json::to_string_pretty(&a2).unwrap();
        let (_, temp_dir, tmp_path) = create_temp_file("action1.json", &s1);
        add_temp_file("action2.json", &s2, &tmp_path);
        let actions = read_config(temp_dir);
        assert_eq!(actions.0.len(), 4);
        tmp_path.close().unwrap();
    }
}
