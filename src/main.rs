mod cmds;
mod core;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::sync::Arc;

use crate::cmds::csvkit::CsvAction;
use crate::core::config::AppConfig;
use crate::core::exit;
use crate::core::history::History;
use crate::core::logger;
use crate::core::signal;

#[derive(Parser)]
#[command(name = "rtool", version, about = "个人命令行工具集")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(long, global = true)]
    no_color: bool,

    #[arg(long, global = true)]
    verbose: bool,

    #[arg(long, global = true)]
    config: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    Rename {
        #[arg(long, short)]
        mode: Option<String>,

        #[arg(long, default_value_t = 1)]
        start: u64,

        #[arg(long, default_value_t = 3)]
        width: usize,

        #[arg(long)]
        pattern: Option<String>,

        #[arg(long)]
        replacement: Option<String>,

        #[arg(long)]
        prefix: Option<String>,

        #[arg(long)]
        suffix: Option<String>,

        #[arg(long)]
        execute: bool,

        #[arg(long)]
        undo: bool,

        files: Vec<PathBuf>,
    },

    Csvkit {
        #[command(subcommand)]
        action: CsvCommands,
    },

    Recent {
        #[arg(long, short, default_value = ".")]
        directory: PathBuf,

        #[arg(long, short, default_value_t = 7)]
        days: u64,

        #[arg(long, short)]
        pattern: Option<String>,
    },

    Finddup {
        #[arg(long, short, default_value = ".")]
        directory: PathBuf,

        #[arg(long, default_value_t = 0)]
        min_size: u64,

        #[arg(long)]
        delete: bool,
    },

    Tree {
        #[arg(long, short, default_value = ".")]
        directory: PathBuf,

        #[arg(long)]
        depth: Option<usize>,

        #[arg(long)]
        hidden: bool,

        #[arg(long)]
        dirs_only: bool,
    },
}

#[derive(Subcommand)]
enum CsvCommands {
    Merge {
        #[arg(long, short)]
        output: PathBuf,

        inputs: Vec<PathBuf>,
    },

    Split {
        #[arg(long, short)]
        input: PathBuf,

        #[arg(long, default_value_t = 10000)]
        rows: usize,

        #[arg(long, short)]
        output_dir: Option<PathBuf>,
    },

    Dedup {
        #[arg(long, short)]
        input: PathBuf,

        #[arg(long, short)]
        output: PathBuf,

        #[arg(long, short)]
        columns: Option<String>,
    },

    Filter {
        #[arg(long, short)]
        input: PathBuf,

        #[arg(long, short)]
        output: PathBuf,

        #[arg(long, default_value_t = 0)]
        column: usize,

        #[arg(long)]
        op: String,

        #[arg(long)]
        value: String,
    },

    Header {
        #[arg(long, short)]
        input: PathBuf,

        #[arg(long, short)]
        output: PathBuf,

        headers: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    if cli.no_color {
        colored::control::set_override(false);
    } else {
        logger::init();
    }

    let temp_files: Arc<std::sync::Mutex<Vec<PathBuf>>> = Arc::new(std::sync::Mutex::new(Vec::new()));
    signal::setup_ctrlc(temp_files.clone());

    let config = core::config::load().unwrap_or_default();

    let cmd_name = match &cli.command {
        Commands::Rename { .. } => "rename",
        Commands::Csvkit { .. } => "csvkit",
        Commands::Recent { .. } => "recent",
        Commands::Finddup { .. } => "finddup",
        Commands::Tree { .. } => "tree",
    };

    let args: Vec<String> = std::env::args().skip(1).collect();
    let history = History::new(cmd_name, &args).ok();

    let result = run_command(&cli, &config);

    let exit_code = result.unwrap_or_else(|e| {
        logger::log_error!("{}: {}", cmd_name, e);
        exit::EXIT_GENERAL
    });

    if let Some(h) = history {
        let _ = h.finish(exit_code);
    }

    std::process::exit(exit_code);
}

fn run_command(cli: &Cli, config: &AppConfig) -> Result<i32, anyhow::Error> {
    match &cli.command {
        Commands::Rename {
            mode, start, width, pattern, replacement, prefix, suffix, execute, undo, files,
        } => {
            let rename_mode = match mode.as_deref() {
                Some("numbered") | None => cmds::rename::RenameMode::Numbered {
                    start: *start,
                    width: *width,
                },
                Some("date") => cmds::rename::RenameMode::Datestamp,
                Some("hash") => cmds::rename::RenameMode::HashPrefix,
                Some("regex") => {
                    let pat = pattern.clone().unwrap_or_default();
                    let repl = replacement.clone().unwrap_or_default();
                    cmds::rename::RenameMode::RegexReplace {
                        pattern: pat,
                        replacement: repl,
                    }
                }
                _ => {
                    logger::log_error!("unknown mode: {}. use: numbered|date|hash|regex", mode.as_deref().unwrap_or(""));
                    return Ok(exit::EXIT_INVALID_ARGS);
                }
            };

            let dry_run = if *execute {
                false
            } else if let Some(ref rc) = config.rename {
                rc.dry_run.unwrap_or(true)
            } else {
                true
            };

            let opts = cmds::rename::RenameOpts {
                paths: files.clone(),
                mode: rename_mode,
                dry_run,
                undo: *undo,
                prefix: prefix.clone(),
                suffix: suffix.clone(),
            };
            cmds::rename::run(&opts, config)
        }

        Commands::Csvkit { action } => {
            let csv_action = match action {
                CsvCommands::Merge { output, inputs } => CsvAction::Merge {
                    inputs: inputs.clone(),
                    output: output.clone(),
                },
                CsvCommands::Split { input, rows, output_dir } => CsvAction::Split {
                    input: input.clone(),
                    rows: *rows,
                    output_dir: output_dir.clone().unwrap_or_else(|| {
                        input.parent().unwrap_or(PathBuf::from(".").as_path()).join("split")
                    }),
                },
                CsvCommands::Dedup { input, output, columns } => {
                    let cols: Vec<usize> = columns
                        .as_ref()
                        .map(|s| s.split(',').filter_map(|n| n.trim().parse().ok()).collect())
                        .unwrap_or_default();
                    CsvAction::Dedup {
                        input: input.clone(),
                        output: output.clone(),
                        columns: cols,
                    }
                }
                CsvCommands::Filter { input, output, column, op, value } => CsvAction::Filter {
                    input: input.clone(),
                    output: output.clone(),
                    column: *column,
                    operator: op.clone(),
                    value: value.clone(),
                },
                CsvCommands::Header { input, output, headers } => CsvAction::AddHeader {
                    input: input.clone(),
                    output: output.clone(),
                    headers: headers.clone(),
                },
            };
            cmds::csvkit::run(&csv_action, config)
        }

        Commands::Recent { directory, days, pattern } => {
            let opts = cmds::recent::RecentOpts {
                directory: directory.clone(),
                days: config.recent.as_ref().and_then(|r| r.days).unwrap_or(*days),
                pattern: pattern.clone(),
            };
            cmds::recent::run(&opts, config)
        }

        Commands::Finddup { directory, min_size, delete } => {
            let opts = cmds::finddup::FinddupOpts {
                directory: directory.clone(),
                min_size: config.finddup.as_ref().and_then(|f| f.min_size).unwrap_or(*min_size),
                delete: *delete,
            };
            cmds::finddup::run(&opts, config)
        }

        Commands::Tree { directory, depth, hidden, dirs_only } => {
            let opts = cmds::tree::TreeOpts {
                directory: directory.clone(),
                max_depth: depth.or(config.tree.as_ref().and_then(|t| t.max_depth)),
                show_hidden: *hidden,
                dirs_only: *dirs_only,
            };
            cmds::tree::run(&opts, config)
        }
    }
}
