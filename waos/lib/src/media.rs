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

    let mut msg = build_media_message(&upload, media_type, &mime, &filename, caption);
    if let Some(qid) = quote_id {
        add_context_info(&mut msg, qid);
    }
    let result = client.send_message(chat_id.clone(), msg).await?;
    Ok(result)
}

fn add_context_info(msg: &mut wa::Message, quote_id: &str) {
    let ctx = Box::new(wa::ContextInfo {
        stanza_id: Some(quote_id.to_string()),
        ..Default::default()
    });
    if let Some(ref mut img) = msg.image_message {
        img.context_info = Some(ctx);
    } else if let Some(ref mut vid) = msg.video_message {
        vid.context_info = Some(ctx);
    } else if let Some(ref mut aud) = msg.audio_message {
        aud.context_info = Some(ctx);
    } else if let Some(ref mut doc) = msg.document_message {
        doc.context_info = Some(ctx);
    }
}

fn get_downloadable(msg: &wa::Message) -> Option<&dyn Downloadable> {
    if let Some(ref img) = msg.image_message {
        Some(img.as_ref() as &dyn Downloadable)
    } else if let Some(ref doc) = msg.document_message {
        Some(doc.as_ref() as &dyn Downloadable)
    } else if let Some(ref audio) = msg.audio_message {
        Some(audio.as_ref() as &dyn Downloadable)
    } else if let Some(ref video) = msg.video_message {
        Some(video.as_ref() as &dyn Downloadable)
    } else if let Some(ref sticker) = msg.sticker_message {
        Some(sticker.as_ref() as &dyn Downloadable)
    } else {
        None
    }
}

fn guess_extension(msg: &wa::Message) -> &'static str {
    if msg.image_message.is_some() { "jpg" }
    else if msg.video_message.is_some() { "mp4" }
    else if msg.audio_message.is_some() { "ogg" }
    else if msg.document_message.is_some() { "bin" }
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

fn build_media_message(upload: &whatsapp_rust::upload::UploadResponse, media_type: &str, mime: &str, filename: &str, caption: Option<&str>) -> wa::Message {
    let url = upload.url.clone();
    let direct_path = upload.direct_path.clone();
    let media_key = upload.media_key.to_vec();
    let file_enc_sha256 = upload.file_enc_sha256.to_vec();
    let file_sha256 = upload.file_sha256.to_vec();
    let file_length = upload.file_length;

    match media_type {
        "image" => wa::Message {
            image_message: Some(Box::new(wa::message::ImageMessage {
                url: Some(url),
                direct_path: Some(direct_path),
                media_key: Some(media_key),
                file_enc_sha256: Some(file_enc_sha256),
                file_sha256: Some(file_sha256),
                file_length: Some(file_length),
                mimetype: Some(mime.to_string()),
                caption: caption.map(|c| c.to_string()),
                ..Default::default()
            })),
            ..Default::default()
        },
        "video" => wa::Message {
            video_message: Some(Box::new(wa::message::VideoMessage {
                url: Some(url),
                direct_path: Some(direct_path),
                media_key: Some(media_key),
                file_enc_sha256: Some(file_enc_sha256),
                file_sha256: Some(file_sha256),
                file_length: Some(file_length),
                mimetype: Some(mime.to_string()),
                caption: caption.map(|c| c.to_string()),
                ..Default::default()
            })),
            ..Default::default()
        },
        "audio" => wa::Message {
            audio_message: Some(Box::new(wa::message::AudioMessage {
                url: Some(url),
                direct_path: Some(direct_path),
                media_key: Some(media_key),
                file_enc_sha256: Some(file_enc_sha256),
                file_sha256: Some(file_sha256),
                file_length: Some(file_length),
                mimetype: Some(mime.to_string()),
                ..Default::default()
            })),
            ..Default::default()
        },
        _ => wa::Message {
            document_message: Some(Box::new(wa::message::DocumentMessage {
                url: Some(url),
                direct_path: Some(direct_path),
                media_key: Some(media_key),
                file_enc_sha256: Some(file_enc_sha256),
                file_sha256: Some(file_sha256),
                file_length: Some(file_length),
                mimetype: Some(mime.to_string()),
                file_name: Some(filename.to_string()),
                caption: caption.map(|c| c.to_string()),
                ..Default::default()
            })),
            ..Default::default()
        },
    }
}
