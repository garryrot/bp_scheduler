use std::fs;

use serde::de::DeserializeOwned;
use tracing::error;

pub fn read_config<T>(config_dir: String) -> Vec<T>
where
    T: DeserializeOwned,
    T: Clone
{
    let mut results = vec![];
    match fs::read_dir(config_dir.clone()) {
        Ok(dir) => {
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
                        .and_then(|x| serde_json::from_str::<Vec<T>>(&x).ok() )
                    {
                        results.append(&mut actions.clone());
                    }
                }
            }
        },
        Err(err) => {
            error!("read_config error: {:?}", err)
        }
    }
    results
}

