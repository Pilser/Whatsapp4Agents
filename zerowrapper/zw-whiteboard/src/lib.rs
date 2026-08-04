pub mod monitor;
pub mod parser;
pub mod resolve;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;
use tokio::sync::Mutex;

use crate::monitor::MonitorCore;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Pin {
    pub name: String,
    pub url: String,
    pub mime: String,
    pub size: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ParsedBoard {
    pub body_md: String,
    pub pins: Vec<Pin>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WhiteboardEvent {
    WhiteboardUpdate {
        session_id: String,
        board: ParsedBoard,
    },
}

const SEED_TEMPLATE: &str = "# Whiteboard\n";

#[derive(Clone)]
pub struct WhiteboardHandle {
    dir: PathBuf,
    cache: Arc<zw_cache::CacheManager>,
    poll_interval: Duration,
    state: Arc<Mutex<MonitorCore>>,
    tx: broadcast::Sender<WhiteboardEvent>,
}

impl WhiteboardHandle {
    pub fn new(
        dir: PathBuf,
        cache: Arc<zw_cache::CacheManager>,
        poll_interval: Duration,
    ) -> Self {
        std::fs::create_dir_all(&dir).ok();
        let (tx, _) = broadcast::channel(64);
        Self {
            dir,
            cache,
            poll_interval,
            state: Arc::new(Mutex::new(MonitorCore::default())),
            tx,
        }
    }

    pub fn spawn(&self) -> tokio::task::JoinHandle<()> {
        let dir = self.dir.clone();
        let cache = self.cache.clone();
        let poll_interval = self.poll_interval;
        let state = self.state.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(poll_interval);
            loop {
                ticker.tick().await;
                let sessions: Vec<String> = {
                    let core = state.lock().await;
                    core.seen.keys().cloned().collect()
                };
                for sid in sessions {
                    let path = dir.join(format!("{sid}.wb"));
                    let changed = {
                        let mut core = state.lock().await;
                        core.has_changed(&sid, &path)
                    };
                    if changed {
                        if let Some(board) = read_board_from(&dir, &cache, &sid) {
                            let mut core = state.lock().await;
                            core.latest.insert(sid.clone(), board.clone());
                            let _ = tx.send(WhiteboardEvent::WhiteboardUpdate {
                                session_id: sid.clone(),
                                board,
                            });
                        }
                    }
                }
            }
        })
    }

    pub async fn ensure(&self, session_id: &str) -> anyhow::Result<()> {
        let path = self.dir.join(format!("{session_id}.wb"));
        if !path.exists() {
            std::fs::write(&path, SEED_TEMPLATE)?;
            set_world_writable(&path);
        }
        set_world_writable(&path);
        let board = read_board_from(&self.dir, &self.cache, session_id).unwrap_or_else(|| ParsedBoard {
            body_md: String::new(),
            pins: Vec::new(),
        });
        let snapshot = {
            let core = self.state.lock().await;
            core.snapshot(&path)
        };
        let mut core = self.state.lock().await;
        core.seen.insert(session_id.to_string(), snapshot);
        core.latest.insert(session_id.to_string(), board);
        Ok(())
    }

    pub async fn latest(&self, session_id: &str) -> Option<ParsedBoard> {
        self.state.lock().await.latest.get(session_id).cloned()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WhiteboardEvent> {
        self.tx.subscribe()
    }

    pub fn dir(&self) -> &PathBuf {
        &self.dir
    }
}

fn read_board_from(dir: &PathBuf, cache: &Arc<zw_cache::CacheManager>, session_id: &str) -> Option<ParsedBoard> {
    let path = dir.join(format!("{session_id}.wb"));
    if !path.exists() {
        return None;
    }
    let raw = std::fs::read_to_string(&path).ok()?;
    let parsed = parser::parse_markdown(&raw);
    let board = resolve::resolve_board(dir, &parsed, cache).ok()?;
    Some(board)
}

fn set_world_writable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o666));
}
