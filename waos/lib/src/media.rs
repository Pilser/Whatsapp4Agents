use std::path::Path;

use wacore::download::MediaType;
use whatsapp_rust::client::Client;
use whatsapp_rust::download::Downloadable;
use whatsapp_rust::upload::UploadOptions;
use whatsapp_rust::Jid;
use whatsapp_rust::SendResult;
use waproto::whatsapp as wa;

pub async fn download_media(client: &Client, msg: &wa::Message, cache_dir: &Path) -> anyhow::Result<String> {
    let downloadable = get_downloadable(msg)
        .ok_or_else(|| anyhow::anyhow!("No media in message"))?;
    let data = client.download(downloadable).await?;
    let ext = guess_extension(msg);
    let filename = format!("{}.{}", "media", ext);
    let path = cache_dir.join(&filename);
    tokio::fs::write(&path, &data).await?;
    Ok(path.to_string_lossy().to_string())
}

const IMAGE_SIZE_LIMIT: u64 = 64 * 1024 * 1024;
const VIDEO_SIZE_LIMIT: u64 = 64 * 1024 * 1024;
const AUDIO_SIZE_LIMIT: u64 = 64 * 1024 * 1024;
const DOCUMENT_SIZE_LIMIT: u64 = 100 * 1024 * 1024;

const STATUS_IMAGE_SIZE_LIMIT: u64 = 10 * 1024 * 1024;
const STATUS_VIDEO_SIZE_LIMIT: u64 = 16 * 1024 * 1024;

pub fn check_status_file_size(file_path: &str, media_type: &str) -> anyhow::Result<()> {
    let meta = std::fs::metadata(file_path)?;
    let size = meta.len();
    let limit = match media_type {
        "image" => STATUS_IMAGE_SIZE_LIMIT,
        "video" => STATUS_VIDEO_SIZE_LIMIT,
        _ => STATUS_IMAGE_SIZE_LIMIT,
    };
    if size > limit {
        let mb = size as f64 / (1024.0 * 1024.0);
        let max_mb = limit / (1024 * 1024);
        anyhow::bail!(
            "File is {:.1}MB, exceeds {max_mb}MB limit for status {media_type}. \
             file too large or has high resolution not supported by WhatsApp, \
             convert it or find another way supported",
            mb,
        );
    }
    Ok(())
}

fn check_file_size(file_path: &str, media_type: &str) -> anyhow::Result<()> {
    let meta = std::fs::metadata(file_path)?;
    let size = meta.len();
    let (limit, type_name) = match media_type {
        "image" => (IMAGE_SIZE_LIMIT, "image"),
        "video" => (VIDEO_SIZE_LIMIT, "video"),
        "audio" => (AUDIO_SIZE_LIMIT, "audio"),
        _ => (DOCUMENT_SIZE_LIMIT, "document"),
    };
    if size > limit {
        let mb = size as f64 / (1024.0 * 1024.0);
        let max_mb = limit / (1024 * 1024);
        anyhow::bail!(
            "File is {:.1}MB, exceeds {max_mb}MB limit for {type_name}. \
             Use --as-document to send as document, or compress the file.",
            mb,
        );
    }
    Ok(())
}

pub async fn upload_and_send(
    client: &Client,
    chat_id: &Jid,
    file_path: &str,
    media_type: &str,
    quote_id: Option<&str>,
    caption: Option<&str>,
) -> anyhow::Result<SendResult> {
    check_file_size(file_path, media_type)?;
    let data = tokio::fs::read(file_path).await?;
    let path = Path::new(file_path);
    let filename = path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let mt = match media_type {
        "image" => MediaType::Image,
        "video" => MediaType::Video,
        "audio" => MediaType::Audio,
        _ => MediaType::Document,
    };

    let mime = guess_mime(media_type, &filename);
    let upload = client.upload(data, mt, UploadOptions::default()).await?;

    let dims: Option<(u32, u32, u32)> = match media_type {
        "image" => get_image_dimensions(file_path).map(|(w, h)| (w, h, 0)),
        "video" => probe_video_info(file_path),
        _ => None,
    };

    let mut msg = build_media_message(&upload, media_type, &mime, &filename, caption, dims);
    if let Some(qid) = quote_id {
        add_context_info(&mut msg, qid);
    }
    let result = client.send_message(chat_id.clone(), msg).await?;
    Ok(result)
}

fn add_context_info(msg: &mut wa::Message, quote_id: &str) {
    let ctx = || wa::ContextInfo {
        stanza_id: Some(quote_id.to_string()),
        ..Default::default()
    };
    if let Some(ref mut img) = msg.image_message.as_option_mut() {
        img.context_info = buffa::MessageField::some(ctx());
    } else if let Some(ref mut vid) = msg.video_message.as_option_mut() {
        vid.context_info = buffa::MessageField::some(ctx());
    } else if let Some(ref mut aud) = msg.audio_message.as_option_mut() {
        aud.context_info = buffa::MessageField::some(ctx());
    } else if let Some(ref mut doc) = msg.document_message.as_option_mut() {
        doc.context_info = buffa::MessageField::some(ctx());
    }
}

fn get_downloadable(msg: &wa::Message) -> Option<&dyn Downloadable> {
    if let Some(img) = msg.image_message.as_option() {
        Some(img as &dyn Downloadable)
    } else if let Some(doc) = msg.document_message.as_option() {
        Some(doc as &dyn Downloadable)
    } else if let Some(audio) = msg.audio_message.as_option() {
        Some(audio as &dyn Downloadable)
    } else if let Some(video) = msg.video_message.as_option() {
        Some(video as &dyn Downloadable)
    } else if let Some(sticker) = msg.sticker_message.as_option() {
        Some(sticker as &dyn Downloadable)
    } else {
        None
    }
}

fn guess_extension(msg: &wa::Message) -> &'static str {
    if msg.image_message.is_set() { "jpg" }
    else if msg.video_message.is_set() { "mp4" }
    else if msg.audio_message.is_set() { "ogg" }
    else if msg.document_message.is_set() { "bin" }
    else { "bin" }
}

fn guess_mime(media_type: &str, filename: &str) -> String {
    match media_type {
        "image" => "image/jpeg".to_string(),
        "video" => "video/mp4".to_string(),
        "audio" => {
            if filename.ends_with(".ogg") { "audio/ogg; codecs=opus".to_string() }
            else { "audio/mpeg".to_string() }
        }
        _ => {
            let ext = Path::new(filename).extension()
                .and_then(|e| e.to_str())
                .unwrap_or("bin");
            match ext {
                "pdf" => "application/pdf".to_string(),
                "doc" | "docx" => "application/msword".to_string(),
                "txt" => "text/plain".to_string(),
                "zip" => "application/zip".to_string(),
                "png" => "image/png".to_string(),
                "jpg" | "jpeg" => "image/jpeg".to_string(),
                "mp4" => "video/mp4".to_string(),
                "mp3" => "audio/mpeg".to_string(),
                "ogg" => "audio/ogg".to_string(),
                _ => "application/octet-stream".to_string(),
            }
        }
    }
}

/// Read image dimensions from JPEG or PNG file headers.
/// Returns `(width, height)` or `None` if format is unsupported or unreadable.
pub fn get_image_dimensions(path: &str) -> Option<(u32, u32)> {
    let data = std::fs::read(path).ok()?;
    if data.len() < 24 {
        return None;
    }
    // JPEG: starts with FF D8
    if data[0] == 0xFF && data[1] == 0xD8 {
        let mut pos = 2;
        while pos + 8 < data.len() {
            if data[pos] != 0xFF {
                return None;
            }
            let marker = data[pos + 1];
            // SOF0, SOF1, SOF2, SOF3 (progressive)
            if matches!(marker, 0xC0 | 0xC1 | 0xC2) {
                let h = u16::from_be_bytes([data[pos + 5], data[pos + 6]]);
                let w = u16::from_be_bytes([data[pos + 7], data[pos + 8]]);
                return Some((w as u32, h as u32));
            }
            let seg_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
            if seg_len < 2 {
                return None;
            }
            pos += seg_len as usize + 2;
        }
        return None;
    }
    // PNG: 8-byte signature + IHDR chunk
    if data[0] == 0x89 && data[1] == 0x50 && data[2] == 0x4E && data[3] == 0x47 {
        let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        return Some((w, h));
    }
    None
}

/// Probe video dimensions and duration using ffprobe.
/// Returns `(width, height, duration_secs)` or `None` if ffprobe is unavailable.
pub fn probe_video_info(path: &str) -> Option<(u32, u32, u32)> {
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_streams",
            "-show_format",
            path,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let streams = json.get("streams")?.as_array()?;
    let video_stream = streams.iter().find(|s| {
        s.get("codec_type").and_then(|v| v.as_str()) == Some("video")
    })?;
    let width = video_stream.get("width")?.as_u64()? as u32;
    let height = video_stream.get("height")?.as_u64()? as u32;
    let duration_secs = json
        .get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str())
        .and_then(|d| d.parse::<f64>().ok())
        .map(|s| s.ceil() as u32)
        .or_else(|| {
            video_stream
                .get("duration")
                .and_then(|d| d.as_str())
                .and_then(|d| d.parse::<f64>().ok())
                .map(|s| s.ceil() as u32)
        })
        .unwrap_or(0);
    Some((width, height, duration_secs))
}

fn build_media_message(
    upload: &whatsapp_rust::upload::UploadResponse,
    media_type: &str,
    mime: &str,
    filename: &str,
    caption: Option<&str>,
    dims: Option<(u32, u32, u32)>,
) -> wa::Message {
    let url = upload.url.clone();
    let direct_path = upload.direct_path.clone();
    let media_key = upload.media_key.to_vec();
    let file_enc_sha256 = upload.file_enc_sha256.to_vec();
    let file_sha256 = upload.file_sha256.to_vec();
    let file_length = upload.file_length;
    let media_key_timestamp = upload.media_key_timestamp;

    match media_type {
        "image" => {
            let (w, h) = dims.map(|(w, h, _)| (w, h)).unwrap_or((0, 0));
            wa::Message {
                image_message: buffa::MessageField::some(wa::message::ImageMessage {
                    url: Some(url),
                    direct_path: Some(direct_path),
                    media_key: Some(media_key),
                    file_enc_sha256: Some(file_enc_sha256),
                    file_sha256: Some(file_sha256),
                    file_length: Some(file_length),
                    media_key_timestamp: Some(media_key_timestamp),
                    mimetype: Some(mime.to_string()),
                    caption: caption.map(|c| c.to_string()),
                    width: Some(w).filter(|&v| v > 0),
                    height: Some(h).filter(|&v| v > 0),
                    ..Default::default()
                }),
                ..Default::default()
            }
        },
        "video" => {
            let (w, h, dur) = dims.unwrap_or((0, 0, 30));
            wa::Message {
                video_message: buffa::MessageField::some(wa::message::VideoMessage {
                    url: Some(url),
                    direct_path: Some(direct_path),
                    media_key: Some(media_key),
                    file_enc_sha256: Some(file_enc_sha256),
                    file_sha256: Some(file_sha256),
                    file_length: Some(file_length),
                    media_key_timestamp: Some(media_key_timestamp),
                    mimetype: Some(mime.to_string()),
                    caption: caption.map(|c| c.to_string()),
                    seconds: Some(dur),
                    width: Some(w).filter(|&v| v > 0),
                    height: Some(h).filter(|&v| v > 0),
                    ..Default::default()
                }),
                ..Default::default()
            }
        },
        "audio" => wa::Message {
            audio_message: buffa::MessageField::some(wa::message::AudioMessage {
                url: Some(url),
                direct_path: Some(direct_path),
                media_key: Some(media_key),
                file_enc_sha256: Some(file_enc_sha256),
                file_sha256: Some(file_sha256),
                file_length: Some(file_length),
                media_key_timestamp: Some(media_key_timestamp),
                mimetype: Some(mime.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        },
        _ => wa::Message {
            document_message: buffa::MessageField::some(wa::message::DocumentMessage {
                url: Some(url),
                direct_path: Some(direct_path),
                media_key: Some(media_key),
                file_enc_sha256: Some(file_enc_sha256),
                file_sha256: Some(file_sha256),
                file_length: Some(file_length),
                media_key_timestamp: Some(media_key_timestamp),
                mimetype: Some(mime.to_string()),
                file_name: Some(filename.to_string()),
                caption: caption.map(|c| c.to_string()),
                ..Default::default()
            }),
            ..Default::default()
        },
    }
}
