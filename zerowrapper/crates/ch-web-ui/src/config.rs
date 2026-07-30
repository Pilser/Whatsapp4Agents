pub struct WebUiConfig {
    pub bind: String,
    pub agent: String,
    pub upload_dir: std::path::PathBuf,
}

impl Default for WebUiConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:3001".to_string(),
            agent: "default".to_string(),
            upload_dir: std::path::PathBuf::from("./uploads"),
        }
    }
}
