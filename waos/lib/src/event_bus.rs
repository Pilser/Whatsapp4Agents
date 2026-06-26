use tokio::sync::broadcast;
use waos_core::events::WaosEvent;

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<WaosEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    pub fn publish(&self, event: WaosEvent) {
        let _ = self.tx.send(event);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WaosEvent> {
        self.tx.subscribe()
    }

    pub fn sender(&self) -> broadcast::Sender<WaosEvent> {
        self.tx.clone()
    }
}
