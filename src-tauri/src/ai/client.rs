use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use super::types::{AiMode, AiSettings, AiSnapshot, AiSuggestion};

const BUILTIN_BASE_URL: &str = "https://csd-api.likeyou.qzz.io";
const DEFAULT_MODEL: &str = "deepseek-v4-pro";
const SYSTEM_PROMPT: &str =
    "你是一个分析用户磁盘使用情况的助手。请仅用中文回复，输出格式为 JSON。\
请根据每个项目的 kind（类别）来判断安全性：\
- system_files: 系统文件，绝对不能删除。可以建议\"调整大小\"或\"禁用对应功能\"。\
- model_files: AI 模型文件，可以迁移到外置或辅助存储，但绝对不能删除。\
- temp_files: 临时文件，可以安全清理。\
- dev_tools: 开发构建产物，可以安全清理。\
- app_cache: 应用缓存，可安全清理。\
- game_files: 游戏文件，可迁移到辅助存储。\
- disk_images: 磁盘镜像/虚拟机，可迁移，不建议删除。\
- installer_files: 下载的安装包，可安全删除。\
- downloads: 下载目录内容，需用户判断是否还需要。\
- media_files: 媒体文件，可迁移或归档。\
- log_files: 日志文件，可以安全清理。\
- large_files / large_dirs: 通用大文件/目录，需要根据标签判断。\
输出 JSON 时，每个建议的 action 必须是以下之一：\
- migrate: 可以迁移到其他磁盘\
- delete: 可以安全删除\
- review: 需要用户手动判断能不能删";

const USER_PROMPT_PREAMBLE: &str = "以下是用户 Windows 磁盘的脱敏快照。\
每个项目包含 label（名称和大小）、kind（类别）、size_mb（大小）。\
请只对那些能明显判断安全性的项目提建议（如 temp_files、dev_tools、app_cache 可建议 delete；\
model_files、game_files、disk_images 可建议 migrate；system_files 可建议 review）。\
不确定的就跳过，不要猜测。\
以 JSON 格式回复：{\"suggestions\":[{\"id\":\"s1\",\"target_token\":\"...\",\
\"target_label\":\"...\",\"action\":\"migrate|delete|review\",\"reason\":\"...\",\
\"estimated_savings_mb\": 0}]} 。\
请使用快照中精确的 target_token 字符串。不要编造路径。\
所有回复内容必须使用中文。每个建议的 reason 要说明为什么安全或不安全。";

pub fn analyze(snapshot: &AiSnapshot, settings: &AiSettings) -> Result<Vec<AiSuggestion>> {
    let model = if settings.provider.model.trim().is_empty() {
        DEFAULT_MODEL.to_string()
    } else {
        settings.provider.model.trim().to_string()
    };

    let body = json!({
        "model": model,
        "temperature": 0.2,
        "response_format": { "type": "json_object" },
        "messages": [
            { "role": "system", "content": SYSTEM_PROMPT },
            {
                "role": "user",
                "content": format!(
                    "{}\n\n<snapshot>\n{}\n</snapshot>",
                    USER_PROMPT_PREAMBLE,
                    serde_json::to_string(snapshot)?
                )
            }
        ]
    });

    let raw_response = match settings.mode {
        AiMode::Builtin => call_builtin(&body)?,
        AiMode::Byok => call_byok(&body, settings)?,
    };

    parse_suggestions(&raw_response)
}

fn call_builtin(body: &Value) -> Result<String> {
    let url = format!("{}/v1/chat/completions", BUILTIN_BASE_URL);
    let resp = ureq::post(&url)
        .set("content-type", "application/json")
        .set("x-csd-builtin", "1")
        .timeout(std::time::Duration::from_secs(120))
        .send_string(&body.to_string());

    match resp {
        Ok(r) => r
            .into_string()
            .map_err(|e| anyhow!("read builtin response failed: {e}")),
        Err(ureq::Error::Status(code, r)) => {
            let body_text = r.into_string().unwrap_or_default();
            Err(anyhow!("builtin proxy returned {code}: {body_text}"))
        }
        Err(e) => Err(anyhow!("builtin proxy transport error: {e}")),
    }
}

fn call_byok(body: &Value, settings: &AiSettings) -> Result<String> {
    let base = settings.provider.base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(anyhow!("BYOK mode requires provider.base_url"));
    }
    let api_key = settings.provider.api_key.trim();
    if api_key.is_empty() {
        return Err(anyhow!("BYOK mode requires provider.api_key"));
    }

    let url = if base.ends_with("/v1") || base.ends_with("/openai") {
        format!("{}/chat/completions", base)
    } else {
        format!("{}/v1/chat/completions", base)
    };

    let resp = ureq::post(&url)
        .set("content-type", "application/json")
        .set("authorization", &format!("Bearer {}", api_key))
        .timeout(std::time::Duration::from_secs(120))
        .send_string(&body.to_string());

    match resp {
        Ok(r) => r
            .into_string()
            .map_err(|e| anyhow!("read byok response failed: {e}")),
        Err(ureq::Error::Status(code, r)) => {
            let body_text = r.into_string().unwrap_or_default();
            Err(anyhow!("BYOK upstream returned {code}: {body_text}"))
        }
        Err(e) => Err(anyhow!("BYOK transport error: {e}")),
    }
}

fn parse_suggestions(raw: &str) -> Result<Vec<AiSuggestion>> {
    let outer: Value =
        serde_json::from_str(raw).map_err(|e| anyhow!("response is not JSON: {e}"))?;
    let content = outer
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .ok_or_else(|| anyhow!("no choices[0].message.content in response"))?;

    let payload: Value = serde_json::from_str(content)
        .map_err(|e| anyhow!("model content was not JSON: {e}"))?;

    let array = payload
        .get("suggestions")
        .cloned()
        .or_else(|| {
            if payload.is_array() {
                Some(payload.clone())
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("response missing 'suggestions' array"))?;

    let mut out: Vec<AiSuggestion> = Vec::new();
    if let Some(items) = array.as_array() {
        for (idx, item) in items.iter().enumerate() {
            let id = item
                .get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("s{}", idx + 1));
            let target_token = item
                .get("target_token")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let target_label = item
                .get("target_label")
                .and_then(|v| v.as_str())
                .unwrap_or("(unknown)")
                .to_string();
            let action = item
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("review")
                .to_string();
            let reason = item
                .get("reason")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let estimated_savings_mb = item
                .get("estimated_savings_mb")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);

            if target_token.is_empty() {
                continue;
            }

            out.push(AiSuggestion {
                id,
                target_label,
                target_token,
                action,
                reason,
                estimated_savings_mb,
            });
        }
    }

    Ok(out)
}
