use anyhow::{Context, Result};
use chrono::Local;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::config::AppConfig;
use crate::core::exit;
use crate::core::history::get_connection;
use crate::core::logger;
use crate::core::signal;

#[derive(Debug, Clone)]
pub enum RenameMode {
    Numbered { start: u64, width: usize },
    Datestamp,
    HashPrefix,
    RegexReplace { pattern: String, replacement: String },
}

pub struct RenameOpts {
    pub paths: Vec<PathBuf>,
    pub mode: RenameMode,
    pub dry_run: bool,
    pub yes: bool,
    pub undo: bool,
    pub prefix: Option<String>,
    pub suffix: Option<String>,
}

fn compute_hash(path: &Path) -> Result<String> {
    let data = fs::read(path).with_context(|| format!("read {:?}", path))?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let hash = hasher.finalize();
    Ok(format!("{:x}", hash)[..8].to_string())
}

fn generate_new_name(
    original: &Path,
    idx: u64,
    mode: &RenameMode,
    prefix: &Option<String>,
    suffix: &Option<String>,
) -> Result<PathBuf> {
    let stem = original
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let ext = original
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();

    let new_stem = match mode {
        RenameMode::Numbered { start, width } => {
            format!("{:0>width$}", start + idx, width = *width)
        }
        RenameMode::Datestamp => {
            let meta = fs::metadata(original)?;
            let modified: chrono::DateTime<Local> = meta.modified()?.into();
            modified.format("%Y%m%d_%H%M%S").to_string()
        }
        RenameMode::HashPrefix => {
            let h = compute_hash(original)?;
            format!("{}_{}", h, stem)
        }
        RenameMode::RegexReplace { pattern, replacement } => {
            let re = Regex::new(pattern).with_context(|| format!("invalid regex: {}", pattern))?;
            re.replace(&stem, replacement.as_str()).to_string()
        }
    };

    let mut result = String::new();
    if let Some(p) = prefix {
        result.push_str(p);
    }
    result.push_str(&new_stem);
    if let Some(s) = suffix {
        result.push_str(s);
    }
    if !ext.is_empty() {
        result.push('.');
        result.push_str(&ext);
    }

    Ok(original.with_file_name(result))
}

fn ensure_rename_table() -> Result<()> {
    let db = get_connection()?;
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS rename_map (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            batch_id TEXT NOT NULL,
            old_path TEXT NOT NULL,
            new_path TEXT NOT NULL
        );"
    )?;
    Ok(())
}

fn save_rename_map(renames: &[(PathBuf, PathBuf)]) -> Result<()> {
    ensure_rename_table()?;
    let db = get_connection()?;
    let batch_id = Local::now().format("%Y%m%d%H%M%S%3f").to_string();
    for (old, new) in renames {
        db.execute(
            "INSERT INTO rename_map (batch_id, old_path, new_path) VALUES (?1, ?2, ?3)",
            rusqlite::params![batch_id, old.to_string_lossy(), new.to_string_lossy()],
        )?;
    }
    Ok(())
}

fn is_tty_stdin() -> bool {
    atty::is(atty::Stream::Stdin)
}

fn confirm_rename(count: usize, auto_yes: bool) -> bool {
    if auto_yes {
        return true;
    }
    if !is_tty_stdin() {
        logger::log_warn!("not a TTY: skipping interactive confirmation. pass -y/--yes to force execute");
        return false;
    }
    match dialoguer::Confirm::new()
        .with_prompt(format!("rename {} file(s)?", count))
        .default(false)
        .interact()
    {
        Ok(v) => v,
        Err(e) => {
            logger::log_error!("confirmation failed: {}", e);
            false
        }
    }
}

pub fn run(opts: &RenameOpts, _config: &AppConfig) -> Result<i32> {
    ensure_rename_table()?;

    if opts.undo {
        return run_undo();
    }

    if opts.paths.is_empty() {
        logger::log_error!("no files specified");
        return Ok(exit::EXIT_INVALID_ARGS);
    }

    let mut renames: Vec<(PathBuf, PathBuf)> = Vec::new();

    for (i, path) in opts.paths.iter().enumerate() {
        if signal::is_interrupted() {
            return Ok(exit::EXIT_UNKNOWN);
        }
        if !path.exists() {
            logger::log_warn!("skip nonexistent: {:?}", path);
            continue;
        }
        let new_path = generate_new_name(path, i as u64, &opts.mode, &opts.prefix, &opts.suffix)?;
        if new_path != *path {
            renames.push((path.clone(), new_path));
        }
    }

    if renames.is_empty() {
        logger::log_info!("nothing to rename");
        return Ok(exit::EXIT_OK);
    }

    for (old, new) in &renames {
        if opts.dry_run {
            println!("{:?} -> {:?}", old, new);
        } else {
            logger::log_info!("{:?} -> {:?}", old, new);
        }
    }

    save_rename_map(&renames)?;

    if opts.dry_run {
        logger::log_info!(
            "dry run: {} file(s) would be renamed. pass --execute and confirm (or -y) to apply.",
            renames.len()
        );
        return Ok(exit::EXIT_OK);
    }

    if !confirm_rename(renames.len(), opts.yes) {
        logger::log_info!("aborted");
        return Ok(exit::EXIT_OK);
    }

    let mut errors = 0;
    for (old, new) in &renames {
        if signal::is_interrupted() {
            return Ok(exit::EXIT_UNKNOWN);
        }
        if let Err(e) = fs::rename(old, new) {
            logger::log_error!("failed: {:?} -> {:?}: {}", old, new, e);
            errors += 1;
        }
    }

    if errors > 0 {
        logger::log_warn!("{} error(s) during rename", errors);
        Ok(exit::EXIT_IO_ERROR)
    } else {
        logger::log_info!("renamed {} file(s)", renames.len());
        Ok(exit::EXIT_OK)
    }
}

fn run_undo() -> Result<i32> {
    ensure_rename_table()?;
    let db = get_connection()?;
    let mut stmt = db.prepare(
        "SELECT batch_id, old_path, new_path FROM rename_map ORDER BY id DESC"
    )?;

    let rows: Vec<(String, String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
        .filter_map(|r| r.ok())
        .collect();

    if rows.is_empty() {
        logger::log_info!("no rename history to undo");
        return Ok(exit::EXIT_OK);
    }

    let latest_batch = &rows[0].0;
    let batch: Vec<_> = rows.iter().filter(|r| r.0 == *latest_batch).collect();

    logger::log_info!("undoing batch {} with {} file(s)", latest_batch, batch.len());

    let mut errors = 0;
    for (_, old, new) in batch {
        if Path::new(new).exists() {
            if let Err(e) = fs::rename(new, old) {
                logger::log_error!("undo failed: {:?} -> {:?}: {}", new, old, e);
                errors += 1;
            } else {
                logger::log_info!("undo: {:?} -> {:?}", new, old);
            }
        } else {
            logger::log_warn!("skip missing {:?}", new);
        }
    }

    db.execute("DELETE FROM rename_map WHERE batch_id = ?1", rusqlite::params![latest_batch])?;

    if errors > 0 {
        Ok(exit::EXIT_IO_ERROR)
    } else {
        Ok(exit::EXIT_OK)
    }
}
