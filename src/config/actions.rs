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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Variable {
    PlayerActorValue(String), // TODO: This entry needs to disapper from bp_scheduler and move somewhere else
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
    pub control: Vec<Control>
}

impl Action {
    pub fn new(name: &str, control: Vec<Control>) -> Self {
        Action {
            name: name.into(),
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
    Any,
    NotTag(String),
    Tag(String),
    And(Vec<Box<Selector>>),
    Or(Vec<Box<Selector>>),
}

impl Selector {
    pub fn body_parts(tags: Vec<String>) -> Selector {
        if tags.len() == 1 {
            return Selector::Tag(tags[0].clone())
        }
        let mut selectors = vec![];
        for tag in tags {
            selectors.push(Box::new(Selector::Tag(tag.trim().to_lowercase())));
        }
        Selector::Or(selectors)
    }
    pub fn from(tags: &Vec<String>) -> Self {
        let mut result = Selector::Any;
        if !tags.is_empty() {
            result = Selector::body_parts(tags.clone());
        }
        result
    }
    pub fn and(self, other: Selector) -> Selector {
        Selector::And(vec![Box::new(self), Box::new(other)])
    }
    pub fn matches(&self, tags: &Vec<String>) -> bool {
        match self {
            Selector::Any => true,
            Selector::NotTag(tag) => !tags.contains(tag),
            Selector::Tag(tag) =>  tags.contains(&tag),
            Selector::And(items) => items.iter().all(|x| x.matches(tags)),
            Selector::Or(items) => items.iter().any(|x| x.matches(tags)),
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
    use tempfile::TempDir;

    use tokio_test::assert_ok;
    use std::fs;
    use crate::config::client::settings_tests::*;
    use crate::config::util::read::read_config_dir;
    
    use super::*;

    #[test]
    pub fn serialize_and_deserialize_actions() {
        let a1 = Actions(vec![
            Action::new("1", vec![Control::Scalar(Selector::Any, vec![])]),
            Action::new(
                "2",
                vec![Control::Scalar(
                    Selector::Any,
                    vec![ScalarActuator::Constrict],
                )],
            ),
        ]);
        let s1 = serde_json::to_string_pretty(&a1).unwrap();
        let a2 = Actions(vec![
            Action::new(
                "3",
                vec![Control::Scalar(
                    Selector::Any,
                    vec![ScalarActuator::Constrict],
                )],
            ),
            Action::new(
                "4",
                vec![Control::Scalar(
                    Selector::Any,
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

    fn add_temp_file(name: &str, content: &str, tmp_path: &TempDir) {
        assert_ok!(fs::write(tmp_path.path().join(name).clone(), content));
    }
}
