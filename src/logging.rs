// Logger - kept for future use
#[allow(dead_code)]
pub struct Logger { log_file: std::path::PathBuf }
#[allow(dead_code)]
impl Logger {
    pub fn new(_name: &str) -> anyhow::Result<Self> {
        Ok(Self { log_file: std::path::PathBuf::from("/tmp/lingshu.log") })
    }
    pub fn path(&self) -> &std::path::PathBuf { &self.log_file }
}
