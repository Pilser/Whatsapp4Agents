use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::Mutex;
use turso::params::Params;
use turso::{Connection, Value};

#[derive(Clone)]
pub struct RelationshipGraph {
    conns: Arc<Mutex<HashMap<String, Connection>>>,
    cross_account_conns: Arc<Mutex<HashMap<String, (Connection, String)>>>,
}

impl RelationshipGraph {
    pub fn new() -> Self {
        Self {
            conns: Arc::new(Mutex::new(HashMap::new())),
            cross_account_conns: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn register_account(&self, alias: &str, conn: Connection) {
        self.conns.lock().await.insert(alias.to_string(), conn);
    }

    pub async fn remove_account(&self, alias: &str) {
        self.conns.lock().await.remove(alias);
    }

    pub fn register_cross_account_connection(&self, alias: &str, conn: Connection, phone_prefix: String) {
        let mut map = self.cross_account_conns.blocking_lock();
        map.insert(alias.to_string(), (conn, phone_prefix));
    }

    pub async fn record_interaction(&self, account: &str, from: &str, to: &str, edge_type: &str) {
        let conns = self.conns.lock().await;
        let conn = match conns.get(account) {
            Some(c) => c,
            None => return,
        };
        let now = Utc::now().timestamp();

        let _ = conn.execute(
            "INSERT INTO graph_nodes (jid, node_type, name, created_at)
             VALUES (?1, 'contact', '', ?2)
             ON CONFLICT(jid) DO NOTHING",
            Params::Positional(vec![Value::from(from), Value::from(now)]),
        ).await;
        let _ = conn.execute(
            "INSERT INTO graph_nodes (jid, node_type, name, created_at)
             VALUES (?1, 'contact', '', ?2)
             ON CONFLICT(jid) DO NOTHING",
            Params::Positional(vec![Value::from(to), Value::from(now)]),
        ).await;

        let _ = conn.execute(
            "INSERT INTO graph_edges (from_jid, to_jid, edge_type, strength, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1.0, ?4, ?4)
             ON CONFLICT(from_jid, to_jid, edge_type) DO UPDATE SET
               strength = MIN(5.0, graph_edges.strength + 0.5),
               updated_at = ?4",
            Params::Positional(vec![
                Value::from(from),
                Value::from(to),
                Value::from(edge_type),
                Value::from(now),
            ]),
        ).await;
    }

    pub async fn record_topic(&self, account: &str, contact_jid: &str, topic: &str) {
        let conns = self.conns.lock().await;
        let conn = match conns.get(account) {
            Some(c) => c,
            None => return,
        };
        let now = Utc::now().timestamp();
        let topic_node = format!("topic:{}", topic.to_lowercase().replace(' ', "_"));

        let _ = conn.execute(
            "INSERT OR IGNORE INTO graph_nodes (jid, node_type, name, created_at)
             VALUES (?1, 'topic', ?2, ?3)",
            Params::Positional(vec![
                Value::from(topic_node.as_str()),
                Value::from(topic),
                Value::from(now),
            ]),
        ).await;

        let _ = conn.execute(
            "INSERT INTO graph_edges (from_jid, to_jid, edge_type, strength, created_at, updated_at)
             VALUES (?1, ?2, 'talked_about', 1.0, ?3, ?3)
             ON CONFLICT(from_jid, to_jid, edge_type) DO UPDATE SET
               strength = MIN(5.0, graph_edges.strength + 0.5),
               updated_at = ?3",
            Params::Positional(vec![
                Value::from(contact_jid),
                Value::from(topic_node.as_str()),
                Value::from(now),
            ]),
        ).await;
    }

    pub async fn shared_groups(&self, account: &str, jid1: &str, jid2: &str) -> Vec<String> {
        let conns = self.conns.lock().await;
        let conn = match conns.get(account) {
            Some(c) => c,
            None => return vec![],
        };
        let mut rows = match conn.query(
            "SELECT DISTINCT e1.to_jid
             FROM graph_edges e1
             JOIN graph_edges e2 ON e1.to_jid = e2.to_jid AND e1.edge_type = e2.edge_type
             WHERE e1.from_jid = ?1 AND e2.from_jid = ?2 AND e1.edge_type = 'member_of'",
            Params::Positional(vec![Value::from(jid1), Value::from(jid2)]),
        ).await {
            Ok(r) => r,
            Err(_) => return vec![],
        };
        let mut results = Vec::new();
        loop {
            let row = match rows.next().await {
                Ok(Some(r)) => r,
                Ok(None) => break,
                Err(_) => break,
            };
            if let Ok(Value::Text(jid)) = row.get_value(0) {
                results.push(jid);
            }
        }
        results
    }

    pub async fn introductions(&self, account: &str, jid: &str, max_depth: usize) -> Vec<IntroducedContact> {
        let conns = self.conns.lock().await;
        let conn = match conns.get(account) {
            Some(c) => c,
            None => return vec![],
        };

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut results = Vec::new();

        visited.insert(jid.to_string());
        queue.push_back((jid.to_string(), 0usize));

        while let Some((current_jid, depth)) = queue.pop_front() {
            if depth > 0 {
                let name = self.get_contact_name(conn, &current_jid).await;
                results.push(IntroducedContact { jid: current_jid.clone(), name, depth: depth as u32 });
            }

            if depth >= max_depth {
                continue;
            }

            let mut rows = match conn.query(
                "SELECT e.to_jid FROM graph_edges e
                 JOIN graph_nodes n ON n.jid = e.to_jid
                 WHERE e.from_jid = ?1 AND n.node_type = 'contact'
                 UNION
                 SELECT e.from_jid FROM graph_edges e
                 JOIN graph_nodes n ON n.jid = e.from_jid
                 WHERE e.to_jid = ?1 AND n.node_type = 'contact'",
                Params::Positional(vec![Value::from(current_jid.as_str())]),
            ).await {
                Ok(r) => r,
                Err(_) => continue,
            };

            loop {
                let row = match rows.next().await {
                    Ok(Some(r)) => r,
                    Ok(None) => break,
                    Err(_) => break,
                };
                let neighbor = match row.get_value(0) { Ok(Value::Text(s)) => s, _ => continue };
                if !visited.contains(&neighbor) {
                    visited.insert(neighbor.clone());
                    queue.push_back((neighbor, depth + 1));
                }
            }
        }
        results
    }

    pub async fn contacts_by_topic(&self, account: &str, topic: &str) -> Vec<String> {
        let conns = self.conns.lock().await;
        let conn = match conns.get(account) {
            Some(c) => c,
            None => return vec![],
        };
        let topic_node = format!("topic:{}", topic.to_lowercase().replace(' ', "_"));
        let mut rows = match conn.query(
            "SELECT from_jid FROM graph_edges WHERE to_jid = ?1 AND edge_type = 'talked_about'",
            Params::Positional(vec![Value::from(topic_node.as_str())]),
        ).await {
            Ok(r) => r,
            Err(_) => return vec![],
        };
        let mut results = Vec::new();
        loop {
            let row = match rows.next().await {
                Ok(Some(r)) => r,
                Ok(None) => break,
                Err(_) => break,
            };
            if let Ok(Value::Text(jid)) = row.get_value(0) {
                results.push(jid);
            }
        }
        results
    }

    pub async fn important_contacts(&self, account: &str, community_id: &str) -> Vec<ScoredContact> {
        let conns = self.conns.lock().await;
        let conn = match conns.get(account) {
            Some(c) => c,
            None => return vec![],
        };
        let mut rows = match conn.query(
            "SELECT c.jid, c.push_name, cs.importance, cs.frequency, cs.reply_rate,
                    (SELECT COUNT(*) FROM graph_edges e WHERE e.from_jid = c.jid) AS centrality
             FROM contacts c
             LEFT JOIN contact_scores cs ON cs.jid = c.jid
             WHERE c.jid IN (
                 SELECT e.from_jid FROM graph_edges e WHERE e.to_jid = ?1 AND e.edge_type = 'member_of'
             )
             ORDER BY cs.importance DESC LIMIT 50",
            Params::Positional(vec![Value::from(community_id)]),
        ).await {
            Ok(r) => r,
            Err(_) => return vec![],
        };
        let mut results = Vec::new();
        loop {
            let row = match rows.next().await {
                Ok(Some(r)) => r,
                Ok(None) => break,
                Err(_) => break,
            };
            results.push(ScoredContact {
                jid: get_text(&row, 0),
                name: get_text(&row, 1),
                importance: get_real(&row, 2),
                frequency: get_int(&row, 3) as u32,
                reply_rate: get_real(&row, 4),
                centrality: get_int(&row, 5) as u32,
            });
        }
        results
    }

    pub async fn merge_contacts_across_accounts(&self) -> Vec<MergedContact> {
        let map = self.cross_account_conns.lock().await;
        let mut phone_map: HashMap<String, MergedContactBuilder> = HashMap::new();

        for (alias, (conn, prefix)) in map.iter() {
            let mut rows = match conn.query(
                "SELECT jid, push_name, phone FROM contacts WHERE phone IS NOT NULL AND phone != ''",
                (),
            ).await {
                Ok(r) => r,
                Err(_) => continue,
            };
            loop {
                let row = match rows.next().await {
                    Ok(Some(r)) => r,
                    Ok(None) => break,
                    Err(_) => break,
                };
                let jid = get_text(&row, 0);
                let name = get_text(&row, 1);
                let phone = get_text(&row, 2);
                let normalized = normalize_phone(&phone, prefix);

                let entry = phone_map.entry(normalized).or_insert(MergedContactBuilder {
                    canonical_phone: phone.clone(),
                    names: Vec::new(),
                    accounts: Vec::new(),
                    total_frequency: 0,
                    total_importance: 0.0,
                });
                entry.names.push(name);
                entry.accounts.push(alias.clone());

                if let Ok(mut freq_rows) = conn.query(
                    "SELECT frequency FROM contact_scores WHERE jid = ?1",
                    Params::Positional(vec![Value::from(jid.as_str())]),
                ).await {
                    if let Ok(Some(freq_row)) = freq_rows.next().await {
                        if let Value::Integer(n) = freq_row.get_value(0).unwrap_or(Value::Integer(0)) {
                            entry.total_frequency += n as u64;
                        }
                    }
                }
                if let Ok(mut imp_rows) = conn.query(
                    "SELECT importance FROM contact_scores WHERE jid = ?1",
                    Params::Positional(vec![Value::from(jid.as_str())]),
                ).await {
                    if let Ok(Some(imp_row)) = imp_rows.next().await {
                        if let Value::Real(f) = imp_row.get_value(0).unwrap_or(Value::Real(0.0)) {
                            entry.total_importance += f;
                        }
                    }
                }
            }
        }

        phone_map.into_iter().map(|(phone, b)| MergedContact {
            canonical_phone: phone,
            best_name: b.names.into_iter().max_by(|a, b| a.len().cmp(&b.len())).unwrap_or_default(),
            accounts: b.accounts,
            total_frequency: b.total_frequency,
            total_importance: b.total_importance,
        }).collect()
    }

    async fn get_contact_name(&self, conn: &Connection, jid: &str) -> String {
        let mut rows = conn.query(
            "SELECT COALESCE(push_name, name, phone, 'Unknown') FROM contacts WHERE jid = ?1",
            Params::Positional(vec![Value::from(jid)]),
        ).await.unwrap();
        match rows.next().await {
            Ok(Some(row)) => get_text(&row, 0),
            _ => "Unknown".into(),
        }
    }
}

fn normalize_phone(phone: &str, prefix: &str) -> String {
    let stripped: String = phone.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect();
    if !stripped.starts_with('+') && !prefix.is_empty() {
        format!("{}{}", prefix, stripped)
    } else {
        stripped
    }
}

fn get_text(row: &turso::Row, idx: usize) -> String {
    match row.get_value(idx) { Ok(Value::Text(s)) => s, _ => String::new() }
}

fn get_real(row: &turso::Row, idx: usize) -> f64 {
    match row.get_value(idx) { Ok(Value::Real(f)) => f, _ => 0.0 }
}

fn get_int(row: &turso::Row, idx: usize) -> i64 {
    match row.get_value(idx) { Ok(Value::Integer(n)) => n, _ => 0 }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct IntroducedContact {
    pub jid: String,
    pub name: String,
    pub depth: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScoredContact {
    pub jid: String,
    pub name: String,
    pub importance: f64,
    pub frequency: u32,
    pub reply_rate: f64,
    pub centrality: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MergedContact {
    pub canonical_phone: String,
    pub best_name: String,
    pub accounts: Vec<String>,
    pub total_frequency: u64,
    pub total_importance: f64,
}

struct MergedContactBuilder {
    canonical_phone: String,
    names: Vec<String>,
    accounts: Vec<String>,
    total_frequency: u64,
    total_importance: f64,
}
