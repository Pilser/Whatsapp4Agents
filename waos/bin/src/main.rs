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
    Group {
        #[arg(long)]
        account: String,
        #[command(subcommand)]
        action: cmds::group::GroupCommand,
    },
    Task {
        #[command(subcommand)]
        action: cmds::task::TaskCommand,
    },
    /// Show categorized help. Use `waos help <category>` for details.
    Help {
        /// Optional category name (status, chat, message, media, contact, memory, graph, community, task, dashboard)
        #[arg(default_value = None)]
        category: Option<String>,
    },
}

#[derive(Subcommand)]
enum ChatAction {
    List {
        /// Block and wait up to N seconds for result (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
    Unread {
        /// Block and wait up to N seconds for result (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
    Search {
        query: String,
        /// Block and wait up to N seconds for result (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
    MarkRead {
        chat_id: String,
        /// Block and wait up to N seconds for result (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
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
        /// Block and wait up to N seconds for result (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
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
                StatusCommand::List { .. } => "",
                StatusCommand::SendText { account, .. } => account.as_str(),
                StatusCommand::SendImage { account, .. } => account.as_str(),
                StatusCommand::SendVideo { account, .. } => account.as_str(),
                StatusCommand::Revoke { account, .. } => account.as_str(),
                StatusCommand::History { account, .. } => account.as_str(),
                StatusCommand::Delete { account, .. } => account.as_str(),
            };
            let req = match &action {
                StatusCommand::List { .. } => IpcRequest::new("status"),
                StatusCommand::SendText { text, bg_color, font, wait, .. } => {
                    let meta = serde_json::json!({"bg": bg_color, "font": font});
                    let mut r = IpcRequest::new("status_send_text").with_account(account).with_text(text).with_metadata(&meta.to_string());
                    match wait {
                        Some(0) => {},
                        Some(n) => { r = r.with_wait(true).with_timeout(*n); },
                        None => { r = r.with_wait(true).with_timeout(30); },
                    }
                    r
                }
                StatusCommand::SendImage { file_path, thumbnail_file, caption, wait, .. } => {
                    let meta = serde_json::json!({"thumbnail": thumbnail_file});
                    let mut r = IpcRequest::new("status_send_image").with_account(account).with_file_path(file_path).with_metadata(&meta.to_string());
                    if let Some(c) = caption { r = r.with_text(c); }
                    match wait {
                        Some(0) => {},
                        Some(n) => { r = r.with_wait(true).with_timeout(*n); },
                        None => { r = r.with_wait(true).with_timeout(30); },
                    }
                    r
                }
                StatusCommand::SendVideo { file_path, thumbnail_file, caption, wait, .. } => {
                    let meta = serde_json::json!({"thumbnail": thumbnail_file});
                    let mut r = IpcRequest::new("status_send_video").with_account(account).with_file_path(file_path).with_metadata(&meta.to_string());
                    if let Some(c) = caption { r = r.with_text(c); }
                    match wait {
                        Some(0) => {},
                        Some(n) => { r = r.with_wait(true).with_timeout(*n); },
                        None => { r = r.with_wait(true).with_timeout(30); },
                    }
                    r
                }
                StatusCommand::Revoke { message_id, wait, .. } => {
                    let mut r = IpcRequest::new("status_revoke").with_account(account).with_message_id(message_id);
                    match wait {
                        Some(0) => {},
                        Some(n) => { r = r.with_wait(true).with_timeout(*n); },
                        None => { r = r.with_wait(true).with_timeout(30); },
                    }
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
            ChatAction::List { .. } => {
                if ipc::try_exec(&IpcRequest::new("chat_list").with_account(&account)).await.is_ok() { return Ok(()); }
                cmds::chat::list(&account).await
            }
            ChatAction::Unread { .. } => {
                if ipc::try_exec(&IpcRequest::new("chat_unread").with_account(&account)).await.is_ok() { return Ok(()); }
                cmds::chat::unread(&account).await
            }
            ChatAction::Search { query, .. } => {
                if ipc::try_exec(&IpcRequest::new("chat_search").with_account(&account).with_query(&query)).await.is_ok() { return Ok(()); }
                cmds::chat::search(&account, &query).await
            }
            ChatAction::MarkRead { chat_id, .. } => {
                if ipc::try_exec(&IpcRequest::new("chat_mark_read").with_account(&account).with_chat_id(&chat_id)).await.is_ok() { return Ok(()); }
                cmds::chat::mark_read(&account, &chat_id).await
            }
            ChatAction::History { chat_id, limit, before, after, offset, total, .. } => {
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
                    match wait {
                        Some(0) => {},
                        Some(n) => { r = r.with_wait(true).with_timeout(*n); },
                        None => { r = r.with_wait(true).with_timeout(30); },
                    }
                    r
                }
                MessageCommand::Reply { chat_id, reply_to_id, text, wait } => {
                    let mut r = IpcRequest::new("message_reply").with_account(&account).with_chat_id(chat_id).with_reply_to(reply_to_id).with_text(text);
                    match wait {
                        Some(0) => {},
                        Some(n) => { r = r.with_wait(true).with_timeout(*n); },
                        None => { r = r.with_wait(true).with_timeout(30); },
                    }
                    r
                }
                MessageCommand::React { chat_id, message_id, emoji, wait } => {
                    let mut r = IpcRequest::new("message_react").with_account(&account).with_chat_id(chat_id).with_message_id(message_id).with_emoji(emoji);
                    match wait {
                        Some(0) => {},
                        Some(n) => { r = r.with_wait(true).with_timeout(*n); },
                        None => { r = r.with_wait(true).with_timeout(30); },
                    }
                    r
                }
                MessageCommand::Edit { chat_id, message_id, text, wait } => {
                    let mut r = IpcRequest::new("message_edit").with_account(&account).with_chat_id(chat_id).with_message_id(message_id).with_text(text);
                    match wait {
                        Some(0) => {},
                        Some(n) => { r = r.with_wait(true).with_timeout(*n); },
                        None => { r = r.with_wait(true).with_timeout(30); },
                    }
                    r
                }
                MessageCommand::Delete { chat_id, message_id, wait } => {
                    let mut r = IpcRequest::new("message_delete").with_account(&account).with_chat_id(chat_id).with_message_id(message_id);
                    match wait {
                        Some(0) => {},
                        Some(n) => { r = r.with_wait(true).with_timeout(*n); },
                        None => { r = r.with_wait(true).with_timeout(30); },
                    }
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
                    match wait {
                        Some(0) => {},
                        Some(n) => { r = r.with_wait(true).with_timeout(*n); },
                        None => { r = r.with_wait(true).with_timeout(30); },
                    }
                    r
                }
                MediaCommand::Download { message_id, .. } =>
                    IpcRequest::new("media_download").with_account(&account).with_message_id(message_id),
            };
            if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
            cmds::media::run(&account, action).await
        }
        Command::Contact { account, action } => {
            use cmds::contact::ContactCommand;
            let req = match &action {
                ContactCommand::Search { query, .. } =>
                    IpcRequest::new("contact_search").with_account(&account).with_query(query),
                ContactCommand::Info { jid, .. } =>
                    IpcRequest::new("contact_info").with_account(&account).with_jid(jid),
                ContactCommand::Score { jid, value, .. } =>
                    IpcRequest::new("contact_score").with_account(&account).with_jid(jid).with_score_value(*value),
            };
            if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
            cmds::contact::run(&account, action).await
        }
        Command::Memory { account, action } => {
            use cmds::memory::MemoryCommand;
            let req = match &action {
                MemoryCommand::Get { chat_id, .. } =>
                    IpcRequest::new("memory_get").with_account(&account).with_chat_id(chat_id),
                MemoryCommand::Set { chat_id, json, .. } =>
                    IpcRequest::new("memory_set").with_account(&account).with_chat_id(chat_id).with_text(json),
                MemoryCommand::Search { query, .. } =>
                    IpcRequest::new("memory_search").with_account(&account).with_query(query),
            };
            if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
            cmds::memory::run(&account, action).await
        }
        Command::Graph { account, action } => {
            use cmds::graph::GraphCommand;
            let req = match &action {
                GraphCommand::SharedGroups { jid1, jid2, .. } =>
                    IpcRequest::new("graph_shared_groups").with_account(&account).with_jid1(jid1).with_jid2(jid2),
                GraphCommand::Introductions { jid, .. } =>
                    IpcRequest::new("graph_introductions").with_account(&account).with_jid(jid),
                GraphCommand::ImportantContacts { community_id, .. } =>
                    IpcRequest::new("graph_important_contacts").with_account(&account).with_community_id(community_id),
            };
            if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
            cmds::graph::run(&account, action).await
        }
        Command::Community { account, action } => {
            use cmds::community::CommunityCommand;
            let req = match &action {
                CommunityCommand::List { .. } =>
                    IpcRequest::new("community_list").with_account(&account),
                CommunityCommand::Info { community_id, .. } =>
                    IpcRequest::new("community_info").with_account(&account).with_community_id(community_id),
            };
            if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
            cmds::community::run(&account, action).await
        }
        Command::Group { account, action } => {
            use cmds::group::GroupCommand;
            match &action {
                GroupCommand::List => {
                    let req = IpcRequest::new("group_list").with_account(&account);
                    if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
                }
                GroupCommand::Enable { group_id } => {
                    let req = IpcRequest::new("group_enable").with_account(&account).with_chat_id(group_id);
                    if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
                }
                GroupCommand::Disable { group_id } => {
                    let req = IpcRequest::new("group_disable").with_account(&account).with_chat_id(group_id);
                    if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
                }
                GroupCommand::Mentions { .. } | GroupCommand::Replies { .. } => {}
            }
            cmds::group::run(&account, action).await
        }
        Command::Task { action } => {
            use cmds::task::TaskCommand;
            let req = match &action {
                TaskCommand::List { status, .. } => {
                    let mut r = IpcRequest::new("task_list");
                    if let Some(s) = status { r = r.with_status(s); }
                    r
                },
                TaskCommand::Complete { task_id, .. } =>
                    IpcRequest::new("task_complete").with_query(&task_id.to_string()),
            };
            if ipc::try_exec(&req).await.is_ok() { return Ok(()); }
            cmds::task::run(action).await
        }
        Command::Help { category } => {
            cmd_help(category).await
        }
    }
}

async fn cmd_help(category: Option<String>) -> anyhow::Result<()> {
    match category.as_deref() {
        None => {
            println!("WAOS — WhatsApp Agent Operating System");
            println!();
            println!("Usage: waos <command> [options]");
            println!();
            println!("Categories:");
            println!("  waos help status     Account status, daemon health, pairing");
            println!("  waos help chat       List, search, history, listen to chats");
            println!("  waos help message    Send, reply, react, edit, delete messages");
            println!("  waos help media      Send media files, download metadata");
            println!("  waos help contact    Search contacts, view info, set scores");
            println!("  waos help memory     Get, set, search conversation memory");
            println!("  waos help graph      Shared groups, introductions, important contacts");
            println!("  waos help community  List communities, view members");
            println!("  waos help group      List groups, enable/disable message storage");
            println!("  waos help stories    Post text/image/video status, revoke, history");
            println!("  waos help task       List and complete tasks");
            println!("  waos help dashboard  Show live dashboard, stream updates");
            println!();
            println!("Flags:");
            println!("  --wait          Wait up to 30s for result (default on queue commands)");
            println!("  --wait <secs>   Wait N seconds, then drop connection (command continues)");
            println!("  --wait 0        Return immediately (opt out of waiting)");
            println!();
            println!("Run `waos help <category>` for detailed commands.");
        }
        Some("status") | Some("daemon") | Some("pair") => {
            println!("waos status                          list accounts with connection status");
            println!("waos daemon                          start daemon (bots, API, IPC)");
            println!("waos pair <alias>                    show pairing instructions");
        }
        Some("chat") => {
            println!("All chat commands require --account <name>");
            println!();
            println!("waos chat --account <name> list                        all chats, most recent first");
            println!("waos chat --account <name> unread                      only unread chats");
            println!("waos chat --account <name> search <query>              search messages");
            println!("waos chat --account <name> history <chat_id>           last 5 messages (is_me flag)");
            println!("  --limit N   --before TS   --after TS   --offset N   --total");
            println!("waos chat --account <name> mark-read <chat_id>         mark as read");
            println!("waos chat --account <name> listen <chat_id>            stream events (--timeout N)");
        }
        Some("message") => {
            println!("All message commands require --account <name>");
            println!();
            println!("waos message --account <name> send <chat_id> <text>           queue text");
            println!("waos message --account <name> reply <chat_id> <id> <text>     queue reply");
            println!("waos message --account <name> react <chat_id> <id> <emoji>    queue reaction");
            println!("waos message --account <name> edit <chat_id> <id> <text>      queue edit");
            println!("waos message --account <name> delete <chat_id> <id>           queue delete");
            println!();
            println!("Use --wait to block until send confirmed.");
        }
        Some("media") => {
            println!("All media commands require --account <name>");
            println!("File path is on the daemon filesystem.");
            println!();
            println!("waos media --account <name> send <chat_id> <file>             auto-detect type");
            println!("  --text <caption>   --reply-to <id>   --as-document");
            println!("waos media --account <name> download <message_id>             lookup metadata");
        }
        Some("contact") => {
            println!("All contact commands require --account <name>");
            println!();
            println!("waos contact --account <name> search <query>                  search by name/phone");
            println!("waos contact --account <name> info <jid>                      full profile");
            println!("waos contact --account <name> score <jid> <0.0-1.0>           set importance");
        }
        Some("memory") => {
            println!("All memory commands require --account <name>");
            println!();
            println!("waos memory --account <name> get <chat_id>                    read conversation memory");
            println!("waos memory --account <name> set <chat_id> <json>             store memory");
            println!("waos memory --account <name> search <query>                   search across chats");
        }
        Some("graph") => {
            println!("All graph commands require --account <name>");
            println!();
            println!("waos graph --account <name> shared-groups <jid1> <jid2>      find shared groups");
            println!("waos graph --account <name> introductions <jid>               find intro paths");
            println!("waos graph --account <name> important-contacts <cid>          key contacts");
        }
        Some("community") => {
            println!("All community commands require --account <name>");
            println!();
            println!("waos community --account <name> list                          list communities");
            println!("waos community --account <name> info <community_id>           details + members");
        }
        Some("group") => {
            println!("All group commands require --account <name>");
            println!();
            println!("waos group --account <name> list                              list groups/communities");
            println!("waos group --account <name> enable <group_id>                 enable message storage");
            println!("waos group --account <name> disable <group_id>                disable message storage");
            println!("waos group --account <name> mentions [group_id]               messages that tagged the agent");
            println!("  --limit N");
            println!("waos group --account <name> replies [group_id]                replies to the agent");
            println!("  --limit N");
        }
        Some("stories") | Some("story") => {
            println!("Stories post to ALL contacts. Files on daemon filesystem.");
            println!();
            println!("waos status send-text --account <name> <text>                 post text story");
            println!("  --bg-color 0xAARRGGBB (default 0xFF1E6E4F)  --font 0-4 (default 0)");
            println!("waos status send-image --account <name> <file>                post image story");
            println!("  --thumbnail-file <thumb>  --caption <text>");
            println!("waos status send-video --account <name> <file>                post video story");
            println!("  --thumbnail-file <thumb>  --caption <text>");
            println!("waos status revoke --account <name> <message_id>              delete from WhatsApp");
            println!("waos status history --account <name>                          list posted stories");
            println!("  --status all|expired|posted|revoked  --limit N  --before TS  --after TS  --offset N");
            println!("waos status delete --account <name>                           DB cleanup (dry-run)");
            println!("  --status filter  --older-than TS  --newer-than TS  --yes");
        }
        Some("task") => {
            println!("waos task list                                               list tasks");
            println!("  --status <filter>");
            println!("waos task complete <task_id>                                 mark completed");
        }
        Some("dashboard") => {
            println!("waos dashboard show                                          live dashboard JSON");
            println!("waos dashboard watch                                         stream updates");
        }
        Some(other) => {
            println!("Unknown category: {other}");
            println!("Run `waos help` for available categories.");
        }
    }
    Ok(())
}
