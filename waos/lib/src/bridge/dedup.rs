use std::collections::HashSet;
use std::sync::Mutex;

pub struct Dedup {
    seen: Mutex<HashSet<String>>,
    max_size: usize,
}

impl Dedup {
    pub fn new(max_size: usize) -> Self {
        Self {
            seen: Mutex::new(HashSet::with_capacity(max_size)),
            max_size,
        }
    }

    pub fn is_new(&self, message_id: &str) -> bool {
        let mut seen = self.seen.lock().unwrap();
        if seen.contains(message_id) {
            return false;
        }
        seen.insert(message_id.to_string());
        if seen.len() > self.max_size {
            seen.clear();
        }
        true
    }
}
