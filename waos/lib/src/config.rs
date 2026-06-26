use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PathsConfig {
    pub data_dir: Option<String>,
    pub accounts_dir: Option<String>,
    pub sessions_dir: Option<String>,
    pub media_dir: Option<String>,
    pub frontend_dir: Option<String>,
    pub main_db: Option<String>,
    pub dashboard: Option<String>,
    pub dashboard_delta: Option<String>,
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            data_dir: None,
            accounts_dir: None,
            sessions_dir: None,
            media_dir: None,
            frontend_dir: None,
            main_db: None,
            dashboard: None,
            dashboard_delta: None,
        }
    }
}

impl PathsConfig {
    fn fallback_data_dir() -> PathBuf {
        dirs::data_dir()
            .map(|d| d.join("waos"))
            .unwrap_or_else(|| PathBuf::from("./srv/waos"))
    }

    pub fn data_dir(&self) -> PathBuf {
        self.data_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(Self::fallback_data_dir)
    }

    pub fn accounts_dir(&self) -> PathBuf {
        self.accounts_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.data_dir().join("accounts"))
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.sessions_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.data_dir().join("sessions"))
    }

    pub fn media_dir(&self) -> PathBuf {
        self.media_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.data_dir().join("media"))
    }

    pub fn frontend_dir(&self) -> PathBuf {
        self.frontend_dir
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.data_dir().join("web"))
    }

    pub fn main_db(&self) -> PathBuf {
        self.main_db
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.data_dir().join("waos.db"))
    }

    pub fn dashboard(&self) -> PathBuf {
        self.dashboard
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.data_dir().join("dashboard.json"))
    }

    pub fn dashboard_delta(&self) -> PathBuf {
        self.dashboard_delta
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.data_dir().join("dashboard.delta"))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerConfig {
    pub enabled: Option<bool>,
    pub port: Option<u16>,
    pub bind: Option<String>,
    pub no_frontend: Option<bool>,
    pub jwt_secret: Option<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            enabled: Some(false),
            port: Some(8080),
            bind: Some("127.0.0.1".to_string()),
            no_frontend: Some(false),
            jwt_secret: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccountConfig {
    pub alias: String,
    pub phone: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentConfig {
    pub webhook_url: Option<String>,
    pub webhook_token: Option<String>,
    pub embedding_provider: Option<String>,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<usize>,
    pub embedding_api_key: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub paths: PathsConfig,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default = "default_accounts")]
    pub accounts: Vec<AccountConfig>,
    pub agent: Option<AgentConfig>,
}

fn default_accounts() -> Vec<AccountConfig> {
    vec![AccountConfig {
        alias: "default".to_string(),
        phone: None,
    }]
}

impl Default for Config {
    fn default() -> Self {
        Self {
            paths: PathsConfig::default(),
            server: ServerConfig::default(),
            accounts: default_accounts(),
            agent: None,
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let mut config = Config::default();

        if let Ok(cfg) = Self::load_toml("config.toml") {
            config = config.merge_overlay(cfg);
        }

        if let Some(cfg) = Self::load_env() {
            config = config.merge_overlay(cfg);
        }

        let sys = get_system_config_path();
        if !Self::config_toml_exists() && !Self::env_file_exists() {
            if let Ok(cfg) = Self::load_toml(sys.to_str().unwrap_or("")) {
                config = config.merge_overlay(cfg);
            }
        }

        config
    }

    fn merge_overlay(self, overlay: Self) -> Self {
        Self {
            paths: PathsConfig {
                data_dir: overlay.paths.data_dir.or(self.paths.data_dir),
                accounts_dir: overlay.paths.accounts_dir.or(self.paths.accounts_dir),
                sessions_dir: overlay.paths.sessions_dir.or(self.paths.sessions_dir),
                media_dir: overlay.paths.media_dir.or(self.paths.media_dir),
                frontend_dir: overlay.paths.frontend_dir.or(self.paths.frontend_dir),
                main_db: overlay.paths.main_db.or(self.paths.main_db),
                dashboard: overlay.paths.dashboard.or(self.paths.dashboard),
                dashboard_delta: overlay.paths.dashboard_delta.or(self.paths.dashboard_delta),
            },
            server: ServerConfig {
                enabled: overlay.server.enabled.or(self.server.enabled),
                port: overlay.server.port.or(self.server.port),
                bind: overlay.server.bind.or(self.server.bind),
                no_frontend: overlay.server.no_frontend.or(self.server.no_frontend),
                jwt_secret: overlay.server.jwt_secret.or(self.server.jwt_secret),
            },
            accounts: self.accounts,
            agent: match (self.agent, overlay.agent) {
                (Some(base), Some(over)) => Some(AgentConfig {
                    webhook_url: over.webhook_url.or(base.webhook_url),
                    webhook_token: over.webhook_token.or(base.webhook_token),
                    embedding_provider: over.embedding_provider.or(base.embedding_provider),
                    embedding_model: over.embedding_model.or(base.embedding_model),
                    embedding_dimensions: over.embedding_dimensions.or(base.embedding_dimensions),
                    embedding_api_key: over.embedding_api_key.or(base.embedding_api_key),
                }),
                (Some(base), None) => Some(base),
                (None, over) => over,
            },
        }
    }

    fn load_toml(path: &str) -> Result<Self, ()> {
        let content = std::fs::read_to_string(path).map_err(|_| ())?;
        toml::from_str(&content).map_err(|_| ())
    }

    fn load_env() -> Option<Self> {
        let toml_str = env_to_toml(".env")?;
        toml::from_str(&toml_str).ok()
    }

    fn config_toml_exists() -> bool {
        std::path::Path::new("config.toml").exists()
    }

    fn env_file_exists() -> bool {
        std::path::Path::new(".env").exists()
    }

    /// Resolve accounts from the main DB (source of truth).
    /// Config `[[accounts]]` is only used for bootstrapping the first account
    /// when the DB is empty. After that, all account management is done via
    /// the REST API / main DB directly.
    pub async fn resolve_accounts(&mut self, main_conn: &turso::Connection) {
        let mut rows = match main_conn
            .query("SELECT alias, phone FROM accounts ORDER BY alias", ())
            .await
        {
            Ok(r) => r,
            Err(_) => return,
        };
        let mut db_accounts = Vec::new();
        loop {
            let row = match rows.next().await {
                Ok(Some(r)) => r,
                Ok(None) => break,
                Err(_) => break,
            };
            let alias = match row.get_value(0) {
                Ok(turso::Value::Text(s)) => s,
                _ => continue,
            };
            let phone = match row.get_value(1) {
                Ok(turso::Value::Text(s)) => Some(s),
                _ => None,
            };
            db_accounts.push(AccountConfig { alias, phone });
        }
        if !db_accounts.is_empty() {
            self.accounts = db_accounts;
            return;
        }

        // DB empty — bootstrap from config (first run only)
        let now = chrono::Utc::now().timestamp();
        for acct in &self.accounts {
            let phone = acct.phone.as_deref().unwrap_or("");
            let _ = main_conn.execute(
                "INSERT INTO accounts (alias, phone, status, created_at, updated_at)
                 VALUES (?1, ?2, 'disconnected', ?3, ?3)
                 ON CONFLICT(alias) DO UPDATE SET phone = ?2, updated_at = ?3",
                turso::params::Params::Positional(vec![
                    turso::Value::from(acct.alias.as_str()),
                    turso::Value::from(phone),
                    turso::Value::from(now),
                ]),
            ).await;
        }
    }

    pub fn data_dir(&self) -> PathBuf {
        self.paths.data_dir()
    }
    pub fn accounts_dir(&self) -> PathBuf {
        self.paths.accounts_dir()
    }
    pub fn sessions_dir(&self) -> PathBuf {
        self.paths.sessions_dir()
    }
    pub fn media_dir(&self) -> PathBuf {
        self.paths.media_dir()
    }
    pub fn frontend_dir(&self) -> PathBuf {
        self.paths.frontend_dir()
    }
    pub fn main_db(&self) -> PathBuf {
        self.paths.main_db()
    }
    pub fn dashboard_path(&self) -> PathBuf {
        self.paths.dashboard()
    }
    pub fn dashboard_delta_path(&self) -> PathBuf {
        self.paths.dashboard_delta()
    }

    pub fn account_db(&self, alias: &str) -> PathBuf {
        self.accounts_dir().join(format!("{alias}.db"))
    }

    pub fn session_dir(&self, alias: &str) -> PathBuf {
        self.sessions_dir().join(alias)
    }

    pub fn session_db(&self, alias: &str) -> PathBuf {
        self.session_dir(alias).join("session.db")
    }
}

fn get_system_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("/etc"))
        .join("waos")
        .join("config.toml")
}

fn env_to_toml(path: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;

    let mut sections: HashMap<String, Vec<(String, String)>> = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim().trim_matches('"').trim_matches('\'');

            if let Some(rest) = key.strip_prefix("WAOS_") {
                if rest.contains("__") {
                    let mut parts = rest.splitn(2, "__");
                    let section = parts.next().unwrap().to_lowercase();
                    let field = parts.next().unwrap().to_lowercase();
                    sections
                        .entry(section)
                        .or_default()
                        .push((field, value.to_string()));
                }
            }
        }
    }

    if sections.is_empty() {
        return None;
    }

    let mut toml_lines = Vec::new();
    let mut section_names: Vec<String> = sections.keys().cloned().collect();
    section_names.sort();

    for section in section_names {
        if let Some(fields) = sections.remove(&section) {
            toml_lines.push(format!("[{section}]"));
            let mut sorted = fields;
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
            for (field, value) in sorted {
                toml_lines.push(format!("{field} = {}", escape_toml_value(&value)));
            }
        }
    }

    Some(toml_lines.join("\n"))
}

fn escape_toml_value(s: &str) -> String {
    if s.eq_ignore_ascii_case("true") || s.eq_ignore_ascii_case("false") {
        s.to_lowercase()
    } else if s.parse::<i64>().is_ok() || s.parse::<f64>().is_ok() {
        s.to_string()
    } else {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    }
}
