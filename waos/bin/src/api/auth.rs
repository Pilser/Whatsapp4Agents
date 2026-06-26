use std::sync::Arc;

use hmac::{Hmac, Mac};
use sha2::Sha256;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Serialize, Deserialize};
use chrono::Utc;

use crate::server::ApiState;

#[derive(Serialize, Deserialize)]
struct JwtPayload {
    sub: String,
    iat: i64,
    exp: i64,
}

pub fn sign_token(secret: &str) -> Option<String> {
    let now = Utc::now().timestamp();
    let payload = JwtPayload {
        sub: "admin".into(),
        iat: now,
        exp: now + 86400,
    };
    let payload_json = serde_json::to_string(&payload).ok()?;
    let payload_b64 = URL_SAFE_NO_PAD.encode(payload_json);

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(b"header.payload");
    let _ = mac.finalize();

    let header_b64 = URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","typ":"JWT"}"#);
    let to_sign = format!("{header_b64}.{payload_b64}");

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).ok()?;
    mac.update(to_sign.as_bytes());
    let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

    Some(format!("{to_sign}.{sig}"))
}

pub fn verify_token(token: &str, secret: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return false;
    }
    let to_sign = format!("{}.{}", parts[0], parts[1]);
    let mut mac = match Hmac::<Sha256>::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(to_sign.as_bytes());
    let expected = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
    if expected != parts[2] {
        return false;
    }
    let payload_json = match URL_SAFE_NO_PAD.decode(parts[1]) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let payload: JwtPayload = match serde_json::from_slice(&payload_json) {
        Ok(p) => p,
        Err(_) => return false,
    };
    Utc::now().timestamp() < payload.exp
}

pub fn require_auth(req: &hyper::Request<hyper::Body>, state: &Arc<ApiState>) -> Option<hyper::Response<hyper::Body>> {
    let secret = state.daemon.config.server.jwt_secret.as_ref()?;

    let header = req
        .headers()
        .get("authorization")?
        .to_str()
        .ok()?;
    let token = header.strip_prefix("Bearer ")?;

    if !verify_token(token, secret) {
        return Some(un_auth());
    }
    None
}

fn un_auth() -> hyper::Response<hyper::Body> {
    hyper::Response::builder()
        .status(401)
        .header("content-type", "application/json")
        .body(hyper::Body::from(r#"{"error":"unauthorized"}"#))
        .unwrap()
}

pub fn un_auth_response() -> hyper::Response<hyper::Body> {
    un_auth()
}

pub async fn token(state: &Arc<ApiState>) -> hyper::Response<hyper::Body> {
    let secret = match &state.daemon.config.server.jwt_secret {
        Some(s) => s,
        None => {
            return hyper::Response::builder()
                .status(401)
                .header("content-type", "application/json")
                .body(hyper::Body::from(r#"{"error":"JWT not configured"}"#))
                .unwrap();
        }
    };

    match sign_token(secret) {
        Some(t) => {
            let body = serde_json::to_string(&serde_json::json!({"token": t})).unwrap();
            hyper::Response::builder()
                .status(200)
                .header("content-type", "application/json")
                .body(hyper::Body::from(body))
                .unwrap()
        }
        None => {
            hyper::Response::builder()
                .status(500)
                .header("content-type", "application/json")
                .body(hyper::Body::from(r#"{"error":"failed to sign token"}"#))
                .unwrap()
        }
    }
}
