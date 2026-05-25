use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use super::types::{AiMode, AiSettings, AiSnapshot, AiSuggestion};

const BUILTIN_BASE_URL: &str = "https://csd-api.likeyou.qzz.io";
const DEFAULT_MODEL: &str = "deepseek-v4-pro";
const SYSTEM_PROMPT: &str =
    "You are a coding assistant analyzing a user's disk usage. Reply with JSON only.";
const USER_PROMPT_PREAMBLE: &str = "Below is a sanitized snapshot of a Windows user's disk. \
For each item or category that looks worth cleaning up or moving, propose ONE suggestion. \
Respond with JSON of the form: {\"suggestions\":[{\"id\":\"s1\",\"target_token\":\"...\",\
\"target_label\":\"...\",\"action\":\"migrate|delete|review\",\"reason\":\"...\",\
\"estimated_savings_mb\": 0}]} . Use the EXACT target_token strings from the snapshot. \
Do not invent paths. Do not include any path, username, or personally identifying info \
in the reason field.";

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
        .timeout(std::time::Duration::from_secs(60))
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
        .timeout(std::time::Duration::from_secs(60))
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
