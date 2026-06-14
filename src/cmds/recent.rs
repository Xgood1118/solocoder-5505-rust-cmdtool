use anyhow::Result;
use chrono::{Local, TimeDelta};
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

use crate::core::config::AppConfig;
use crate::core::exit;
use crate::core::logger;
use crate::core::signal;

pub struct RecentOpts {
    pub directory: PathBuf,
    pub days: u64,
    pub pattern: Option<String>,
}

pub fn run(opts: &RecentOpts, _config: &AppConfig) -> Result<i32> {
    if !opts.directory.exists() {
        logger::log_error!("directory not found: {:?}", opts.directory);
        return Ok(exit::EXIT_INVALID_ARGS);
    }

    let cutoff = Local::now() - TimeDelta::days(opts.days as i64);
    let mut found = 0u64;

    for entry in WalkDir::new(&opts.directory).into_iter().filter_map(|e| e.ok()) {
        if signal::is_interrupted() {
            return Ok(exit::EXIT_UNKNOWN);
        }

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        if let Some(ref pat) = opts.pattern {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.contains(pat) {
                continue;
            }
        }

        let modified: chrono::DateTime<Local> = match fs::metadata(path).and_then(|m| m.modified()) {
            Ok(m) => m.into(),
            Err(_) => continue,
        };

        if modified > cutoff {
            let rel = path.strip_prefix(&opts.directory).unwrap_or(path);
            println!("{}  {}", modified.format("%Y-%m-%d %H:%M:%S"), rel.display());
            found += 1;
        }
    }

    logger::log_info!("found {} file(s) modified in last {} day(s)", found, opts.days);
    Ok(exit::EXIT_OK)
}
