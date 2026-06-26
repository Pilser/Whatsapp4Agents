use crate::ipc;

pub async fn run(alias: &str, chat_id: &str, timeout: u64) -> anyhow::Result<()> {
    let req = ipc::IpcRequest::new("chat_watch")
        .with_account(alias)
        .with_chat_id(chat_id)
        .with_timeout(timeout);
    ipc::send_stream_command(&req).await.map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}
