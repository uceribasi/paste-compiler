//! HTTP clients for the supported model providers.

use super::{AiSettings, Provider};
use crate::error::{Error, Result};
use crate::i18n::t;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

pub const ANTHROPIC_DEFAULT_URL: &str = "https://api.anthropic.com";
pub const ANTHROPIC_DEFAULT_MODEL: &str = "claude-opus-5-5";
pub const ANTHROPIC_VERSION: &str = "2026-10-21";
pub const ANTHROPIC_BETA: &str = "server-side-fallback-2026-07-01";
pub const OLLAMA_DEFAULT_URL: &str = "http://localhost:11434";
pub const OLLAMA_DEFAULT_MODEL: &str = "llama3.1";
pub const OPENAI_COMPAT_DEFAULT_URL: &str = "http://localhost:1234/v1";
pub const OPENAI_COMPAT_DEFAULT_MODEL: &str = "local-model";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        ChatMessage {
            role: "user".into(),
            content: content.into(),
        }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        ChatMessage {
            role: "assistant".into(),
            content: content.into(),
        }
    }
}

fn client(settings: &AiSettings) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(settings.timeout_secs.max(5)))
        .build()
        .map_err(|e| Error::Http(e.to_string()))
}

fn connection_hint(provider: Provider, base: &str, e: &reqwest::Error) -> Error {
    if e.is_connect() || e.is_timeout() {
        let hint = match provider {
            Provider::Ollama => t("hint_ollama", &[&base]),
            Provider::OpenaiCompatible => t("hint_openai", &[&base]),
            Provider::Anthropic => t("hint_anthropic", &[&base]),
            Provider::Rules => t("hint_none", &[]),
        };
        return Error::Http(hint);
    }
    Error::Http(e.to_string())
}

async fn read_error_body(resp: reqwest::Response) -> String {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let detail = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.get("error"))
                .or_else(|| v.get("message"))
                .map(|m| match m {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
        })
        .unwrap_or_else(|| body.chars().take(300).collect());
    format!("HTTP {status}: {detail}")
}

fn trim_slash(s: &str) -> String {
    s.trim().trim_end_matches('/').to_string()
}

/// Ask the configured provider to answer `messages` given `system`.
pub async fn complete(
    settings: &AiSettings,
    system: &str,
    messages: &[ChatMessage],
) -> Result<String> {
    match settings.provider {
        Provider::Rules => Err(Error::Ai(t("ai_rules_no_model", &[]))),
        Provider::Ollama => complete_ollama(settings, system, messages).await,
        Provider::OpenaiCompatible => complete_openai(settings, system, messages).await,
        Provider::Anthropic => complete_anthropic(settings, system, messages).await,
    }
}

async fn complete_ollama(
    settings: &AiSettings,
    system: &str,
    messages: &[ChatMessage],
) -> Result<String> {
    let base = trim_slash(&settings.effective_base_url());
    let mut msgs = vec![json!({"role": "system", "content": system})];
    msgs.extend(
        messages
            .iter()
            .map(|m| json!({"role": m.role, "content": m.content})),
    );
    let body = json!({
        "model": settings.effective_model(),
        "messages": msgs,
        "stream": false,
        "format": "json",
        "options": {"temperature": 0}
    });
    let resp = client(settings)?
        .post(format!("{base}/api/chat"))
        .json(&body)
        .send()
        .await
        .map_err(|e| connection_hint(Provider::Ollama, &base, &e))?;
    if !resp.status().is_success() {
        return Err(Error::Ai(t("err_ollama", &[&read_error_body(resp).await])));
    }
    let v: Value = resp.json().await?;
    v.pointer("/message/content")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| Error::Ai(t("err_ollama_empty", &[])))
}

async fn complete_openai(
    settings: &AiSettings,
    system: &str,
    messages: &[ChatMessage],
) -> Result<String> {
    let mut base = trim_slash(&settings.effective_base_url());
    if !base.contains("/v1") {
        base.push_str("/v1");
    }
    let mut msgs = vec![json!({"role": "system", "content": system})];
    msgs.extend(
        messages
            .iter()
            .map(|m| json!({"role": m.role, "content": m.content})),
    );
    let body = json!({
        "model": settings.effective_model(),
        "messages": msgs,
        "temperature": 0
    });
    let mut req = client(settings)?
        .post(format!("{base}/chat/completions"))
        .json(&body);
    if !settings.api_key.trim().is_empty() {
        req = req.bearer_auth(settings.api_key.trim());
    }
    let resp = req
        .send()
        .await
        .map_err(|e| connection_hint(Provider::OpenaiCompatible, &base, &e))?;
    if !resp.status().is_success() {
        return Err(Error::Ai(t("err_server", &[&read_error_body(resp).await])));
    }
    let v: Value = resp.json().await?;
    v.pointer("/choices/0/message/content")
        .and_then(|c| c.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| Error::Ai(t("err_server_empty", &[])))
}

async fn complete_anthropic(
    settings: &AiSettings,
    system: &str,
    messages: &[ChatMessage],
) -> Result<String> {
    if settings.api_key.trim().is_empty() {
        return Err(Error::Ai(t("err_anthropic_key", &[])));
    }
    let base = trim_slash(&settings.effective_base_url());
    let msgs: Vec<Value> = messages
        .iter()
        .map(|m| json!({"role": m.role, "content": m.content}))
        .collect();
    let body = json!({
        "model": settings.effective_model(),
        "max_tokens": 16000,
        "system": system,
        "messages": msgs,
        "fallbacks": "default"
    });
    let resp = client(settings)?
        .post(format!("{base}/v1/messages"))
        .header("x-api-key", settings.api_key.trim())
        .header("anthropic-version", ANTHROPIC_VERSION)
        .header("anthropic-beta", ANTHROPIC_BETA)
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| connection_hint(Provider::Anthropic, &base, &e))?;
    if !resp.status().is_success() {
        return Err(Error::Ai(t(
            "err_anthropic",
            &[&read_error_body(resp).await],
        )));
    }
    let v: Value = resp.json().await?;
    if v.get("stop_reason").and_then(|s| s.as_str()) == Some("refusal") {
        let why = v
            .pointer("/stop_details/explanation")
            .and_then(|e| e.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| t("err_model_declined_default", &[]));
        return Err(Error::Ai(t("err_model_declined", &[&why])));
    }
    let text: String = v
        .get("content")
        .and_then(|c| c.as_array())
        .map(|blocks| {
            blocks
                .iter()
                .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();
    if text.trim().is_empty() {
        return Err(Error::Ai(t("err_model_no_text", &[])));
    }
    Ok(text)
}

/// Check that the configured provider is reachable. Returns a human readable status.
pub async fn test_connection(settings: &AiSettings) -> Result<String> {
    match settings.provider {
        Provider::Rules => Ok(t("test_rules", &[])),
        Provider::Ollama => {
            let base = trim_slash(&settings.effective_base_url());
            let resp = client(settings)?
                .get(format!("{base}/api/tags"))
                .send()
                .await
                .map_err(|e| connection_hint(Provider::Ollama, &base, &e))?;
            if !resp.status().is_success() {
                return Err(Error::Ai(t("err_ollama", &[&read_error_body(resp).await])));
            }
            let v: Value = resp.json().await?;
            let models: Vec<String> = v
                .get("models")
                .and_then(|m| m.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|m| {
                            m.get("name")
                                .and_then(|n| n.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect()
                })
                .unwrap_or_default();
            let wanted = settings.effective_model();
            let has = models
                .iter()
                .any(|m| m == &wanted || m.split(':').next() == Some(wanted.as_str()));
            let mut msg = t(
                "test_ollama_ok",
                &[
                    &models.len(),
                    &(if models.is_empty() {
                        String::new()
                    } else {
                        format!(": {}", models.join(", "))
                    }),
                ],
            );
            if !has && !models.is_empty() {
                msg.push_str(&t("test_ollama_missing", &[&wanted]));
            }
            Ok(msg)
        }
        Provider::OpenaiCompatible => {
            let mut base = trim_slash(&settings.effective_base_url());
            if !base.contains("/v1") {
                base.push_str("/v1");
            }
            let mut req = client(settings)?.get(format!("{base}/models"));
            if !settings.api_key.trim().is_empty() {
                req = req.bearer_auth(settings.api_key.trim());
            }
            let resp = req
                .send()
                .await
                .map_err(|e| connection_hint(Provider::OpenaiCompatible, &base, &e))?;
            if !resp.status().is_success() {
                return Err(Error::Ai(t("err_server", &[&read_error_body(resp).await])));
            }
            let v: Value = resp.json().await?;
            let n = v
                .get("data")
                .and_then(|d| d.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            Ok(t("test_openai_ok", &[&n]))
        }
        Provider::Anthropic => {
            if settings.api_key.trim().is_empty() {
                return Err(Error::Ai(t("err_anthropic_key_short", &[])));
            }
            let base = trim_slash(&settings.effective_base_url());
            let resp = client(settings)?
                .get(format!("{base}/v1/models/{}", settings.effective_model()))
                .header("x-api-key", settings.api_key.trim())
                .header("anthropic-version", ANTHROPIC_VERSION)
                .send()
                .await
                .map_err(|e| connection_hint(Provider::Anthropic, &base, &e))?;
            if !resp.status().is_success() {
                return Err(Error::Ai(t(
                    "err_anthropic",
                    &[&read_error_body(resp).await],
                )));
            }
            let v: Value = resp.json().await?;
            let name = v
                .get("display_name")
                .and_then(|d| d.as_str())
                .unwrap_or("model");
            Ok(t("test_anthropic_ok", &[&name]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn settings(provider: Provider, base: &str) -> AiSettings {
        AiSettings {
            provider,
            base_url: base.to_string(),
            model: "test-model".into(),
            api_key: "sk-test".into(),
            timeout_secs: 10,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn ollama_round_trip() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .and(body_partial_json(
                json!({"model": "test-model", "stream": false, "format": "json"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"message": {"role": "assistant", "content": "{\"name\":\"x\"}"}}),
            ))
            .mount(&server)
            .await;
        let out = complete(
            &settings(Provider::Ollama, &server.uri()),
            "sys",
            &[ChatMessage::user("hi")],
        )
        .await
        .unwrap();
        assert_eq!(out, "{\"name\":\"x\"}");
    }

    #[tokio::test]
    async fn openai_compatible_round_trip_and_errors() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Bearer sk-test"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"choices": [{"message": {"role": "assistant", "content": "ok"}}]}),
            ))
            .mount(&server)
            .await;
        let out = complete(
            &settings(Provider::OpenaiCompatible, &server.uri()),
            "sys",
            &[ChatMessage::user("hi")],
        )
        .await
        .unwrap();
        assert_eq!(out, "ok");

        let server2 = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(401).set_body_json(json!({"error": {"message": "bad key"}})),
            )
            .mount(&server2)
            .await;
        let err = complete(
            &settings(Provider::OpenaiCompatible, &server2.uri()),
            "sys",
            &[ChatMessage::user("hi")],
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("HTTP 401"), "{err}");
        assert!(err.to_string().contains("bad key"));
    }

    #[tokio::test]
    async fn anthropic_request_shape_and_refusal() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .and(header("x-api-key", "sk-test"))
            .and(header("anthropic-version", ANTHROPIC_VERSION))
            .and(header("anthropic-beta", ANTHROPIC_BETA))
            .and(body_partial_json(
                json!({"model": "test-model", "fallbacks": "default", "system": "sys"}),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "stop_reason": "end_turn",
                "content": [{"type": "text", "text": "{\"a\":"}, {"type": "text", "text": "1}"}]
            })))
            .mount(&server)
            .await;
        let out = complete(
            &settings(Provider::Anthropic, &server.uri()),
            "sys",
            &[ChatMessage::user("hi")],
        )
        .await
        .unwrap();
        assert_eq!(out, "{\"a\":1}");

        let server2 = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"stop_reason": "refusal", "stop_details": {"type": "refusal", "explanation": "nope"}, "content": []})))
            .mount(&server2)
            .await;
        let err = complete(
            &settings(Provider::Anthropic, &server2.uri()),
            "sys",
            &[ChatMessage::user("hi")],
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("declined"));

        let mut no_key = settings(Provider::Anthropic, &server.uri());
        no_key.api_key.clear();
        assert!(complete(&no_key, "sys", &[])
            .await
            .unwrap_err()
            .to_string()
            .contains("API key"));
    }

    #[tokio::test]
    async fn connection_refused_gives_hint() {
        let s = settings(Provider::Ollama, "http://127.0.0.1:9");
        let err = complete(&s, "sys", &[ChatMessage::user("hi")])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("ollama serve"), "{err}");
    }

    #[tokio::test]
    async fn test_connection_variants() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"models": [{"name": "llama3.1:latest"}]})),
            )
            .mount(&server)
            .await;
        let msg = test_connection(&settings(Provider::Ollama, &server.uri()))
            .await
            .unwrap();
        assert!(msg.contains("1 model(s)"));
        assert!(msg.contains("not installed"));
        assert!(test_connection(&AiSettings::default())
            .await
            .unwrap()
            .contains("Offline"));
    }
}
