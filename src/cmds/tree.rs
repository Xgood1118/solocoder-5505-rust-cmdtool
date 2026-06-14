use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::config::AppConfig;
use crate::core::exit;
use crate::core::logger;
use crate::core::signal;

pub struct TreeOpts {
    pub directory: PathBuf,
    pub max_depth: Option<usize>,
    pub show_hidden: bool,
    pub dirs_only: bool,
}

fn print_tree(path: &Path, prefix: &str, depth: usize, opts: &TreeOpts) -> Result<()> {
    if signal::is_interrupted() {
        return Ok(());
    }

    let max = opts.max_depth.unwrap_or(usize::MAX);
    if depth > max {
        return Ok(());
    }

    let mut entries: Vec<_> = fs::read_dir(path)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            if !opts.show_hidden {
                let name = e.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') {
                    return false;
                }
            }
            true
        })
        .collect();

    entries.sort_by(|a, b| {
        let a_is_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let b_is_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
        b_is_dir.cmp(&a_is_dir).then_with(|| a.file_name().cmp(&b.file_name()))
    });

    let count = entries.len();
    for (i, entry) in entries.iter().enumerate() {
        if signal::is_interrupted() {
            return Ok(());
        }

        let is_last = i == count - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let extension = if is_last { "    " } else { "│   " };

        let name = entry.file_name().to_string_lossy().to_string();
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

        if opts.dirs_only && !is_dir {
            continue;
        }

        let display = if is_dir {
            format!("{}/", name)
        } else {
            name
        };

        println!("{}{}{}", prefix, connector, display);

        if is_dir {
            let new_prefix = format!("{}{}", prefix, extension);
            print_tree(&entry.path(), &new_prefix, depth + 1, opts)?;
        }
    }

    Ok(())
}

pub fn run(opts: &TreeOpts, _config: &AppConfig) -> Result<i32> {
    if !opts.directory.exists() {
        logger::log_error!("directory not found: {:?}", opts.directory);
        return Ok(exit::EXIT_INVALID_ARGS);
    }

    println!("{}", opts.directory.display());
    print_tree(&opts.directory, "", 1, opts)?;
    Ok(exit::EXIT_OK)
}
