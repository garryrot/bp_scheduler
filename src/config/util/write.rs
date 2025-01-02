use std::{fs, path::PathBuf};

use serde::Serialize;
use tracing::{error, info};

pub fn try_write<T>(content: &T, settings_path: &str, settings_file: &str) -> bool
where
    T: ?Sized + Serialize
{
    match serde_json::to_string_pretty(content) {
        Ok(json) => {
            let _ = fs::create_dir_all(settings_path);
            let filename = [settings_path, settings_file].iter().collect::<PathBuf>();
            info!(?filename, "storing file");
            if let Err(err) = fs::write(filename.clone(), json) {
                error!(?err, ?filename, "errorr writing to path");
                return false;
            }
            true
        },
        Err(err) => {  
            error!(?err, "error deserializing");
            false
        },
    }
}