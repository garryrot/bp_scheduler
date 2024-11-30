use std::{fmt::{self, Display}, sync::{atomic::AtomicI64, Arc}};

use buttplug::core::message::ActuatorType;
use serde::{Deserialize, Serialize};

use crate::speed::Speed;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Actions(pub Vec<Action>);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ActionRef {
    pub action: String,
    pub strength: Stren,
}

impl ActionRef {
    pub fn new(name: &str, strength: Stren) -> Self {
        ActionRef {
            action: name.into(),
            strength,
        }
    }
}

// TODO: This struct needs to disapper from bp_scheduler and move somewhere else
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Variable {
    PlayerActorValue(String),
    BoneTrackingRate,
    BoneTrackingDepth,
    BoneTrackingPos,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Stren {
    Constant(i32),
    Variable(Variable),
    Funscript(i32, String),
    RandomFunscript(i32, Vec<String>)
}

#[derive(Debug, Clone)]
pub enum Strength {
    Constant(i32),
    Variable(Arc<AtomicI64>),
    Funscript(i32, String),
    RandomFunscript(i32, Vec<String>)
}

impl Strength {
    pub fn multiply(self, speed: &Speed) -> Strength {
        let mult = |x: i32| Speed::new(x.into()).multiply(speed).value.into();
        match self {
            Strength::Constant(x) => Strength::Constant(mult(x)),
            Strength::Funscript(x, fs) => Strength::Funscript(mult(x), fs),
            Strength::RandomFunscript(x, fss) => Strength::RandomFunscript(mult(x), fss),
            Strength::Variable(arc) => Strength::Variable(arc),
        }
    }
}

impl Display for Strength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Strength::Constant(speed) => write!(f, "Constant({}%)", speed),
            Strength::Funscript(speed, funscript) => write!(f, "Funscript({}, {}%)", funscript, speed),
            Strength::RandomFunscript(speed, vec) => write!(f, "Random({}%, {})", speed, vec.join(",")),
            Strength::Variable(_) => write!(f, "Dynamic"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Action {
    pub name: String,
    pub do_bone_tracking: bool,
    pub control: Vec<Control>
}

impl Action {
    pub fn new(name: &str, control: Vec<Control>) -> Self {
        Action {
            name: name.into(),
            do_bone_tracking: false,
            control
        }
    }

    pub fn new_with_bone(name: &str, control: Vec<Control>) -> Self {
        Action {
            name: name.into(),
            do_bone_tracking: true,
            control
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Control {
    Scalar(Selector, Vec<ScalarActuator>),
    Stroke(Selector, StrokeRange)
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
pub enum ScalarActuator {
    Vibrate,
    Oscillate,
    Constrict,
    Inflate,
}

impl From<ScalarActuator> for buttplug::core::message::ActuatorType {
    fn from(val: ScalarActuator) -> Self {
        match val {
            ScalarActuator::Vibrate => ActuatorType::Vibrate,
            ScalarActuator::Oscillate => ActuatorType::Oscillate,
            ScalarActuator::Constrict => ActuatorType::Constrict,
            ScalarActuator::Inflate => ActuatorType::Inflate,
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
    use crate::{config::client::settings_tests::*, read::read_config_dir};

    use super::*;

    #[test]
    pub fn build_mm_actions() {
        let actions = vec![Action::new(
            "milkmod.milkingstage",
            vec![
                Control::Scalar(
                    Selector::BodyParts(vec!["nipple".into()]),
                    vec![ScalarActuator::Vibrate, ScalarActuator::Constrict],
                ),
                Control::Scalar(
                    Selector::BodyParts(vec!["anal".into()]),
                    vec![
                        ScalarActuator::Vibrate,
                        ScalarActuator::Constrict,
                        ScalarActuator::Oscillate,
                    ],
                ),
                Control::Scalar(
                    Selector::BodyParts(vec!["inflate".into()]),
                    vec![ScalarActuator::Inflate],
                ),
            ],
        )];
        println!("{}", serde_json::to_string_pretty(&actions).unwrap());
    }

    #[test]
    pub fn serialize_and_deserialize_actions() {
        let a1 = Actions(vec![
            Action::new("1", vec![Control::Scalar(Selector::All, vec![])]),
            Action::new(
                "2",
                vec![Control::Scalar(
                    Selector::All,
                    vec![ScalarActuator::Constrict],
                )],
            ),
        ]);
        let s1 = serde_json::to_string_pretty(&a1).unwrap();
        let a2 = Actions(vec![
            Action::new(
                "3",
                vec![Control::Scalar(
                    Selector::All,
                    vec![ScalarActuator::Constrict],
                )],
            ),
            Action::new(
                "4",
                vec![Control::Scalar(
                    Selector::All,
                    vec![ScalarActuator::Vibrate],
                )],
            ),
        ]);
        let s2 = serde_json::to_string_pretty(&a2).unwrap();
        let (_, temp_dir, tmp_path) = create_temp_file("action1.json", &s1);
        add_temp_file("action2.json", &s2, &tmp_path);
        let actions: Vec<Action> = read_config_dir(temp_dir);
        assert_eq!(actions.len(), 4);
        tmp_path.close().unwrap();
    }
}
