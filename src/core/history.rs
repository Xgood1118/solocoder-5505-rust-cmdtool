use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;
use std::time::Instant;

fn history_db_path() -> Result<PathBuf> {
    let data_dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rtool");
    std::fs::create_dir_all(&data_dir)?;
    Ok(data_dir.join("history.db"))
}

pub struct HistoryRecord {
    pub id: i64,
    pub command: String,
    pub args_sanitized: String,
    pub started_at: String,
    pub duration_ms: u64,
    pub exit_code: i32,
}

pub struct History {
    conn: Connection,
    start: Instant,
    command: String,
    args_sanitized: String,
}

fn sanitize_args(args: &[String]) -> String {
    let sensitive_patterns = ["password", "pass", "pwd", "token", "secret", "key", "auth"];
    args.iter()
        .map(|a| {
            let lower = a.to_lowercase();
            if sensitive_patterns.iter().any(|p| lower.contains(p)) {
                "***".to_string()
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

impl History {
    pub fn new(command: &str, args: &[String]) -> Result<Self> {
        let db_path = history_db_path()?;
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                command TEXT NOT NULL,
                args_sanitized TEXT NOT NULL,
                started_at TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                exit_code INTEGER NOT NULL
            );"
        )?;
        Ok(History {
            conn,
            start: Instant::now(),
            command: command.to_string(),
            args_sanitized: sanitize_args(args),
        })
    }

    pub fn finish(self, exit_code: i32) -> Result<()> {
        let duration_ms = self.start.elapsed().as_millis() as u64;
        let started_at = chrono::Local::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO history (command, args_sanitized, started_at, duration_ms, exit_code) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![self.command, self.args_sanitized, started_at, duration_ms, exit_code],
        )?;
        Ok(())
    }

    pub fn list(conn: &Connection, limit: usize) -> Result<Vec<HistoryRecord>> {
        let mut stmt = conn.prepare(
            "SELECT id, command, args_sanitized, started_at, duration_ms, exit_code FROM history ORDER BY id DESC LIMIT ?1"
        )?;
        let rows = stmt.query_map(rusqlite::params![limit], |row| {
            Ok(HistoryRecord {
                id: row.get(0)?,
                command: row.get(1)?,
                args_sanitized: row.get(2)?,
                started_at: row.get(3)?,
                duration_ms: row.get(4)?,
                exit_code: row.get(5)?,
            })
        })?;
        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }
        Ok(records)
    }
}

pub fn get_connection() -> Result<Connection> {
    let db_path = history_db_path()?;
    let conn = Connection::open(&db_path)?;
    Ok(conn)
}
