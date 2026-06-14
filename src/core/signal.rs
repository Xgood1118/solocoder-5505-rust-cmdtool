use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

pub fn is_interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

pub fn setup_ctrlc(temp_files: Arc<std::sync::Mutex<Vec<std::path::PathBuf>>>) {
    ctrlc::set_handler(move || {
        INTERRUPTED.store(true, Ordering::SeqCst);
        eprintln!("\n[interrupted] cleaning up temp files...");
        if let Ok(files) = temp_files.lock() {
            for f in files.iter() {
                if f.exists() {
                    let _ = std::fs::remove_file(f);
                }
            }
        }
        std::process::exit(5);
    }).expect("failed to set ctrlc handler");
}
