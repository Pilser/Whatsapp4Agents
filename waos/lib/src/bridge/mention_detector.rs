pub fn extract_mentions(content: &str) -> Vec<String> {
    content
        .split_whitespace()
        .filter(|w| w.starts_with('@'))
        .map(|w| w.trim_start_matches('@').to_string())
        .collect()
}

pub fn is_agent_mentioned(content: &str, agent_jid: &str) -> bool {
    extract_mentions(content).iter().any(|m| m == agent_jid)
}
