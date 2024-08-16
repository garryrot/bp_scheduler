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
    pub control: Vec<Control>,
}

impl Action {
    pub fn build(name: &str, controls: Vec<Control>) -> Self {
        let mut selectors = vec![];
        for control in controls {
            selectors.push( control );
        }
        Action {
            name: name.into(),
            control: selectors,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Control {
    Scalar(Speed, Selector, Vec<ScalarActuators>),
    Stroke(Speed, Selector, StrokeRange),
    ScalarPattern(Speed, Selector, Vec<ScalarActuators>, String),
    StrokePattern(Speed, Selector, String)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Selector {
    All,
    BodyParts(Vec<String>)
}

pub enum Modulation {
    Constant,
    Funscript(String)
}
  
impl Control {
    pub fn get_actuators(&self) -> Vec<ActuatorType> {
        match self {
            Control::Scalar(_, _, y) => y.iter().map(|x| x.clone().into()).collect(),
            Control::ScalarPattern(_, _,  y, _) => y.iter().map(|x| x.clone().into()).collect(),
            Control::Stroke(_, _,  _) => vec![ActuatorType::Position],
            Control::StrokePattern(_, _,  _) => vec![ActuatorType::Position],
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
            Action::build("vibrate", vec![ Control::Scalar(Speed::new(100), Selector::All, vec![ScalarActuators::Vibrate]) ]),
            Action::build("constrict", vec![ Control::Scalar(Speed::new(100), Selector::All, vec![ScalarActuators::Constrict]) ]),
            Action::build("inflate", vec![ Control::Scalar(Speed::new(100), Selector::All, vec![ScalarActuators::Constrict]) ]),
            Action::build("scalar", vec![ Control::Scalar(Speed::new(100), Selector::All, vec![
                ScalarActuators::Vibrate,
                ScalarActuators::Constrict,
                ScalarActuators::Oscillate,
                ScalarActuators::Inflate,
            ])]),
            Action::build("stroke.linear", vec![ Control::Stroke(Speed::new(100), Selector::All, StrokeRange { min_ms: 100, max_ms: 1500, min_pos: 0.0, max_pos: 1.0 } ) ]),
            Action::build("stroke.oscillate", vec![ Control::Scalar(Speed::new(100), Selector::All, vec![ ScalarActuators::Oscillate ]) ] )
        ]);

        let json = serde_json::to_string_pretty(&actions).unwrap();
        println!("{}", json);
    }

    #[test]
    pub fn serialize_and_deserialize_actions() {
        let a1 = Actions(vec![
            Action::build("1", vec![ Control::Scalar(Speed::new(100), Selector::All, vec![]) ]),
            Action::build("2", vec![ Control::Scalar(Speed::new(100), Selector::All, vec![ScalarActuators::Constrict]) ]),
        ]); 
        let s1 = serde_json::to_string_pretty(&a1).unwrap();
        let a2 = Actions(vec![
            Action::build("3", vec![ Control::Scalar(Speed::new(100), Selector::All, vec![ScalarActuators::Constrict]) ]),
            Action::build("4", vec![ Control::Scalar(Speed::new(100), Selector::All, vec![
                ScalarActuators::Vibrate
            ]) ])
        ]);
        let s2 = serde_json::to_string_pretty(&a2).unwrap();
        let (_, temp_dir, tmp_path) = create_temp_file("action1.json", &s1);
        add_temp_file("action2.json", &s2, &tmp_path);
        let actions = read_config(temp_dir);
        assert_eq!(actions.0.len(), 4);
        tmp_path.close().unwrap();
    }
}
