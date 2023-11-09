use std::fs::remove_dir;

use crate::{
    config::{user_cache_directory, user_configuration_directory},
    utils::create_runtime_directory,
};

/// Print platform info like which platform directories will be used.
pub fn info() -> Result<(), String> {
    let user_configuration_directory = user_configuration_directory();
    let user_cache_directory = user_cache_directory();
    let user_runtime_directory = create_runtime_directory();

    if let Ok(ref directory) = user_runtime_directory {
        // NOTE: It might be cleaner to have a separate function like for the configuration and
        // cache directories.
        let _ = remove_dir(directory);
    }

    println!(
        "USER CONFIGURATION PATH: {}",
        user_configuration_directory
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or("not found".into())
    );
    println!(
        "USER CACHE PATH: {}",
        user_cache_directory
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or("not found".into())
    );
    println!(
        "USER RUNTIME PATH: {}",
        user_runtime_directory
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or("not found".into())
    );

    Ok(())
}
