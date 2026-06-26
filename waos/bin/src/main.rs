use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

mod api;
mod cmds;
mod daemon;
mod ipc;
mod server;

#[derive(Parser)]
#[command(name = "waos", version, about = "WhatsApp Agent Operating System")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the WAOS daemon (bots, API server, resource monitoring).
    /// This is the default command — `waos` is equivalent to `waos daemon`.
    Daemon {
        /// Management API port
        #[arg(long, default_value = "8080")]
        port: u16,
    },
    Status {
        #[command(subcommand)]
        action: Option<cmds::status::StatusCommand>,
    },
    Pair { alias: String },
    Dashboard {
        #[command(subcommand)]
        action: DashboardAction,
    },
    Chat {
        #[arg(long)]
        account: String,
        #[command(subcommand)]
        action: ChatAction,
    },
    Message {
        #[arg(long)]
        account: String,
        #[command(subcommand)]
        action: cmds::message::MessageCommand,
    },
    Media {
        #[arg(long)]
        account: String,
        #[command(subcommand)]
        action: cmds::media::MediaCommand,
    },
    Contact {
        #[arg(long)]
        account: String,
        #[command(subcommand)]
        action: cmds::contact::ContactCommand,
    },
    Memory {
        #[arg(long)]
        account: String,
        #[command(subcommand)]
        action: cmds::memory::MemoryCommand,
    },
    Graph {
        #[arg(long)]
        account: String,
        #[command(subcommand)]
        action: cmds::graph::GraphCommand,
    },
    Community {
        #[arg(long)]
        account: String,
        #[command(subcommand)]
        action: cmds::community::CommunityCommand,
    },
    Task {
        #[command(subcommand)]
        action: cmds::task::TaskCommand,
    },
}

#[derive(Subcommand)]
enum ChatAction {
    List,
    Unread,
    Search { query: String },
    MarkRead { chat_id: String },
    History {
        chat_id: String,
        #[arg(long, default_value = "5")]
        limit: usize,
        /// Get messages before this timestamp (for older pages)
        #[arg(long)]
        before: Option<i64>,
        /// Get messages after this timestamp (for newer pages)
        #[arg(long)]
        after: Option<i64>,
        /// Skip N messages, then take `limit` (index-based pagination)
        #[arg(long, default_value = "0")]
        offset: usize,
        /// Show total message count for the chat
        #[arg(long)]
        total: bool,
    },
    /// Stream real-time events for a chat (queue updates, receipts, messages)
    Listen {
        chat_id: String,
        /// Timeout in seconds before the stream auto-closes
        #[arg(long, default_value = "120")]
        timeout: u64,
    },
}

#[derive(Subcommand)]
enum DashboardAction {
    Show,
    Watch,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let cli = Cli::parse();

    use ipc::IpcRequest;

    match cli.command.unwrap_or(Command::Daemon { port: 8080 }) {
        Command::Daemon { port } => daemon::run(port).await,
        Command::Status { action } => {
            use cmds::status::StatusCommand;
            let Some(action) = action else {
                if ipc::try_exec(&IpcRequest::new("status")).await.is_ok() { return Ok(()); }
                return cmds::status::run().await;
            };
            let account = match &action {
                StatusCommand::List => "",
                StatusCommand::SendText { account, .. } => account.as_str(),
                StatusCommand::SendImage { account, .. } => account.as_str(),
                StatusCommand::SendVideo { account, .. } => account.as_str(),
                StatusCommand::Revoke { account, .. } => account.as_str(),
                StatusCommand::History { account, .. } => account.as_str(),
                StatusCommand::Delete { account, .. } => account.as_str(),
            };
            let req = match &action {
                StatusCommand::List => IpcRequest::new("status"),
                StatusCommand::SendText { text, bg_color, font, wait, .. } => {
                    let meta = serde_json::json!({"bg": bg_color, "font": font});
                    let mut r = IpcRequest::new("status_send_text").with_account(account).with_text(text).with_metadata(&meta.to_string());
                    if let Some(w) = wait { r = r.with_wait(true).with_timeout(*w); }
                    r
                }
                StatusCommand::SendImage { file_path, thumbnail_file, caption, wait, .. } => {
                    let meta = serde_json::json!({"thumbnail": thumbnail_file});
                    let mut r = IpcRequest::new("status_send_image").with_account(account).with_file_path(file_path).with_metadata(&meta.to_string());
                    if let Some(c) = caption { r = r.with_text(c); }
                    if let Some(w) = wait { r = r.with_wait(true).with_timeout(*w); }
                    r
                }
                StatusCommand::SendVideo { file_path, thumbnail_file, caption, wait, .. } => {
                    let meta = serde_json::json!({"thumbnail": thumbnail_file});
                    let mut r = IpcRequest::new("status_send_video").with_account(account).with_file_path(file_path).with_metadata(&meta.to_string());
                    if let Some(c) = caption { r = r.with_text(c); }
                    if let Some(w) = wait { r = r.with_wait(true).with_timeout(*w); }
                    r
                }
                StatusCommand::Revoke { message_id, wait, .. } => {
                    let mut r = IpcRequest::new("status_revoke").with_account(account).with_message_id(message_id);
                    if let Some(w) = wait { r = r.with_wait(true).with_timeout(*w); }
                    r
                }
                StatusCommand::History { status: status_filter, before, after, limit, offset, .. } => {
                    let mut r = IpcRequest::new("status_history")
                        .with_account(account)
                        .with_query(status_filter)
                        .with_text(&limit.to_string())
                        .with_reply_to(&offset.to_string());
                    if let Some(ts) = before { r = r.with_before(*ts); }
                    if let Some(ts) = after { r = r.with_after(*ts); }
                    r
                }
                StatusCommand::Delete { older_than, newer_than, status: status_filter, yes, .. } => {
                    let mut r = IpcRequest::new("status_delete")
                        .with_account(account)
                        .with_query(status_filter);
                    if let Some(ts) = older_than { r = r.with_before(*ts); }
                    if let Some(ts) = newer_than { r = r.with_after(*ts); }
                    if *yes { r = r.with_text("yes"); }
                    r
                }
            };
            if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
            cmds::status::run_cmd(account, &action).await
        }
        Command::Pair { alias } => {
            if ipc::try_exec(&IpcRequest::new("pair").with_account(&alias)).await.is_ok() { return Ok(()); }
            cmds::pair::run(&alias).await
        }
        Command::Dashboard { action } => match action {
            DashboardAction::Show => {
                if ipc::try_exec(&IpcRequest::new("dashboard")).await.is_ok() { return Ok(()); }
                cmds::dashboard::run().await
            }
            DashboardAction::Watch => cmds::dashboard_watch::run().await,
        },
        Command::Chat { account, action } => match action {
            ChatAction::List => {
                if ipc::try_exec(&IpcRequest::new("chat_list").with_account(&account)).await.is_ok() { return Ok(()); }
                cmds::chat::list(&account).await
            }
            ChatAction::Unread => {
                if ipc::try_exec(&IpcRequest::new("chat_unread").with_account(&account)).await.is_ok() { return Ok(()); }
                cmds::chat::unread(&account).await
            }
            ChatAction::Search { query } => {
                if ipc::try_exec(&IpcRequest::new("chat_search").with_account(&account).with_query(&query)).await.is_ok() { return Ok(()); }
                cmds::chat::search(&account, &query).await
            }
            ChatAction::MarkRead { chat_id } => {
                if ipc::try_exec(&IpcRequest::new("chat_mark_read").with_account(&account).with_chat_id(&chat_id)).await.is_ok() { return Ok(()); }
                cmds::chat::mark_read(&account, &chat_id).await
            }
            ChatAction::History { chat_id, limit, before, after, offset, total } => {
                let mut req = IpcRequest::new("chat_history").with_account(&account).with_chat_id(&chat_id).with_query(&limit.to_string()).with_text(&offset.to_string());
                if let Some(ts) = before { req = req.with_before(ts); }
                if let Some(ts) = after { req = req.with_after(ts); }
                if total { req = req.with_total(true); }
                if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
                cmds::chat::history(&account, &chat_id, limit, before, after, offset, total).await
            }
            ChatAction::Listen { chat_id, timeout } => {
                cmds::chat_listen::run(&account, &chat_id, timeout).await
            }
        },
        Command::Message { account, action } => {
            use cmds::message::MessageCommand;
            let req = match &action {
                MessageCommand::Send { chat_id, text, wait } => {
                    let mut r = IpcRequest::new("message_send").with_account(&account).with_chat_id(chat_id).with_text(text);
                    if let Some(w) = wait { r = r.with_wait(true).with_timeout(*w); }
                    r
                }
                MessageCommand::Reply { chat_id, reply_to_id, text, wait } => {
                    let mut r = IpcRequest::new("message_reply").with_account(&account).with_chat_id(chat_id).with_reply_to(reply_to_id).with_text(text);
                    if let Some(w) = wait { r = r.with_wait(true).with_timeout(*w); }
                    r
                }
                MessageCommand::React { chat_id, message_id, emoji, wait } => {
                    let mut r = IpcRequest::new("message_react").with_account(&account).with_chat_id(chat_id).with_message_id(message_id).with_emoji(emoji);
                    if let Some(w) = wait { r = r.with_wait(true).with_timeout(*w); }
                    r
                }
                MessageCommand::Edit { chat_id, message_id, text, wait } => {
                    let mut r = IpcRequest::new("message_edit").with_account(&account).with_chat_id(chat_id).with_message_id(message_id).with_text(text);
                    if let Some(w) = wait { r = r.with_wait(true).with_timeout(*w); }
                    r
                }
                MessageCommand::Delete { chat_id, message_id, wait } => {
                    let mut r = IpcRequest::new("message_delete").with_account(&account).with_chat_id(chat_id).with_message_id(message_id);
                    if let Some(w) = wait { r = r.with_wait(true).with_timeout(*w); }
                    r
                }
            };
            if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
            cmds::message::run(&account, action).await
        }
        Command::Media { account, action } => {
            use cmds::media::MediaCommand;
            let req = match &action {
                MediaCommand::Send { chat_id, file_path, text, reply_to, as_document, wait } => {
                    let mut r = IpcRequest::new("media_send").with_account(&account).with_chat_id(chat_id).with_file_path(file_path);
                    if let Some(t) = text { r = r.with_text(t); }
                    if let Some(rt) = reply_to { r = r.with_reply_to(rt); }
                    if *as_document { r = r.with_as_document(true); }
                    if let Some(w) = wait { r = r.with_wait(true).with_timeout(*w); }
                    r
                }
                MediaCommand::Download { message_id } =>
                    IpcRequest::new("media_download").with_account(&account).with_message_id(message_id),
            };
            if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
            cmds::media::run(&account, action).await
        }
        Command::Contact { account, action } => {
            use cmds::contact::ContactCommand;
            let req = match &action {
                ContactCommand::Search { query } =>
                    IpcRequest::new("contact_search").with_account(&account).with_query(query),
                ContactCommand::Info { jid } =>
                    IpcRequest::new("contact_info").with_account(&account).with_jid(jid),
                ContactCommand::Score { jid, value } =>
                    IpcRequest::new("contact_score").with_account(&account).with_jid(jid).with_score_value(*value),
            };
            if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
            cmds::contact::run(&account, action).await
        }
        Command::Memory { account, action } => {
            use cmds::memory::MemoryCommand;
            let req = match &action {
                MemoryCommand::Get { chat_id } =>
                    IpcRequest::new("memory_get").with_account(&account).with_chat_id(chat_id),
                MemoryCommand::Set { chat_id, json } =>
                    IpcRequest::new("memory_set").with_account(&account).with_chat_id(chat_id).with_text(json),
                MemoryCommand::Search { query } =>
                    IpcRequest::new("memory_search").with_account(&account).with_query(query),
            };
            if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
            cmds::memory::run(&account, action).await
        }
        Command::Graph { account, action } => {
            use cmds::graph::GraphCommand;
            let req = match &action {
                GraphCommand::SharedGroups { jid1, jid2 } =>
                    IpcRequest::new("graph_shared_groups").with_account(&account).with_jid1(jid1).with_jid2(jid2),
                GraphCommand::Introductions { jid } =>
                    IpcRequest::new("graph_introductions").with_account(&account).with_jid(jid),
                GraphCommand::ImportantContacts { community_id } =>
                    IpcRequest::new("graph_important_contacts").with_account(&account).with_community_id(community_id),
            };
            if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
            cmds::graph::run(&account, action).await
        }
        Command::Community { account, action } => {
            use cmds::community::CommunityCommand;
            let req = match &action {
                CommunityCommand::List =>
                    IpcRequest::new("community_list").with_account(&account),
                CommunityCommand::Info { community_id } =>
                    IpcRequest::new("community_info").with_account(&account).with_community_id(community_id),
            };
            if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
            cmds::community::run(&account, action).await
        }
        Command::Task { action } => {
            use cmds::task::TaskCommand;
            let req = match &action {
                TaskCommand::List { status } => {
                    let mut r = IpcRequest::new("task_list");
                    if let Some(s) = status { r = r.with_status(s); }
                    r
                },
                TaskCommand::Complete { task_id } =>
                    IpcRequest::new("task_complete").with_query(&task_id.to_string()),
            };
            if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
            cmds::task::run(action).await
        }
    }
}
