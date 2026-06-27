use clap::Subcommand;

use waos_lib::config::Config;

async fn open_conn(alias: &str) -> anyhow::Result<turso::Connection> {
    let config = Config::load();
    let db_path = config.data_dir().join("accounts").join(format!("{alias}.db"));
    if !db_path.exists() {
        anyhow::bail!("No database found for account '{alias}'");
    }
    let db = turso::Builder::new_local(db_path.to_str().unwrap()).build().await?;
    Ok(db.connect()?)
}

#[derive(Subcommand)]
pub enum GraphCommand {
    SharedGroups {
        jid1: String,
        jid2: String,
        /// Block and wait up to N seconds for result (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
    Introductions {
        jid: String,
        /// Block and wait up to N seconds for result (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
    ImportantContacts {
        community_id: String,
        /// Block and wait up to N seconds for result (--wait alone defaults to 30)
        #[arg(long, default_missing_value = "30", num_args = 0..=1)]
        wait: Option<u64>,
    },
}

pub async fn run(alias: &str, cmd: GraphCommand) -> anyhow::Result<()> {
    match cmd {
        GraphCommand::SharedGroups { jid1, jid2, .. } => shared_groups(alias, &jid1, &jid2).await,
        GraphCommand::Introductions { jid, .. } => introductions(alias, &jid).await,
        GraphCommand::ImportantContacts { community_id, .. } => important_contacts(alias, &community_id).await,
    }
}

async fn shared_groups(alias: &str, jid1: &str, jid2: &str) -> anyhow::Result<()> {
    let graph = waos_lib::graph::RelationshipGraph::new();
    let conn = open_conn(alias).await?;
    graph.register_account(alias, conn).await;
    let groups = graph.shared_groups(alias, jid1, jid2).await;
    println!("{}", serde_json::to_string_pretty(&groups)?);
    Ok(())
}

async fn introductions(alias: &str, jid: &str) -> anyhow::Result<()> {
    let graph = waos_lib::graph::RelationshipGraph::new();
    let conn = open_conn(alias).await?;
    graph.register_account(alias, conn).await;
    let results = graph.introductions(alias, jid, 3).await;
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}

async fn important_contacts(alias: &str, community_id: &str) -> anyhow::Result<()> {
    let graph = waos_lib::graph::RelationshipGraph::new();
    let conn = open_conn(alias).await?;
    graph.register_account(alias, conn).await;
    let results = graph.important_contacts(alias, community_id).await;
    println!("{}", serde_json::to_string_pretty(&results)?);
    Ok(())
}
