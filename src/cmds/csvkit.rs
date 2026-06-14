use anyhow::{Context, Result};
use csv::{Reader, ReaderBuilder, Writer, WriterBuilder};
use std::cmp::Ordering;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::core::config::AppConfig;
use crate::core::exit;
use crate::core::logger;
use crate::core::signal;

#[derive(Debug)]
pub enum CsvAction {
    Merge { inputs: Vec<PathBuf>, output: PathBuf },
    Split { input: PathBuf, rows: usize, output_dir: PathBuf },
    Dedup { input: PathBuf, output: PathBuf, columns: Vec<usize> },
    Filter { input: PathBuf, output: PathBuf, column: usize, operator: String, value: String },
    AddHeader { input: PathBuf, output: PathBuf, headers: Vec<String> },
}

fn make_reader(path: &Path) -> Result<Reader<fs::File>> {
    Ok(ReaderBuilder::new()
        .flexible(true)
        .from_path(path)
        .with_context(|| format!("open csv {:?}", path))?)
}

fn make_writer(path: &Path) -> Result<Writer<fs::File>> {
    Ok(WriterBuilder::new()
        .from_path(path)
        .with_context(|| format!("create csv {:?}", path))?)
}

fn merge(inputs: &[PathBuf], output: &Path) -> Result<i32> {
    if inputs.is_empty() {
        logger::log_error!("no input files");
        return Ok(exit::EXIT_INVALID_ARGS);
    }

    let mut writer = make_writer(output)?;
    let mut header_written = false;
    let mut total_rows: u64 = 0;

    for input in inputs {
        if signal::is_interrupted() {
            return Ok(exit::EXIT_UNKNOWN);
        }
        let mut reader = make_reader(input)?;
        let headers = reader.headers()?.clone();

        if !header_written {
            writer.write_record(&headers)?;
            header_written = true;
        }

        for result in reader.records() {
            if signal::is_interrupted() {
                return Ok(exit::EXIT_UNKNOWN);
            }
            let record = result?;
            writer.write_record(&record)?;
            total_rows += 1;
        }
        logger::log_info!("merged from {:?}: done", input);
    }

    writer.flush()?;
    logger::log_info!("merged {} rows into {:?}", total_rows, output);
    Ok(exit::EXIT_OK)
}

fn split(input: &Path, rows_per_file: usize, output_dir: &Path) -> Result<i32> {
    fs::create_dir_all(output_dir)?;
    let mut reader = make_reader(input)?;
    let headers = reader.headers()?.clone();

    let mut file_idx: usize = 1;
    let mut row_count: usize = 0;
    let mut writer = make_writer(&output_dir.join(format!("split_{:04}.csv", file_idx)))?;
    writer.write_record(&headers)?;

    for result in reader.records() {
        if signal::is_interrupted() {
            return Ok(exit::EXIT_UNKNOWN);
        }
        let record = result?;
        writer.write_record(&record)?;
        row_count += 1;

        if row_count >= rows_per_file {
            writer.flush()?;
            file_idx += 1;
            row_count = 0;
            writer = make_writer(&output_dir.join(format!("split_{:04}.csv", file_idx)))?;
            writer.write_record(&headers)?;
        }
    }

    writer.flush()?;
    logger::log_info!("split into {} file(s)", file_idx);
    Ok(exit::EXIT_OK)
}

fn dedup(input: &Path, output: &Path, columns: &[usize]) -> Result<i32> {
    let mut reader = make_reader(input)?;
    let headers = reader.headers()?.clone();
    let mut writer = make_writer(output)?;
    writer.write_record(&headers)?;

    let mut seen: HashSet<String> = HashSet::new();
    let mut total = 0u64;
    let mut dupes = 0u64;

    for result in reader.records() {
        if signal::is_interrupted() {
            return Ok(exit::EXIT_UNKNOWN);
        }
        let record = result?;
        let key: String = if columns.is_empty() {
            record.iter().collect::<Vec<_>>().join("|")
        } else {
            columns
                .iter()
                .filter_map(|&i| record.get(i))
                .collect::<Vec<_>>()
                .join("|")
        };

        total += 1;
        if seen.insert(key) {
            writer.write_record(&record)?;
        } else {
            dupes += 1;
        }
    }

    writer.flush()?;
    logger::log_info!(
        "{} total, {} duplicates removed, {} unique rows",
        total,
        dupes,
        total - dupes
    );
    Ok(exit::EXIT_OK)
}

fn col_value_cmp(a: &str, b: &str) -> Option<Ordering> {
    let (na, nb) = (a.parse::<f64>(), b.parse::<f64>());
    if let (Ok(na), Ok(nb)) = (na, nb) {
        return na.partial_cmp(&nb);
    }
    Some(a.cmp(b))
}

fn col_value_contains(haystack: &str, needle: &str) -> bool {
    haystack.contains(needle)
}

fn eval_filter(col_val: &str, op: &str, val: &str) -> bool {
    match op {
        "eq" => col_val == val,
        "ne" => col_val != val,
        "gt" => col_value_cmp(col_val, val).map_or(false, |o| o.is_gt()),
        "lt" => col_value_cmp(col_val, val).map_or(false, |o| o.is_lt()),
        "ge" => col_value_cmp(col_val, val).map_or(false, |o| o.is_ge()),
        "le" => col_value_cmp(col_val, val).map_or(false, |o| o.is_le()),
        "contains" => col_value_contains(col_val, val),
        _ => false,
    }
}

fn filter(input: &Path, output: &Path, column: usize, operator: &str, value: &str) -> Result<i32> {
    let mut reader = make_reader(input)?;
    let headers = reader.headers()?.clone();
    let mut writer = make_writer(output)?;
    writer.write_record(&headers)?;

    let mut matched = 0u64;
    let mut total = 0u64;

    for result in reader.records() {
        if signal::is_interrupted() {
            return Ok(exit::EXIT_UNKNOWN);
        }
        let record = result?;
        total += 1;

        let col_val = record.get(column).unwrap_or("");
        if eval_filter(col_val, operator, value) {
            writer.write_record(&record)?;
            matched += 1;
        }
    }

    writer.flush()?;
    logger::log_info!("{} of {} rows matched filter", matched, total);
    Ok(exit::EXIT_OK)
}

fn add_header(input: &Path, output: &Path, headers: &[String]) -> Result<i32> {
    let mut reader = make_reader(input)?;
    let mut writer = make_writer(output)?;

    writer.write_record(headers)?;

    for result in reader.records() {
        if signal::is_interrupted() {
            return Ok(exit::EXIT_UNKNOWN);
        }
        let record = result?;
        writer.write_record(&record)?;
    }

    writer.flush()?;
    logger::log_info!("added {} header(s)", headers.len());
    Ok(exit::EXIT_OK)
}

pub fn run(action: &CsvAction, _config: &AppConfig) -> Result<i32> {
    match action {
        CsvAction::Merge { inputs, output } => merge(inputs, output),
        CsvAction::Split { input, rows, output_dir } => split(input, *rows, output_dir),
        CsvAction::Dedup { input, output, columns } => dedup(input, output, columns),
        CsvAction::Filter { input, output, column, operator, value } => {
            filter(input, output, *column, operator, value)
        }
        CsvAction::AddHeader { input, output, headers } => add_header(input, output, headers),
    }
}
