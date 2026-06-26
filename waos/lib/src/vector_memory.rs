use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::Mutex;
use turso::params::Params;
use turso::{Connection, Value};

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn dimensions(&self) -> usize;

    async fn embed(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>>;

    async fn embed_one(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let mut results = self.embed(&[text]).await?;
        results.pop().ok_or_else(|| anyhow::anyhow!("Empty embedding result"))
    }
}

pub struct OpenAiEmbedding {
    base_url: String,
    api_key: String,
    model: String,
    dims: usize,
}

impl OpenAiEmbedding {
    pub fn new(base_url: String, api_key: String, model: String, dims: usize) -> Self {
        Self { base_url, api_key, model, dims }
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiEmbedding {
    fn dimensions(&self) -> usize { self.dims }

    async fn embed(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
        });
        let url = format!("{}/v1/embeddings", self.base_url.trim_end_matches('/'));
        let resp = ureq::Agent::new()
            .post(&url)
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .send_string(&serde_json::to_string(&body)?)?;

        if resp.status() != 200 {
            anyhow::bail!("Embedding API error: {}", resp.status());
        }

        let json: serde_json::Value = serde_json::from_reader(resp.into_reader())?;
        let data = json["data"].as_array()
            .ok_or_else(|| anyhow::anyhow!("Missing 'data' in embedding response"))?;

        let mut embeddings = Vec::with_capacity(data.len());
        for item in data {
            let emb = item["embedding"].as_array()
                .ok_or_else(|| anyhow::anyhow!("Missing 'embedding' in item"))?;
            let vec: Vec<f32> = emb.iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();
            embeddings.push(vec);
        }
        Ok(embeddings)
    }
}

pub struct NoopEmbedding;

#[async_trait]
impl EmbeddingProvider for NoopEmbedding {
    fn dimensions(&self) -> usize { 0 }

    async fn embed(&self, _texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(vec![])
    }
}

pub fn create_embedding_provider(
    provider: &str,
    api_key: &str,
    model: &str,
    dims: usize,
) -> Box<dyn EmbeddingProvider> {
    match provider {
        "openai" => Box::new(OpenAiEmbedding::new(
            "https://api.openai.com".into(), api_key.into(), model.into(), dims,
        )),
        "openrouter" => Box::new(OpenAiEmbedding::new(
            "https://openrouter.ai/api/v1".into(), api_key.into(), model.into(), dims,
        )),
        p if p.starts_with("custom:") => {
            let base_url = p.strip_prefix("custom:").unwrap_or("").to_string();
            Box::new(OpenAiEmbedding::new(base_url, api_key.into(), model.into(), dims))
        }
        _ => Box::new(NoopEmbedding),
    }
}

#[derive(Clone)]
pub struct VectorMemory {
    conns: Arc<Mutex<HashMap<String, Connection>>>,
    provider: Arc<Mutex<Box<dyn EmbeddingProvider>>>,
}

impl VectorMemory {
    pub fn new(provider: Box<dyn EmbeddingProvider>) -> Self {
        Self {
            conns: Arc::new(Mutex::new(HashMap::new())),
            provider: Arc::new(Mutex::new(provider)),
        }
    }

    pub async fn register_account(&self, alias: &str, conn: Connection) {
        self.conns.lock().await.insert(alias.to_string(), conn);
    }

    pub async fn remove_account(&self, alias: &str) {
        self.conns.lock().await.remove(alias);
    }

    pub async fn index_message(&self, account: &str, message_id: &str, content: &str) -> anyhow::Result<()> {
        if content.trim().is_empty() {
            return Ok(());
        }
        let provider = self.provider.lock().await;
        if provider.dimensions() == 0 {
            return Ok(());
        }
        let embedding = provider.embed_one(content).await?;
        let blob: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();

        let conns = self.conns.lock().await;
        let conn = match conns.get(account) {
            Some(c) => c,
            None => return Ok(()),
        };
        let _ = conn.execute(
            "INSERT OR REPLACE INTO message_vectors (message_id, embedding, updated_at)
             VALUES (?1, ?2, ?3)",
            Params::Positional(vec![
                Value::from(message_id),
                Value::from(blob),
                Value::from(Utc::now().timestamp()),
            ]),
        ).await;
        Ok(())
    }

    pub async fn search_similar(&self, account: &str, query: &str, limit: usize) -> anyhow::Result<Vec<SearchResult>> {
        let provider = self.provider.lock().await;
        if provider.dimensions() == 0 {
            return Ok(vec![]);
        }
        let query_vec = provider.embed_one(query).await?;
        let query_blob: Vec<u8> = query_vec.iter().flat_map(|f| f.to_le_bytes()).collect();

        let conns = self.conns.lock().await;
        let conn = match conns.get(account) {
            Some(c) => c,
            None => return Ok(vec![]),
        };

        let mut rows = conn.query(
            "SELECT m.id, m.chat_id, m.sender_jid, m.sender_name, m.content, m.timestamp,
                    vector_distance_cos(mv.embedding, ?1) AS distance
             FROM message_vectors mv
             JOIN messages m ON m.id = mv.message_id
             ORDER BY distance ASC
             LIMIT ?2",
            Params::Positional(vec![
                Value::from(query_blob),
                Value::from(limit as i64),
            ]),
        ).await?;

        let mut results = Vec::new();
        loop {
            let row = match rows.next().await {
                Ok(Some(r)) => r,
                Ok(None) => break,
                Err(_) => break,
            };
            results.push(SearchResult {
                message_id: text_val(&row, 0),
                chat_id: text_val(&row, 1),
                sender_jid: text_val(&row, 2),
                sender_name: text_val(&row, 3),
                content: text_val(&row, 4),
                timestamp: int_val(&row, 5),
                score: real_val(&row, 6),
            });
        }
        Ok(results)
    }

    pub async fn find_contacts_for_query(&self, account: &str, query: &str, limit: usize) -> anyhow::Result<Vec<ContactMatch>> {
        let results = self.search_similar(account, query, limit * 5).await?;
        let mut contact_map: HashMap<String, (String, f64, u32, i64)> = HashMap::new();
        for r in results {
            let entry = contact_map.entry(r.sender_jid.clone()).or_insert((
                r.sender_name.clone(),
                0.0f64, 0u32, 0i64,
            ));
            entry.1 = entry.1.max(1.0 - r.score);
            entry.2 += 1;
            entry.3 = entry.3.max(r.timestamp);
        }
        let mut contacts: Vec<ContactMatch> = contact_map.into_iter()
            .map(|(jid, (name, score, count, recency))| ContactMatch { jid, name, score, match_count: count, last_activity: recency })
            .collect();
        contacts.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        contacts.truncate(limit);
        Ok(contacts)
    }
}

fn text_val(row: &turso::Row, idx: usize) -> String {
    match row.get_value(idx) { Ok(Value::Text(s)) => s, _ => String::new() }
}

fn int_val(row: &turso::Row, idx: usize) -> i64 {
    match row.get_value(idx) { Ok(Value::Integer(n)) => n, _ => 0 }
}

fn real_val(row: &turso::Row, idx: usize) -> f64 {
    match row.get_value(idx) { Ok(Value::Real(f)) => f, _ => 0.0 }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SearchResult {
    pub message_id: String,
    pub chat_id: String,
    pub sender_jid: String,
    pub sender_name: String,
    pub content: String,
    pub timestamp: i64,
    pub score: f64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContactMatch {
    pub jid: String,
    pub name: String,
    pub score: f64,
    pub match_count: u32,
    pub last_activity: i64,
}
