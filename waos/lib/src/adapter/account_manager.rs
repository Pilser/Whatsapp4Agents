use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use tokio::sync::Mutex;
use tracing::{error, info, warn};
use whatsapp_rust::bot::Bot;
use whatsapp_rust::client::Client;
use whatsapp_rust::store::{Device, DeviceStore};
use whatsapp_rust::TokioRuntime;
use whatsapp_rust_tokio_transport::TokioWebSocketTransportFactory;
use whatsapp_rust_ureq_http_client::UreqHttpClient;
use wacore::proto_helpers::MessageExt;
use wacore::store::DevicePropsOverride;
use wacore::types::events::Event;
use waproto::whatsapp::device_props::PlatformType;

use crate::adapter::storage::TursoStore;
use crate::bridge::mention_detector;
use crate::event_bus::EventBus;
use crate::sync_engine::SyncEngine;
use waos_core::events::WaosEvent;

/// Target hostname for network connectivity checks (WhatsApp Web endpoint).
const WHATSAPP_HOST: &str = "wss.web.whatsapp.com:443";
/// Interval between DNS probes during backoff sleep.
const NET_CHECK_INTERVAL: Duration = Duration::from_secs(5);

/// Returns true if DNS resolution succeeds for the WhatsApp Web hostname.
async fn is_network_available() -> bool {
    tokio::net::lookup_host(WHATSAPP_HOST).await.is_ok()
}

/// Sleeps up to `max_delay`, waking every `NET_CHECK_INTERVAL` to probe DNS.
/// Returns true if network was restored before the full delay elapsed.
/// Checks DNS immediately on entry so a healthy network returns instantly.
async fn wait_for_network(max_delay: Duration, alias: &str) -> bool {
    if is_network_available().await {
        return true;
    }
    let start = std::time::Instant::now();
    loop {
        let elapsed = start.elapsed();
        if elapsed >= max_delay {
            return false;
        }
        let chunk = NET_CHECK_INTERVAL.min(max_delay - elapsed);
        tokio::time::sleep(chunk).await;
        if is_network_available().await {
            info!("[{alias}] Network connectivity restored");
            return true;
        }
    }
}


pub type ClientRegistry = Arc<Mutex<HashMap<String, Arc<Client>>>>;

fn detect_message_type(msg: &waproto::whatsapp::Message) -> &'static str {
    let base = msg.get_base_message();
    if base.conversation.is_some() { "text" }
    else if base.extended_text_message.is_some() { "extended_text" }
    else if base.image_message.is_some() { "image" }
    else if base.video_message.is_some() { "video" }
    else if base.audio_message.is_some() { "audio" }
    else if base.document_message.is_some() { "document" }
    else if base.sticker_message.is_some() { "sticker" }
    else if base.location_message.is_some() { "location" }
    else if base.live_location_message.is_some() { "live_location" }
    else if base.contact_message.is_some() || base.contacts_array_message.is_some() { "contact" }
    else if base.reaction_message.is_some() { "reaction" }
    else if base.poll_creation_message.is_some() { "poll" }
    else if base.poll_update_message.is_some() { "poll_update" }
    else if base.protocol_message.is_some() { "protocol" }
    else if base.group_invite_message.is_some() { "group_invite" }
    else if base.list_message.is_some() { "list" }
    else if base.list_response_message.is_some() { "list_response" }
    else if base.order_message.is_some() { "order" }
    else if base.product_message.is_some() { "product" }
    else if base.template_message.is_some() { "template" }
    else if base.buttons_message.is_some() { "buttons" }
    else if base.buttons_response_message.is_some() { "buttons_response" }
    else if base.interactive_message.is_some() { "interactive" }
    else if base.interactive_response_message.is_some() { "interactive_response" }
    else if base.pin_in_chat_message.is_some() { "pin_in_chat" }
    else if base.keep_in_chat_message.is_some() { "keep_in_chat" }
    else if base.event_message.is_some() { "event" }
    else if base.call.is_some() { "call" }
    else if base.ptv_message.is_some() { "ptv" }
    else if base.poll_creation_message_v2.is_some() { "poll" }
    else if base.poll_creation_message_v3.is_some() { "poll" }
    else if base.poll_creation_message_v4.is_some() { "poll" }
    else if base.poll_creation_message_v5.is_some() { "poll" }
    else if base.poll_creation_message_v6.is_some() { "poll" }
    else { "unknown" }
}

pub struct AccountManager {
    handles: HashMap<String, AccountHandle>,
    pub clients: ClientRegistry,
    pub pending_clients: Arc<Mutex<HashMap<String, Arc<Client>>>>,
    pub qr_codes: Arc<Mutex<HashMap<String, String>>>,
    pub sync_engine: Option<SyncEngine>,
}

struct AccountHandle {
    alias: String,
    bot_handle: tokio::task::JoinHandle<()>,
    cancel: Arc<AtomicBool>,
}

impl AccountManager {
    pub fn new() -> Self {
        Self {
            handles: HashMap::new(),
            clients: Arc::new(Mutex::new(HashMap::new())),
            pending_clients: Arc::new(Mutex::new(HashMap::new())),
            qr_codes: Arc::new(Mutex::new(HashMap::new())),
            sync_engine: None,
        }
    }

    pub async fn add_account(
        &mut self,
        alias: &str,
        sessions_dir: &Path,
        event_bus: &EventBus,
    ) -> anyhow::Result<()> {
        self.add_account_with_phone(alias, sessions_dir, event_bus, None).await
    }

    pub async fn add_account_with_phone(
        &mut self,
        alias: &str,
        sessions_dir: &Path,
        event_bus: &EventBus,
        phone: Option<String>,
    ) -> anyhow::Result<()> {
        let session_dir = sessions_dir.join(alias);
        tokio::fs::create_dir_all(&session_dir).await?;
        let session_path = session_dir.join("session.db");

        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();
        let alias_owned = alias.to_string();
        let eb = event_bus.clone();
        let clients = self.clients.clone();
        let pending_clients = self.pending_clients.clone();
        let qr_codes = self.qr_codes.clone();
        let sync = self.sync_engine.clone();
        let phone_owned = phone.clone();

        let bot_handle = tokio::spawn(async move {
            Self::run_bot(&alias_owned, &session_path, eb, cancel_clone, clients, pending_clients, qr_codes, phone_owned, sync).await;
        });

        self.handles.insert(
            alias.to_string(),
            AccountHandle {
                alias: alias.to_string(),
                bot_handle,
                cancel,
            },
        );

        info!("Account '{alias}' bot spawned");
        Ok(())
    }

    #[allow(unused_variables)]
    async fn run_bot(
        alias: &str,
        session_path: &Path,
        event_bus: EventBus,
        cancel: Arc<AtomicBool>,
        clients: ClientRegistry,
        pending_clients: Arc<Mutex<HashMap<String, Arc<Client>>>>,
        qr_codes: Arc<Mutex<HashMap<String, String>>>,
        phone: Option<String>,
        sync_engine: Option<SyncEngine>,
    ) {
        const MAX_RETRIES: u32 = 10;
        let retry_count = Arc::new(AtomicU32::new(0));

        loop {
            if cancel.load(Ordering::Relaxed) {
                info!("[{alias}] Cancelled, stopping");
                return;
            }

            if !is_network_available().await {
                info!("[{alias}] Network unavailable, waiting before retry");
                wait_for_network(Duration::from_secs(30), alias).await;
            }

            let store = match TursoStore::new(session_path).await {
                Ok(s) => s,
                Err(e) => {
                    error!("[{alias}] Failed to create session store: {e}");
                    wait_for_network(Duration::from_secs(5), alias).await;
                    continue;
                }
            };
            let backend = Arc::new(store);

            let mut device = Device::new(backend.clone());
            let pair_reason = match backend.exists().await {
                Ok(true) => {
                    if let Ok(Some(core_device)) = backend.load().await {
                        let had_creds = core_device.pn.is_some();
                        device.load_from_serializable(core_device);
                        if had_creds {
                            info!("[{alias}] Loaded saved session — phone: {}", device.pn.as_ref().map(|j| j.to_string()).unwrap_or_default());
                            // logged in, no pairing needed
                            "none"
                        } else {
                            info!("[{alias}] Session DB exists but no credentials (pn missing) — will pair");
                            "pn_missing"
                        }
                    } else {
                        info!("[{alias}] Session DB empty or failed to decode — will pair");
                        "decode_failed"
                    }
                }
                Ok(false) => {
                    info!("[{alias}] No saved session — will pair on connect");
                    "no_db"
                }
                Err(e) => {
                    error!("[{alias}] Session check failed: {e}");
                    "check_failed"
                }
            };

            let transport_factory = TokioWebSocketTransportFactory::new();
            let http_client = UreqHttpClient::new();

            let (logout_tx, mut logout_rx) = tokio::sync::broadcast::channel::<()>(1);
            let session_revoked = Arc::new(AtomicBool::new(false));

            let alias_clone = alias.to_string();
            let eb = event_bus.clone();
            let session_revoked_clone = session_revoked.clone();
            let logout_tx_clone = logout_tx.clone();
            let retry_count_clone = retry_count.clone();
            let clients_clone = clients.clone();
            let qr_codes_clone = qr_codes.clone();
            let sync_clone = sync_engine.clone();

            let mut builder = Bot::builder()
                .with_backend(backend)
                .with_transport_factory(transport_factory)
                .with_http_client(http_client)
                .with_runtime(TokioRuntime)
                .with_device_props(
                    DevicePropsOverride::new()
                        .with_os("WAOS")
                        .with_platform_type(PlatformType::Desktop),
                );

            // Pair code is handled externally by the API handler — not via
            // the builder's internal task which has poor error visibility.
            // The external handler stores the client early via pending_clients
            // and calls Client::pair_with_code() directly with proper errors.

            builder = builder.on_event({
                    let alias = alias_clone.clone();
                    let eb = eb.clone();
                    let session_revoked = session_revoked_clone.clone();
                    let logout_tx = logout_tx_clone.clone();
                    let retry_count = retry_count_clone.clone();
                    let clients = clients_clone.clone();
                    let qr = qr_codes_clone.clone();
                    move |event, client| {
                        let a = alias.clone();
                        let e = eb.clone();
                        let sr = session_revoked.clone();
                        let lt = logout_tx.clone();
                        let _rc = retry_count.clone();
                        let cl = clients.clone();
                        let qr_clone = qr.clone();
                        let sync = sync_clone.clone();
                        async move {
                            match &*event {
                                Event::Connected(_) => {
                                    info!("[{a}] Connected to WhatsApp");
                                    cl.lock().await.insert(a.clone(), client.clone());
                                    e.publish(WaosEvent::Connected { account: a });
                                }
                                Event::Disconnected(_) => {
                                    warn!("[{a}] Disconnected");
                                    cl.lock().await.remove(&a);
                                    e.publish(WaosEvent::Disconnected { account: a });
                                }
                                Event::Message(msg, info) => {
                                    let content = msg.text_content().unwrap_or("").to_string();
                                    let sender_jid = info.source.sender.to_string();
                                    let sender_name = info.push_name.clone();
                                    let chat_id = info.source.chat.to_string();
                                    let timestamp = info.timestamp;

                                    let mentions = mention_detector::extract_mentions(&content);

                                    let message_type = detect_message_type(msg);
                                    let media_caption = msg.get_caption().unwrap_or("").to_string();

                                    let has_media = msg.image_message.is_some()
                                        || msg.video_message.is_some()
                                        || msg.audio_message.is_some()
                                        || msg.document_message.is_some()
                                        || msg.sticker_message.is_some()
                                        || msg.ptv_message.is_some();

                                    e.publish(WaosEvent::NewMessage {
                                        account: a,
                                        chat_id,
                                        message_id: info.id.to_string(),
                                        sender_jid,
                                        sender_name,
                                        content,
                                        mentions,
                                        has_media,
                                        is_group: info.source.is_group,
                                        is_from_me: info.source.is_from_me,
                                        message_type: message_type.to_string(),
                                        media_caption,
                                        timestamp: timestamp.timestamp(),
                                    });
                                }
                                Event::LoggedOut(_) => {
                                    sr.store(true, Ordering::Relaxed);
                                    cl.lock().await.remove(&a);
                                    warn!("[{a}] Session revoked");
                                    let _ = lt.send(());
                                }
                                Event::StreamError(err) => {
                                    error!("[{a}] Stream error: {err:?}");
                                }
                                Event::PairingCode { code, .. } => {
                                    e.publish(WaosEvent::PairingCode {
                                        account: a.clone(),
                                        code: code.clone(),
                                    });
                                    println!("\nPair code for account '{a}': {code}\n");
                                }
                                Event::PairingQrCode { code, .. } => {
                                    qr_clone.lock().await.insert(a.clone(), code.clone());
                                    e.publish(WaosEvent::QrCodeReady {
                                        account: a.clone(),
                                        code: code.clone(),
                                    });
                                }
                                Event::PushNameUpdate(update) => {
                                    e.publish(WaosEvent::ContactPushNameUpdated {
                                        account: a.clone(),
                                        jid: update.jid.to_string(),
                                        push_name: update.new_push_name.clone(),
                                    });
                                }
                                Event::ContactUpdated(updated) => {
                                    info!("[{a}] Contact profile updated: {}", updated.jid);
                                }
                                Event::ContactUpdate(update) => {
                                    e.publish(WaosEvent::ContactPushNameUpdated {
                                        account: a.clone(),
                                        jid: update.jid.to_string(),
                                        push_name: String::new(),
                                    });
                                }
                                Event::GroupUpdate(update) => {
                                    let group_jid = update.group_jid.to_string();
                                    use wacore::stanza::groups::GroupNotificationAction;
                                    match &update.action {
                                        GroupNotificationAction::Subject { subject, .. } => {
                                            e.publish(WaosEvent::GroupMetadataChanged {
                                                account: a.clone(),
                                                group_jid: group_jid.clone(),
                                                name: subject.clone(),
                                                description: String::new(),
                                            });
                                        }
                                        GroupNotificationAction::Description { description, .. } => {
                                            let desc = description.as_deref().unwrap_or("");
                                            e.publish(WaosEvent::GroupMetadataChanged {
                                                account: a.clone(),
                                                group_jid: group_jid.clone(),
                                                name: String::new(),
                                                description: desc.to_string(),
                                            });
                                        }
                                        GroupNotificationAction::Add { participants, .. } => {
                                            let p_jids: Vec<String> = participants.iter().map(|p| p.jid.to_string()).collect();
                                            e.publish(WaosEvent::GroupParticipantsChanged {
                                                account: a.clone(),
                                                group_jid: group_jid.clone(),
                                                action: "add".to_string(),
                                                participant_jids: p_jids,
                                            });
                                        }
                                        GroupNotificationAction::Remove { participants, .. } => {
                                            let p_jids: Vec<String> = participants.iter().map(|p| p.jid.to_string()).collect();
                                            e.publish(WaosEvent::GroupParticipantsChanged {
                                                account: a.clone(),
                                                group_jid: group_jid.clone(),
                                                action: "remove".to_string(),
                                                participant_jids: p_jids,
                                            });
                                        }
                                        GroupNotificationAction::Promote { participants } => {
                                            let p_jids: Vec<String> = participants.iter().map(|p| p.jid.to_string()).collect();
                                            e.publish(WaosEvent::GroupParticipantsChanged {
                                                account: a.clone(),
                                                group_jid: group_jid.clone(),
                                                action: "promote".to_string(),
                                                participant_jids: p_jids,
                                            });
                                        }
                                        GroupNotificationAction::Demote { participants } => {
                                            let p_jids: Vec<String> = participants.iter().map(|p| p.jid.to_string()).collect();
                                            e.publish(WaosEvent::GroupParticipantsChanged {
                                                account: a.clone(),
                                                group_jid: group_jid.clone(),
                                                action: "demote".to_string(),
                                                participant_jids: p_jids,
                                            });
                                        }
                                        _ => {}
                                    }
                                }
                                Event::PictureUpdate(pic) => {
                                    e.publish(WaosEvent::PictureChanged {
                                        account: a.clone(),
                                        jid: pic.jid.to_string(),
                                        removed: pic.removed,
                                    });
                                }
                                Event::ArchiveUpdate(archive) => {
                                    let archived = archive.action.archived.unwrap_or(false);
                                    e.publish(WaosEvent::ChatArchived {
                                        account: a.clone(),
                                        chat_id: archive.jid.to_string(),
                                        archived,
                                    });
                                }
                                Event::MarkChatAsReadUpdate(read) => {
                                    let read_bool = read.action.read.unwrap_or(true);
                                    if read_bool {
                                        e.publish(WaosEvent::ChatMarkedRead {
                                            account: a.clone(),
                                            chat_id: read.jid.to_string(),
                                        });
                                    }
                                }
                                Event::DeleteChatUpdate(del) => {
                                    e.publish(WaosEvent::ChatDeleted {
                                        account: a.clone(),
                                        chat_id: del.jid.to_string(),
                                    });
                                }
                                Event::MuteUpdate(mute) => {
                                    let muted_until = mute.action.mute_end_timestamp.unwrap_or(0);
                                    e.publish(WaosEvent::ChatMuted {
                                        account: a.clone(),
                                        chat_id: mute.jid.to_string(),
                                        muted_until,
                                    });
                                }
                                Event::PinUpdate(pin) => {
                                    if pin.action.pinned.unwrap_or(false) {
                                        e.publish(WaosEvent::ChatPinned {
                                            account: a.clone(),
                                            chat_id: pin.jid.to_string(),
                                        });
                                    }
                                }
                                // === Tier 1: Critical ===
                                Event::HistorySync(lazy_hs) => {
                                    if let Some(sync) = &sync {
                                        match sync.get_connection(&a).await {
                                            Some(conn) => {
                                                if let Some(hs) = lazy_hs.get() {
                                                    e.publish(WaosEvent::HistorySyncProgress {
                                                        account: a.clone(),
                                                        sync_type: hs.sync_type,
                                                        progress: hs.progress,
                                                        chunk_order: hs.chunk_order,
                                                    });
                                                    if let Err(e) = sync.process_history_sync(&conn, hs).await {
                                                        error!("[{a}] HistorySync processing failed: {e}");
                                                    } else {
                                                        info!("[{a}] HistorySync processed successfully");
                                                    }
                                                }
                                            }
                                            None => error!("[{a}] No DB connection for HistorySync"),
                                        }
                                    }
                                }
                                Event::Receipt(receipt) => {
                                    e.publish(WaosEvent::MessageReceipt {
                                        account: a.clone(),
                                        chat_id: receipt.source.chat.to_string(),
                                        message_ids: receipt.message_ids.iter().map(|id| id.to_string()).collect(),
                                        receipt_type: format!("{:?}", receipt.r#type).to_lowercase(),
                                        timestamp: receipt.timestamp.timestamp(),
                                    });
                                }
                                Event::TemporaryBan(ban) => {
                                    let secs = ban.expire.num_seconds().max(0) as u64;
                                    warn!("[{a}] Temporary ban: {:?}, expires in {}s", ban.code, secs);
                                    e.publish(WaosEvent::AccountTempBan {
                                        account: a.clone(),
                                        reason: format!("{:?}", ban.code),
                                        expire_secs: secs,
                                    });
                                }
                                // === Tier 2: High ===
                                Event::ChatPresence(presence) => {
                                    e.publish(WaosEvent::ChatPresenceUpdate {
                                        account: a.clone(),
                                        chat_id: presence.source.chat.to_string(),
                                        sender_jid: presence.source.sender.to_string(),
                                        state: format!("{:?}", presence.state).to_lowercase(),
                                    });
                                }
                                Event::Presence(presence) => {
                                    e.publish(WaosEvent::PresenceUpdate {
                                        account: a.clone(),
                                        jid: presence.from.to_string(),
                                        available: !presence.unavailable,
                                        last_seen: presence.last_seen.map(|dt| dt.timestamp()),
                                    });
                                }
                                Event::UndecryptableMessage(undec) => {
                                    let source = &undec.info.source;
                                    e.publish(WaosEvent::UndecryptableMessage {
                                        account: a.clone(),
                                        chat_id: source.chat.to_string(),
                                        message_id: undec.info.id.to_string(),
                                        sender_jid: source.sender.to_string(),
                                        timestamp: undec.info.timestamp.timestamp(),
                                    });
                                }
                                Event::ContactNumberChanged(changed) => {
                                    info!("[{a}] Contact number changed: {} -> {}", changed.old_jid, changed.new_jid);
                                    e.publish(WaosEvent::ContactPhoneChanged {
                                        account: a.clone(),
                                        old_jid: changed.old_jid.to_string(),
                                        new_jid: changed.new_jid.to_string(),
                                    });
                                }
                                // === Tier 3: Medium ===
                                Event::PairSuccess(success) => {
                                    info!("[{a}] Pair success: {} (lid: {})", success.id, success.lid);
                                    e.publish(WaosEvent::AccountStatusUpdate {
                                        account: a.clone(),
                                        status: "paired".to_string(),
                                    });
                                }
                                Event::PairError(err) => {
                                    error!("[{a}] Pair error: {} (lid: {}) err: {}", err.id, err.lid, err.error);
                                    e.publish(WaosEvent::AccountStatusUpdate {
                                        account: a.clone(),
                                        status: format!("pair_error:{}", err.error),
                                    });
                                }
                                Event::ConnectFailure(fail) => {
                                    error!("[{a}] Connect failure: {:?} - {}", fail.reason, fail.message);
                                    e.publish(WaosEvent::AccountStatusUpdate {
                                        account: a.clone(),
                                        status: format!("connect_failure:{:?}", fail.reason),
                                    });
                                }
                                Event::SelfPushNameUpdated(name) => {
                                    info!("[{a}] Self push name updated: '{}' -> '{}'", name.old_name, name.new_name);
                                }
                                Event::DeleteMessageForMeUpdate(del) => {
                                    e.publish(WaosEvent::MessageDeleted {
                                        account: a.clone(),
                                        chat_id: del.chat_jid.to_string(),
                                        message_id: del.message_id.clone(),
                                    });
                                }
                                Event::ContactSyncRequested(_) => {
                                    info!("[{a}] Contact sync requested by server");
                                }
                                Event::StreamReplaced(_) => {
                                    info!("[{a}] Stream replaced");
                                }
                                Event::OfflineSyncPreview(preview) => {
                                    info!("[{a}] Offline sync preview: {} messages, {} receipts", preview.messages, preview.receipts);
                                }
                                Event::OfflineSyncCompleted(completed) => {
                                    info!("[{a}] Offline sync completed: {} items", completed.count);
                                }
                                Event::StarUpdate(star) => {
                                    info!("[{a}] Message starred: {} (chat: {})", star.message_id, star.chat_jid);
                                }
                                Event::QrScannedWithoutMultidevice(_) => {
                                    warn!("[{a}] QR scanned but phone does not support multi-device");
                                }
                                Event::ClientOutdated(_) => {
                                    error!("[{a}] Client version outdated, please update WAOS");
                                }
                                Event::DisappearingModeChanged(_) |
                                Event::BusinessStatusUpdate(_) |
                                Event::DeviceListUpdate(_) |
                                Event::IdentityChange(_) |
                                Event::UserAboutUpdate(_) |
                                Event::IncomingCall(_) |
                                Event::NewsletterLiveUpdate(_) => {
                                    // Logged at trace level — low-value events
                                }
                                _ => {}
                            }
                        }
                    }
                });

            let mut bot = match builder.build().await {
                Ok(b) => b,
                Err(e) => {
                    error!("[{alias}] Build bot failed: {e}");
                    wait_for_network(Duration::from_secs(10), alias).await;
                    continue;
                }
            };

            // Store client immediately so API handlers can call pair_with_code() directly
            pending_clients.lock().await.insert(alias.to_string(), bot.client());

            if device.pn.is_some() {
                info!("[{alias}] Starting bot with saved session");
            } else {
                match pair_reason {
                    "no_db" => info!("[{alias}] No session DB — will generate QR code for pairing"),
                    "pn_missing" => info!("[{alias}] Session file has no phone credentials — will generate QR code for pairing"),
                    "decode_failed" => info!("[{alias}] Failed to decode session — will generate QR code for pairing"),
                    "check_failed" => info!("[{alias}] Session check errored — will generate QR code for pairing"),
                    _ => info!("[{alias}] No valid credentials — will generate QR code for pairing"),
                }
            }

            // bot.run() spawns the client in a background task and returns a
            // BotHandle that fires when the client's run loop completes.
            let mut bot_handle = match bot.run().await {
                Ok(h) => h,
                Err(e) => {
                    pending_clients.lock().await.remove(alias);
                    error!("[{alias}] Run bot failed: {e}");
                    wait_for_network(Duration::from_secs(10), alias).await;
                    continue;
                }
            };

            // DNS monitor: aborts the bot if network stays down for ~30s,
            // so we don't sit idle through the inner loop's fibonacci backoff.
            let dns_alert = {
                let a = alias.to_string();
                let c = cancel.clone();
                async move {
                    let mut failures = 0u32;
                    loop {
                        tokio::time::sleep(Duration::from_secs(10)).await;
                        if c.load(Ordering::Relaxed) { return; }
                        if !is_network_available().await {
                            failures += 1;
                            if failures >= 3 {
                                warn!("[{a}] Network unreachable for ~30s");
                                return;
                            }
                        } else {
                            failures = 0;
                        }
                    }
                }
            };
            tokio::pin!(dns_alert);

            let mut network_forced = false;
            let mut session_revoked_val = false;

            tokio::select! {
                biased;
                _ = &mut bot_handle => {
                    info!("[{alias}] Client run loop exited");
                    clients.lock().await.remove(alias);
                }
                _ = logout_rx.recv() => {
                    info!("[{alias}] Session logged out / revoked");
                    bot_handle.abort();
                    clients.lock().await.remove(alias);
                    let _ = &mut bot_handle.await;
                    session_revoked_val = session_revoked.load(Ordering::Relaxed);
                }
                _ = &mut dns_alert => {
                    warn!("[{alias}] Aborting stuck bot due to network loss");
                    bot_handle.abort();
                    clients.lock().await.remove(alias);
                    pending_clients.lock().await.remove(alias);
                    let _ = &mut bot_handle.await;
                    network_forced = true;
                }
                _ = tokio::signal::ctrl_c() => {
                    info!("[{alias}] Ctrl+C received");
                    bot_handle.abort();
                    clients.lock().await.remove(alias);
                    let _ = &mut bot_handle.await;
                    drop(bot);
                    break;
                }
            }

            drop(bot);
            drop(device);

            if session_revoked_val {
                if let Some(parent) = session_path.parent() {
                    let _ = std::fs::remove_dir_all(parent);
                }
            }

            if network_forced {
                info!("[{alias}] Waiting for network recovery before retry");
                wait_for_network(Duration::from_secs(300), alias).await;
            }

            let attempts = retry_count.fetch_add(1, Ordering::SeqCst) + 1;
            if attempts >= MAX_RETRIES {
                error!("[{alias}] Max retries ({MAX_RETRIES}) exceeded, giving up");
                return;
            }

            let delay = std::cmp::min(3u64.pow(attempts), 300);
            info!("[{alias}] Reconnecting in {delay}s (attempt {attempts}/{MAX_RETRIES})");
            wait_for_network(Duration::from_secs(delay), alias).await;
        }
    }

    pub async fn remove_account(&mut self, alias: &str) {
        self.pending_clients.lock().await.remove(alias);
        if let Some(handle) = self.handles.remove(alias) {
            handle.cancel.store(true, Ordering::Relaxed);
            info!("Account '{alias}' removed");
        }
    }

    pub async fn shutdown_all(&mut self) {
        let handles: Vec<String> = self.handles.keys().cloned().collect();
        for alias in &handles {
            if let Some(handle) = self.handles.remove(alias) {
                handle.cancel.store(true, Ordering::Relaxed);
                let _ = handle.bot_handle.await;
                info!("Account '{alias}' shut down");
            }
        }
    }

    pub fn list_accounts(&self) -> Vec<String> {
        self.handles.keys().cloned().collect()
    }
}
