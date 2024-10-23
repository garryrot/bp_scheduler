use std::{fs, path::PathBuf};

use serde::de::DeserializeOwned;
use tracing::{error, info};

pub fn read_config_dir<T>(config_dir: String) -> Vec<T>
where
    T: DeserializeOwned,
    T: Clone
{
    let mut results = vec![];
    match fs::read_dir(config_dir) {
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

pub fn read_or_default<T>(settings_dir: &str, settings_file: &str) -> T 
where
    T: DeserializeOwned,
    T: Clone,
    T: Default
{
    let path: PathBuf = [settings_dir, settings_file].iter().collect::<PathBuf>();
    match fs::read_to_string(path) {
        Ok(settings_json) => match serde_json::from_str::<T>(&settings_json) {
            Ok(settings) => {
                settings
            }
            Err(err) => {
                error!("File '{}/{}' could not be parsed. Error: {}. Using default configuration.", settings_dir, settings_file, err);
                T::default()
            }
        },
        Err(err) => {
            info!("File '{}/{}' could not be opened. Error: {}. Using default configuration.", settings_dir, settings_file, err);
            T::default()
        }
    }
}