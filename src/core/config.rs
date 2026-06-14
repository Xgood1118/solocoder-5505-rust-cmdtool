use anyhow::Result;
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Default, Clone)]
pub struct AppConfig {
    pub general: Option<GeneralConfig>,
    pub rename: Option<RenameConfig>,
    pub csvkit: Option<CsvkitConfig>,
    pub recent: Option<RecentConfig>,
    pub finddup: Option<FinddupConfig>,
    pub tree: Option<TreeConfig>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct GeneralConfig {
    pub no_color: Option<bool>,
    pub verbose: Option<bool>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct RenameConfig {
    pub dry_run: Option<bool>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct CsvkitConfig {
    pub delimiter: Option<char>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct RecentConfig {
    pub days: Option<u64>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct FinddupConfig {
    pub min_size: Option<u64>,
}

#[derive(Debug, Deserialize, Default, Clone)]
pub struct TreeConfig {
    pub max_depth: Option<usize>,
}

fn global_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("rtool").join("config.toml"))
}

fn project_config_path() -> PathBuf {
    PathBuf::from(".rtool.toml")
}

fn load_toml(path: &PathBuf) -> Option<AppConfig> {
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

pub fn load() -> Result<AppConfig> {
    let mut config = AppConfig::default();

    if let Some(global_path) = global_config_path() {
        if let Some(c) = load_toml(&global_path) {
            merge_config(&mut config, c);
        }
    }

    let proj_path = project_config_path();
    if let Some(c) = load_toml(&proj_path) {
        merge_config(&mut config, c);
    }

    Ok(config)
}

fn merge_config(base: &mut AppConfig, overlay: AppConfig) {
    if overlay.general.is_some() {
        base.general = overlay.general;
    }
    if overlay.rename.is_some() {
        base.rename = overlay.rename;
    }
    if overlay.csvkit.is_some() {
        base.csvkit = overlay.csvkit;
    }
    if overlay.recent.is_some() {
        base.recent = overlay.recent;
    }
    if overlay.finddup.is_some() {
        base.finddup = overlay.finddup;
    }
    if overlay.tree.is_some() {
        base.tree = overlay.tree;
    }
}
