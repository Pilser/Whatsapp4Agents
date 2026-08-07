use std::collections::HashMap;
use std::path::Path;

use crate::ParsedBoard;

#[derive(Default)]
pub struct MonitorCore {
    pub seen: HashMap<String, FileSnapshot>,
    pub rev: HashMap<String, i64>,
    pub latest: HashMap<String, ParsedBoard>,
}

#[derive(Debug, Clone)]
pub struct FileSnapshot {
    pub mtime: Option<std::time::SystemTime>,
    pub size: u64,
}

impl MonitorCore {
    pub fn snapshot(&self, path: &Path) -> FileSnapshot {
        match std::fs::metadata(path) {
            Ok(meta) => FileSnapshot {
                mtime: meta.modified().ok(),
                size: meta.len(),
            },
            Err(_) => FileSnapshot { mtime: None, size: 0 },
        }
    }

    pub fn has_changed(&mut self, sid: &str, path: &Path) -> bool {
        let current = self.snapshot(path);
        let changed = match self.seen.get(sid) {
            Some(prev) => prev.mtime != current.mtime || prev.size != current.size,
            None => true,
        };
        if changed {
            self.seen.insert(sid.to_string(), current);
        }
        changed
    }
}