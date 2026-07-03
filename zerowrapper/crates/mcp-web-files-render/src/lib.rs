use std::sync::Arc;
use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use zw_cache::CacheManager;
use zw_core::{McpToolProvider, ToolOutput};

#[derive(Serialize)]
pub struct RenderedFile {
    pub url: String,
    pub mime_type: String,
    pub file_size: i64,
    pub original_path: String,
    pub cached: bool,
}

pub struct WebFilesRenderProvider {
    cache: Arc<CacheManager>,
}

impl WebFilesRenderProvider {
    pub fn new(cache: Arc<CacheManager>) -> Self {
        Self { cache }
    }
}

#[async_trait]
impl McpToolProvider for WebFilesRenderProvider {
    fn name(&self) -> &str {
        "web_files_render"
    }

    fn description(&self) -> &str {
        "Render one or more local files (images, videos, PDFs, etc.) into HTTP-accessible URLs so the frontend can display them inline"
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Array of absolute paths to files to render"
                }
            },
            "required": ["files"]
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<ToolOutput> {
        let paths = args.get("files")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("Missing required argument: files"))?;

        let mut rendered = Vec::new();

        for file_val in paths {
            let file_path = file_val
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Each file entry must be a string"))?;

            let path = std::path::Path::new(file_path);
            if !path.exists() {
                rendered.push(RenderedFile {
                    url: String::new(),
                    mime_type: String::new(),
                    file_size: 0,
                    original_path: file_path.to_string(),
                    cached: false,
                });
                continue;
            }

            let cached = self.cache.resolve(file_path)?;

            rendered.push(RenderedFile {
                url: format!("/files/{}", cached.hash),
                mime_type: cached.meta.mime_type,
                file_size: cached.meta.file_size,
                original_path: file_path.to_string(),
                cached: true,
            });
        }

        Ok(ToolOutput::new(serde_json::json!({
            "files": rendered
        }).to_string()))
    }
}
