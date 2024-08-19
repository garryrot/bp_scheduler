// actions/*.json

use std::fs;

use buttplug::core::message::ActuatorType;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Actions(pub Vec<Action>);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Action {
    pub name: String,
    pub control: Vec<Control>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Control {
    Scalar(Selector, Strength, Vec<ScalarActuators>),
    Stroke(Selector, Strength, StrokeRange),
    StrokePattern(Selector, Strength, String),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StrokeRange {
    pub min_ms: i64,
    pub max_ms: i64,
    pub min_pos: f64,
    pub max_pos: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Selector {
    All,
    BodyParts(Vec<String>),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Strength {
    Constant(i32),
    Funscript(i32, String),
    RandomFunscript(i32, Vec<String>)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ScalarActuators {
    Vibrate,
    Oscillate,
    Constrict,
    Inflate,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BodyParts {
    All,
    Tags(Vec<String>),
}

impl Control {
    pub fn get_selector(&self) -> Selector {
        match self {
            Control::Scalar(selector, _, _) => selector.clone(),
            Control::Stroke(selector, _, _) => selector.clone(),
            Control::StrokePattern(selector, _, _) => selector.clone(),
        }
    }
}

impl Control {
    pub fn get_actuators(&self) -> Vec<ActuatorType> {
        match self {
            Control::Scalar(_, _, y) => y.iter().map(|x| x.clone().into()).collect(),
            Control::Stroke(_, _, _) => vec![ActuatorType::Position],
            Control::StrokePattern(_, _, _) => vec![ActuatorType::Position],
        }
    }
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
    use crate::client::settings::settings_tests::*;

    use super::*;

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

    #[test]
    pub fn build_mm_actions() {
        
        let actions = Actions(vec![
            Action::build("milkmod.milkingstage", vec![
                Control::Scalar(
                    Selector::BodyParts(vec!["nipple".into()]),
                    Strength::Constant(100),
                    vec![
                        ScalarActuators::Vibrate,
                        ScalarActuators::Constrict
                    ],
                ),
                Control::Scalar(
                    Selector::BodyParts(vec!["anal".into()]),
                    Strength::Constant(100),
                    vec![
                        ScalarActuators::Vibrate,
                        ScalarActuators::Constrict,
                        ScalarActuators::Oscillate
                    ],
                ),
                // Control::StrokePattern(
                //     Selector::BodyParts(vec!["penis".into(), "vaginal".into()]),
                //     Strength::Constant(100),
                //     StrokeRange { min_ms: (), max_ms: (), min_pos: (), max_pos: () }
                // ),
                Control::Scalar(
                    Selector::BodyParts(vec!["inflate".into()]),
                    Strength::Constant(10),
                    vec![
                        ScalarActuators::Inflate
                    ],
                )
            ])
        ]);
        println!("{}", serde_json::to_string_pretty(&actions).unwrap());
    }
    
    #[test]
    pub fn build_dd_actions() {
        let dd_actions = Actions(vec![
            Action::build(
                "dd.nipple",
                vec![
                    Control::Scalar(
                        Selector::BodyParts(vec!["nipple".into()]),
                        Strength::Constant(100),
                        vec![
                            ScalarActuators::Vibrate
                        ],
                    )
                ],
            ),
            Action::build(
                "dd.vaginal.vibrator",
                vec![
                    Control::Scalar(
                        Selector::BodyParts(vec!["vaginal".into()]),
                        Strength::RandomFunscript(
                            100, 
                            vec![
                                "60_Blowjob".into(),
                                "61_Deepthroat".into()
                            ]
                        ),
                        vec![
                            ScalarActuators::Vibrate
                        ],
                    )
                ]
            ),
            Action::build(
                "dd.vaginal.inflator",
                vec![
                    Control::Scalar(
                        Selector::BodyParts(vec!["vaginal".into()]),
                        Strength::Constant(33),
                        vec![
                            ScalarActuators::Inflate
                        ],
                    )
                ]
            ),
            Action::build(
                "dd.anal.vibrate",
                vec![
                    Control::Scalar(
                        Selector::BodyParts(vec!["anal".into()]),
                        Strength::Constant(100),
                        vec![
                            ScalarActuators::Vibrate
                        ],
                    )
                ]
            ),
            Action::build(
                "dd.anal.inflate",
                vec![
                    Control::Scalar(
                        Selector::BodyParts(vec!["anal".into()]),
                        Strength::Constant(33),
                        vec![
                            ScalarActuators::Inflate
                        ],
                    )
                ]
            ),
        ]);
        println!("{}", serde_json::to_string_pretty(&dd_actions).unwrap());

    }

    #[test]
    pub fn build_default_actions() {

        let actions = Actions(vec![
            Action::build(
                "vibrate",
                vec![Control::Scalar(
                    Selector::All,
                    Strength::Constant(100),
                    vec![ScalarActuators::Vibrate],
                )],
            ),
            Action::build(
                "constrict",
                vec![Control::Scalar(
                    Selector::All,
                    Strength::Constant(100),
                    vec![ScalarActuators::Constrict],
                )],
            ),
            Action::build(
                "inflate",
                vec![Control::Scalar(
                    Selector::All,
                    Strength::Constant(100),
                    vec![ScalarActuators::Constrict],
                )],
            ),
            Action::build(
                "scalar",
                vec![Control::Scalar(
                    Selector::All,
                    Strength::Constant(100),
                    vec![
                        ScalarActuators::Vibrate,
                        ScalarActuators::Constrict,
                        ScalarActuators::Oscillate,
                        ScalarActuators::Inflate,
                    ],
                )],
            ),
            Action::build(
                "linear.stroke",
                vec![Control::Stroke(
                    Selector::All,
                    Strength::Constant(100),
                    StrokeRange {
                        min_ms: 100,
                        max_ms: 1500,
                        min_pos: 0.0,
                        max_pos: 1.0,
                    },
                )],
            ),
            Action::build(
                "oscillate.stroke",
                vec![Control::Scalar(
                    Selector::All,
                    Strength::Constant(100),
                    vec![ScalarActuators::Oscillate],
                )],
            ),
        ]);

        let json = serde_json::to_string_pretty(&actions).unwrap();
        println!("{}", json);
    }

    #[test]
    pub fn serialize_and_deserialize_actions() {
        let a1 = Actions(vec![
            Action::build(
                "1",
                vec![Control::Scalar(
                    Selector::All, 
                    Strength::Constant(100),
                    vec![])],
            ),
            Action::build(
                "2",
                vec![Control::Scalar(
                    Selector::All,
                    Strength::Constant(100),
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
                    Strength::Constant(100),
                    vec![ScalarActuators::Constrict],
                )],
            ),
            Action::build(
                "4",
                vec![Control::Scalar(
                    Selector::All,
                    Strength::Constant(100),
                    vec![ScalarActuators::Vibrate],
                )],
            ),
        ]);
        let s2 = serde_json::to_string_pretty(&a2).unwrap();
        let (_, temp_dir, tmp_path) = create_temp_file("action1.json", &s1);
        add_temp_file("action2.json", &s2, &tmp_path);
        let actions = read_config(temp_dir);
        assert_eq!(actions.0.len(), 4);
        tmp_path.close().unwrap();
    }
}
