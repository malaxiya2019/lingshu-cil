use chrono::Local;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Logger {
    log_file: PathBuf,
    buffer: Mutex<Vec<String>>,
}

impl Logger {
    pub fn new(name: &str) -> anyhow::Result<Self> {
        let log_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("lingshu")
            .join("logs");
        fs::create_dir_all(&log_dir)?;

        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let log_file = log_dir.join(format!("{}_{}.log", name, timestamp));

        Ok(Self {
            log_file,
            buffer: Mutex::new(Vec::new()),
        })
    }

    pub fn log(&self, level: &str, target: &str, message: &str) {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let entry = format!("[{}] [{}] [{}] {}\n", timestamp, level, target, message);

        if let Ok(mut buf) = self.buffer.lock() {
            buf.push(entry.clone());
        }

        // Also write to stderr immediately for live tailing
        eprint!("{}", entry);

        // Async flush to file
        let _ = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)
            .and_then(|mut f| f.write_all(entry.as_bytes()));
    }

    pub fn info(&self, target: &str, message: &str) {
        self.log("INFO", target, message);
    }

    pub fn warn(&self, target: &str, message: &str) {
        self.log("WARN", target, message);
    }

    pub fn error(&self, target: &str, message: &str) {
        self.log("ERROR", target, message);
    }

    pub fn debug(&self, target: &str, message: &str) {
        self.log("DEBUG", target, message);
    }

    pub fn path(&self) -> &PathBuf {
        &self.log_file
    }
}
