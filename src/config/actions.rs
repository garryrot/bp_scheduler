use buttplug::core::message::ActuatorType;
use serde::{Deserialize, Serialize};

use crate::speed::Speed;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Actions(pub Vec<Action>);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ActionRef {
    pub action: String,
    pub strength: Strength,
}

impl ActionRef {
    pub fn new(name: &str, strength: Strength) -> Self {
        ActionRef {
            action: name.into(),
            strength,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Strength {
    Constant(i32),
    Funscript(i32, String),
    RandomFunscript(i32, Vec<String>),
}

impl Strength {
    pub fn multiply(self, speed: &Speed) -> Strength {
        let mult = |x: i32| Speed::new(x.into()).multiply(speed).value.into();
        match self {
            Strength::Constant(x) => Strength::Constant(mult(x)),
            Strength::Funscript(x, fs) => Strength::Funscript(mult(x), fs),
            Strength::RandomFunscript(x, fss) => Strength::RandomFunscript(mult(x), fss),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BodyParts {
    All,
    Tags(Vec<String>),
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
            selectors.push(control);
        }
        Action {
            name: name.into(),
            control: selectors,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Control {
    Scalar(Selector, Vec<ScalarActuators>),
    Stroke(Selector, StrokeRange),
}

impl Control {
    pub fn get_selector(&self) -> Selector {
        match self {
            Control::Scalar(selector, _) => selector.clone(),
            Control::Stroke(selector, _) => selector.clone(),
        }
    }
    pub fn get_actuators(&self) -> Vec<ActuatorType> {
        match self {
            Control::Scalar(_, y) => y.iter().map(|x| x.clone().into()).collect(),
            Control::Stroke(_, _) => vec![ActuatorType::Position],
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Selector {
    All,
    BodyParts(Vec<String>),
}

impl Selector {
    pub fn from(tags: &Vec<String>) -> Self {
        let mut result = Selector::All;
        if !tags.is_empty() {
            result = Selector::BodyParts(tags.clone());
        }
        result
    }
    pub fn and(&self, selector: Selector) -> Selector {
        match self {
            Selector::All => match selector {
                Selector::All => Selector::All,
                Selector::BodyParts(vec) => Selector::BodyParts(vec),
            },
            Selector::BodyParts(vec) => match selector {
                Selector::All => Selector::BodyParts(vec.clone()),
                Selector::BodyParts(vec2) => {
                    let mut a = vec.clone();
                    a.extend(vec2);
                    Selector::BodyParts(a)
                },
            },
        }
    }
    pub fn as_vec(&self) -> Vec<String> {
        match self {
            Selector::All => vec![],
            Selector::BodyParts(vec) => vec.clone(),
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
pub struct StrokeRange {
    pub min_ms: i64,
    pub max_ms: i64,
    pub min_pos: f64,
    pub max_pos: f64,
}

#[cfg(test)]
mod tests {
    use crate::{client::settings::settings_tests::*, read::read_config};

    use super::*;

    #[test]
    pub fn build_mm_actions() {
        let actions = vec![Action::build(
            "milkmod.milkingstage",
            vec![
                Control::Scalar(
                    Selector::BodyParts(vec!["nipple".into()]),
                    vec![ScalarActuators::Vibrate, ScalarActuators::Constrict],
                ),
                Control::Scalar(
                    Selector::BodyParts(vec!["anal".into()]),
                    vec![
                        ScalarActuators::Vibrate,
                        ScalarActuators::Constrict,
                        ScalarActuators::Oscillate,
                    ],
                ),
                Control::Scalar(
                    Selector::BodyParts(vec!["inflate".into()]),
                    vec![ScalarActuators::Inflate],
                ),
            ],
        )];
        println!("{}", serde_json::to_string_pretty(&actions).unwrap());
    }

    #[test]
    pub fn serialize_and_deserialize_actions() {
        let a1 = Actions(vec![
            Action::build("1", vec![Control::Scalar(Selector::All, vec![])]),
            Action::build(
                "2",
                vec![Control::Scalar(
                    Selector::All,
                    vec![ScalarActuators::Constrict],
                )],
            ),
        ]);
        let s1 = serde_json::to_string_pretty(&a1).unwrap();
        let a2 = Actions(vec![
            Action::build(
                "3",
                vec![Control::Scalar(
                    Selector::All,
                    vec![ScalarActuators::Constrict],
                )],
            ),
            Action::build(
                "4",
                vec![Control::Scalar(
                    Selector::All,
                    vec![ScalarActuators::Vibrate],
                )],
            ),
        ]);
        let s2 = serde_json::to_string_pretty(&a2).unwrap();
        let (_, temp_dir, tmp_path) = create_temp_file("action1.json", &s1);
        add_temp_file("action2.json", &s2, &tmp_path);
        let actions: Vec<Action> = read_config(temp_dir);
        assert_eq!(actions.len(), 4);
        tmp_path.close().unwrap();
    }
}
