#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WAVersion {
    pub primary: i64,
    pub secondary: i64,
    pub tertiary: i64,
}

impl std::fmt::Display for WAVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.primary, self.secondary, self.tertiary)
    }
}

pub fn fetch_wa_version() -> Result<WAVersion, String> {
    let url = "https://web.whatsapp.com/sw.js";
    let resp = ureq::get(url)
        .set("User-Agent", "Mozilla/5.0 (X11; Linux x86_64; rv:100.0) Gecko/20100101 Firefox/100.0")
        .set("Sec-Fetch-Dest", "script")
        .set("Sec-Fetch-Mode", "no-cors")
        .set("Sec-Fetch-Site", "same-origin")
        .set("Referer", "https://web.whatsapp.com/")
        .set("Accept", "*/*")
        .call()
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    let status = resp.status();
    let text = resp
        .into_string()
        .map_err(|e| format!("Failed to read response body: {e}"))?;

    if status != 200 {
        return Err(format!("HTTP {} (expected 200), body preview: {:?}", status, &text[..text.len().min(200)]));
    }

    let revision = parse_revision(&text)
        .ok_or_else(|| format!("Could not find client_revision in sw.js (got {} bytes)", text.len()))?;

    Ok(WAVersion {
        primary: 2,
        secondary: 3000,
        tertiary: revision,
    })
}

fn parse_revision(text: &str) -> Option<i64> {
    let marker = "client_revision";
    let start = text.find(marker)?;
    let after = &text[start + marker.len()..];

    let after = after.trim_start_matches(|c| c == '\\' || c == '"' || c == ':' || c == ' ');

    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse::<i64>().ok()
}

