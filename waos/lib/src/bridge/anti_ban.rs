use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use rand::Rng;
use tokio::sync::Mutex;
use turso::Connection;
use turso::params::Params;
use turso::Value;

pub struct AntiBan {
    main_conn: Connection,
    hourly_cap: u32,
    daily_cap: u32,
    pacer: SendPacer,
    cooldown_until: Arc<Mutex<Option<i64>>>,
}

pub struct SendPacer {
    min_delay_ms: u64,
    jitter_ms: u64,
    last_send: Arc<Mutex<Instant>>,
}

impl SendPacer {
    pub fn new(min_delay_ms: u64, jitter_ms: u64) -> Self {
        Self {
            min_delay_ms,
            jitter_ms,
            last_send: Arc::new(Mutex::new(Instant::now())),
        }
    }

    pub async fn wait(&self) {
        let elapsed = self.last_send.lock().await.elapsed();
        let delay = self.min_delay_ms + rand::thread_rng().gen_range(0..self.jitter_ms);
        if elapsed < std::time::Duration::from_millis(delay) {
            tokio::time::sleep(std::time::Duration::from_millis(delay) - elapsed).await;
        }
        *self.last_send.lock().await = Instant::now();
    }
}

impl AntiBan {
    pub fn new(main_conn: Connection) -> Self {
        Self {
            main_conn,
            hourly_cap: 80,
            daily_cap: 800,
            pacer: SendPacer::new(2000, 1000),
            cooldown_until: Arc::new(Mutex::new(None)),
        }
    }

    pub fn pacer(&self) -> &SendPacer {
        &self.pacer
    }

    pub async fn check_can_send(&self, account: &str) -> Result<(), String> {
        let cooldown = *self.cooldown_until.lock().await;
        if let Some(until) = cooldown {
            if Utc::now().timestamp() < until {
                return Err("Emergency cooldown active".into());
            }
        }

        let now = Utc::now().timestamp();
        let hour_window = now - (now % 3600);
        let day_window = now - (now % 86400);

        let hourly = self.get_count(account, hour_window).await;
        let daily = self.get_count(account, day_window).await;

        if hourly >= self.hourly_cap {
            return Err(format!("Hourly send cap reached ({}/{})", hourly, self.hourly_cap));
        }
        if daily >= self.daily_cap {
            return Err(format!("Daily send cap reached ({}/{})", daily, self.daily_cap));
        }
        Ok(())
    }

    pub async fn record_send(&self, account: &str) {
        let now = Utc::now().timestamp();
        let hour_window = now - (now % 3600);
        let day_window = now - (now % 86400);

        self.increment(account, hour_window).await;
        self.increment(account, day_window).await;
    }

    pub async fn trigger_cooldown(&self, duration_secs: i64) {
        let until = Utc::now().timestamp() + duration_secs;
        *self.cooldown_until.lock().await = Some(until);
    }

    async fn get_count(&self, account: &str, window_start: i64) -> u32 {
        let mut rows = self.main_conn.query(
            "SELECT sends FROM rate_counters WHERE account = ?1 AND window_start = ?2",
            Params::Positional(vec![Value::from(account), Value::from(window_start)]),
        ).await.unwrap();
        match rows.next().await {
            Ok(Some(row)) => match row.get_value(0) {
                Ok(Value::Integer(n)) => n as u32,
                _ => 0,
            },
            _ => 0,
        }
    }

    async fn increment(&self, account: &str, window_start: i64) {
        self.main_conn.execute(
            "INSERT INTO rate_counters (account, window_start, sends) VALUES (?1, ?2, 1)
             ON CONFLICT(account, window_start) DO UPDATE SET sends = sends + 1",
            Params::Positional(vec![Value::from(account), Value::from(window_start)]),
        ).await.ok();
    }

    pub async fn remaining(&self, account: &str) -> (u32, u32) {
        let now = Utc::now().timestamp();
        let hour_window = now - (now % 3600);
        let day_window = now - (now % 86400);
        let hourly = self.get_count(account, hour_window).await;
        let daily = self.get_count(account, day_window).await;
        (self.hourly_cap.saturating_sub(hourly), self.daily_cap.saturating_sub(daily))
    }
}
