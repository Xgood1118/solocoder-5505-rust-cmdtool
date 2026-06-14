use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::core::config::AppConfig;
use crate::core::exit;
use crate::core::logger;
use crate::core::signal;

pub struct FinddupOpts {
    pub directory: PathBuf,
    pub min_size: u64,
    pub delete: bool,
}

fn file_hash(path: &Path) -> Result<String> {
    let data = fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = hasher.finalize();
    Ok(format!("{:x}", hash))
}

pub fn run(opts: &FinddupOpts, _config: &AppConfig) -> Result<i32> {
    if !opts.directory.exists() {
        logger::log_error!("directory not found: {:?}", opts.directory);
        return Ok(exit::EXIT_INVALID_ARGS);
    }

    let mut size_map: HashMap<u64, Vec<PathBuf>> = HashMap::new();

    for entry in WalkDir::new(&opts.directory).into_iter().filter_map(|e| e.ok()) {
        if signal::is_interrupted() {
            return Ok(exit::EXIT_INTERRUPTED);
        }
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let size = match fs::metadata(path) {
            Ok(m) => m.len(),
            Err(_) => continue,
        };
        if size < opts.min_size {
            continue;
        }
        size_map.entry(size).or_default().push(path.to_path_buf());
    }

    let mut hash_map: HashMap<String, Vec<PathBuf>> = HashMap::new();
    let candidates = size_map.into_values().filter(|v| v.len() > 1);

    for group in candidates {
        for path in group {
            if signal::is_interrupted() {
                return Ok(exit::EXIT_INTERRUPTED);
            }
            match file_hash(&path) {
                Ok(hash) => {
                    hash_map.entry(hash).or_default().push(path);
                }
                Err(e) => {
                    logger::log_warn!("skip {:?}: {}", path, e);
                }
            }
        }
    }

    let mut dup_groups = 0;
    let mut dup_files = 0u64;

    for (hash, paths) in &hash_map {
        if paths.len() < 2 {
            continue;
        }
        dup_groups += 1;
        println!("--- duplicate ({}): {} file(s) ---", hash, paths.len());
        for (i, p) in paths.iter().enumerate() {
            println!("  {}", p.display());
            if opts.delete && i > 0 {
                if let Err(e) = fs::remove_file(p) {
                    logger::log_error!("delete failed {:?}: {}", p, e);
                } else {
                    logger::log_info!("deleted {:?}", p);
                }
            }
        }
        dup_files += paths.len() as u64 - 1;
    }

    if dup_groups == 0 {
        logger::log_info!("no duplicates found");
    } else {
        logger::log_info!("{} group(s), {} redundant file(s)", dup_groups, dup_files);
    }

    Ok(exit::EXIT_OK)
}
