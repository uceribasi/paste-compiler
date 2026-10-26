//! Natural-language recipe generation: local rules, Ollama, OpenAI-compatible
//! servers and the Anthropic API.

pub mod prompt;
pub mod providers;
pub mod rules;

use crate::error::{Error, Result};
use crate::i18n::{t, Lang};
use crate::parse::ParseOptions;
use crate::pipeline::{analyze, ConvertResult};
use crate::recipe::{extract_json_object, parse_recipe_lenient, Recipe};
use crate::validate::ValidationReport;
use prompt::{build_repair_prompt, build_user_prompt, system_prompt, SampleContext};
pub use providers::{test_connection, ChatMessage};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    /// Offline, rule-based interpretation. No network access.
    #[default]
    Rules,
    /// Local models served by Ollama.
    Ollama,
    /// Anthropic Claude via the Messages API.
    Anthropic,
    /// Any OpenAI-compatible chat completions server (LM Studio, llama.cpp, vLLM, ...).
    OpenaiCompatible,
}

impl Provider {
    pub fn label(&self) -> &'static str {
        match self {
            Provider::Rules => "Offline rules (no AI)",
            Provider::Ollama => "Ollama (local)",
            Provider::Anthropic => "Anthropic Claude",
            Provider::OpenaiCompatible => "OpenAI-compatible server",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub provider: Provider,
    /// Base URL of the provider; empty means the provider default.
    pub base_url: String,
    /// Model name; empty means the provider default.
    pub model: String,
    pub api_key: String,
    /// Send a sample of the pasted data to the model (default true).
    pub include_sample: bool,
    /// How many lines of the sample to send.
    pub sample_lines: usize,
    pub timeout_secs: u64,
    /// Fall back to the offline rules when the model fails.
    pub fallback_to_rules: bool,
}

impl Default for AiSettings {
    fn default() -> Self {
        AiSettings {
            provider: Provider::Rules,
            base_url: String::new(),
            model: String::new(),
            api_key: String::new(),
            include_sample: true,
            sample_lines: 20,
            timeout_secs: 120,
            fallback_to_rules: true,
        }
    }
}

impl AiSettings {
    pub fn effective_base_url(&self) -> String {
        if !self.base_url.trim().is_empty() {
            return self.base_url.trim().to_string();
        }
        match self.provider {
            Provider::Rules => String::new(),
            Provider::Ollama => providers::OLLAMA_DEFAULT_URL.to_string(),
            Provider::Anthropic => providers::ANTHROPIC_DEFAULT_URL.to_string(),
            Provider::OpenaiCompatible => providers::OPENAI_COMPAT_DEFAULT_URL.to_string(),
        }
    }

    pub fn effective_model(&self) -> String {
        if !self.model.trim().is_empty() {
            return self.model.trim().to_string();
        }
        match self.provider {
            Provider::Rules => String::new(),
            Provider::Ollama => providers::OLLAMA_DEFAULT_MODEL.to_string(),
            Provider::Anthropic => providers::ANTHROPIC_DEFAULT_MODEL.to_string(),
            Provider::OpenaiCompatible => providers::OPENAI_COMPAT_DEFAULT_MODEL.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratedRecipe {
    pub recipe: Recipe,
    pub explanation: String,
    pub provider: Provider,
    pub model: String,
    pub attempts: u32,
    /// True when the model failed and the offline rules produced the recipe.
    pub used_fallback: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
    /// Output of running the recipe on the sample, when a sample was given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_report: Option<ValidationReport>,
}

/// Describe the pasted data for the prompt.
pub fn sample_context(sample: &str, include_sample: bool) -> SampleContext {
    let mut ctx = SampleContext {
        sample: if include_sample {
            sample.to_string()
        } else {
            String::new()
        },
        ..Default::default()
    };
    if sample.trim().is_empty() {
        return ctx;
    }
    if let Ok(a) = analyze(sample, None, &ParseOptions::default()) {
        ctx.format = Some(a.detection.format);
        ctx.delimiter = a.detection.delimiter;
        ctx.summary = Some(a.summary);
    }
    ctx
}

/// Turn a raw model reply into a recipe plus the explanation it carried.
pub fn interpret_response(raw: &str, instruction: &str) -> Result<(Recipe, String)> {
    let json_text = extract_json_object(raw).ok_or_else(|| Error::Ai(t("ai_no_json", &[])))?;
    let mut value: Value =
        serde_json::from_str(json_text).map_err(|e| Error::Ai(t("ai_invalid_json", &[&e])))?;
    let explanation = value
        .as_object_mut()
        .and_then(|o| o.remove("explanation"))
        .map(|e| match e {
            Value::String(s) => s,
            other => other.to_string(),
        })
        .unwrap_or_default();
    let mut recipe = parse_recipe_lenient(&value.to_string())?;
    let problems = recipe.check();
    if !problems.is_empty() {
        return Err(Error::Recipe(problems.join("; ")));
    }
    recipe.prompt = Some(instruction.trim().to_string());
    if recipe.name.trim().is_empty() || recipe.name == "Untitled recipe" {
        recipe.name = rules_name(instruction);
    }
    Ok((recipe, explanation))
}

fn rules_name(instruction: &str) -> String {
    let mut name: String = instruction.trim().chars().take(60).collect();
    if instruction.trim().chars().count() > 60 {
        name.push('…');
    }
    let mut chars = name.chars();
    match chars.next() {
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Untitled recipe".into(),
    }
}

fn test_run(recipe: &Recipe, sample: &str) -> Result<Option<ConvertResult>> {
    if sample.trim().is_empty() {
        return Ok(None);
    }
    recipe.run(sample).map(Some)
}

fn rules_result(
    instruction: &str,
    ctx: &SampleContext,
    sample: &str,
    settings: &AiSettings,
    attempts: u32,
    reason: Option<String>,
) -> Result<GeneratedRecipe> {
    let r = rules::generate(instruction, ctx)?;
    let test = test_run(&r.recipe, sample)?;
    Ok(GeneratedRecipe {
        recipe: r.recipe,
        explanation: r.explanation,
        provider: Provider::Rules,
        model: String::new(),
        attempts,
        used_fallback: settings.provider != Provider::Rules,
        fallback_reason: reason,
        test_output: test.as_ref().map(|t| t.output.clone()),
        test_report: test.map(|t| t.report),
    })
}

/// Generate a recipe from a natural-language instruction.
///
/// The recipe is test-run on `sample`; when it fails the model gets one or two
/// chances to repair it, and finally the offline rules are used when enabled.
pub async fn generate_recipe(
    settings: &AiSettings,
    instruction: &str,
    sample: &str,
    existing: Option<&Recipe>,
) -> Result<GeneratedRecipe> {
    let lang = Lang::current();
    if instruction.trim().is_empty() {
        return Err(Error::Ai(t("ai_describe_first", &[])));
    }
    let ctx = sample_context(sample, settings.include_sample);
    if settings.provider == Provider::Rules {
        return rules_result(instruction, &ctx, sample, settings, 0, None);
    }

    let mut messages = vec![ChatMessage::user(build_user_prompt(
        instruction,
        &ctx,
        existing,
        settings.sample_lines.max(1),
    ))];
    let mut attempts = 0u32;
    let mut last_error = String::new();
    const MAX_ATTEMPTS: u32 = 3;

    while attempts < MAX_ATTEMPTS {
        attempts += 1;
        let raw = match providers::complete(settings, &system_prompt(lang), &messages).await {
            Ok(r) => r,
            Err(e) => {
                // Network / auth problems: fall back right away, keep the reason.
                Lang::set(lang);
                if settings.fallback_to_rules {
                    return rules_result(
                        instruction,
                        &ctx,
                        sample,
                        settings,
                        attempts,
                        Some(e.to_string()),
                    )
                    .map_err(|rules_err| Error::Ai(t("ai_fallback_failed", &[&e, &rules_err])));
                }
                return Err(e);
            }
        };
        Lang::set(lang);
        match interpret_response(&raw, instruction).and_then(|(recipe, explanation)| {
            test_run(&recipe, sample).map(|t| (recipe, explanation, t))
        }) {
            Ok((recipe, explanation, test)) => {
                return Ok(GeneratedRecipe {
                    recipe,
                    explanation,
                    provider: settings.provider,
                    model: settings.effective_model(),
                    attempts,
                    used_fallback: false,
                    fallback_reason: None,
                    test_output: test.as_ref().map(|t| t.output.clone()),
                    test_report: test.map(|t| t.report),
                });
            }
            Err(e) => {
                last_error = e.to_string();
                messages.push(ChatMessage::assistant(raw));
                messages.push(ChatMessage::user(build_repair_prompt(&last_error)));
            }
        }
    }

    Lang::set(lang);
    let reason = t("ai_gave_up", &[&attempts, &last_error]);
    if settings.fallback_to_rules {
        return rules_result(
            instruction,
            &ctx,
            sample,
            settings,
            attempts,
            Some(reason.clone()),
        )
        .map_err(|rules_err| Error::Ai(t("ai_fallback_failed", &[&reason, &rules_err])));
    }
    Err(Error::Ai(reason))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OutputFormat;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SAMPLE: &str = "name,age\nAda,36\nLinus,54";

    fn ollama(base: &str) -> AiSettings {
        AiSettings {
            provider: Provider::Ollama,
            base_url: base.into(),
            model: "m".into(),
            timeout_secs: 10,
            ..Default::default()
        }
    }

    fn ollama_reply(content: &str) -> ResponseTemplate {
        ResponseTemplate::new(200)
            .set_body_json(json!({"message": {"role": "assistant", "content": content}}))
    }

    #[test]
    fn interpret_strips_explanation_and_names_recipe() {
        let raw = "Sure!\n```json\n{\"explanation\": \"keeps adults\", \"steps\": [{\"op\": \"filter_rows\", \"column\": \"age\", \"condition\": \">\", \"value\": 18}], \"output\": \"csv\"}\n```";
        let (recipe, explanation) = interpret_response(raw, "keep adults").unwrap();
        assert_eq!(explanation, "keeps adults");
        assert_eq!(recipe.name, "Keep adults");
        assert_eq!(recipe.output.format, OutputFormat::Csv);
        assert_eq!(recipe.prompt.as_deref(), Some("keep adults"));
        assert!(interpret_response("no json", "x").is_err());
    }

    #[test]
    fn sample_context_describes_data() {
        let ctx = sample_context(SAMPLE, true);
        assert_eq!(ctx.format, Some(crate::model::InputFormat::Csv));
        assert_eq!(ctx.summary.unwrap().headers.unwrap(), vec!["name", "age"]);
        assert_eq!(ctx.sample, SAMPLE);
        assert!(sample_context(SAMPLE, false).sample.is_empty());
    }

    #[tokio::test]
    async fn rules_provider_needs_no_network() {
        let g = generate_recipe(
            &AiSettings::default(),
            "sort by age desc as json",
            SAMPLE,
            None,
        )
        .await
        .unwrap();
        assert_eq!(g.provider, Provider::Rules);
        assert!(!g.used_fallback);
        assert!(g.test_output.unwrap().contains("Linus"));
        assert!(g.test_report.unwrap().lossless.is_some());
    }

    #[tokio::test]
    async fn model_reply_is_tested_and_returned() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ollama_reply(r#"{"name":"Adults","explanation":"filters","steps":[{"op":"filter_rows","column":"age","condition":"gt","value":"40"}],"output":{"format":"json_compact"}}"#))
            .mount(&server)
            .await;
        let g = generate_recipe(&ollama(&server.uri()), "adults only", SAMPLE, None)
            .await
            .unwrap();
        assert_eq!(g.provider, Provider::Ollama);
        assert_eq!(g.attempts, 1);
        assert_eq!(g.recipe.name, "Adults");
        assert_eq!(g.test_output.unwrap(), r#"[{"name":"Linus","age":54}]"#);
    }

    #[tokio::test]
    async fn broken_reply_is_repaired_on_second_attempt() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ollama_reply(
                r#"{"steps":[{"op":"select_columns","columns":["nope"]}],"output":"csv"}"#,
            ))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ollama_reply(
                r#"{"steps":[{"op":"select_columns","columns":["name"]}],"output":"csv"}"#,
            ))
            .mount(&server)
            .await;
        let g = generate_recipe(&ollama(&server.uri()), "just names", SAMPLE, None)
            .await
            .unwrap();
        assert_eq!(g.attempts, 2);
        assert!(!g.used_fallback);
        assert_eq!(g.test_output.unwrap(), "name\nAda\nLinus");
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        let second: Value = serde_json::from_slice(&requests[1].body).unwrap();
        let msgs = second["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 4, "system + user + assistant + repair");
        assert!(msgs[3]["content"]
            .as_str()
            .unwrap()
            .contains("did not work"));
    }

    #[tokio::test]
    async fn falls_back_to_rules_when_model_unreachable() {
        let settings = ollama("http://127.0.0.1:9");
        let g = generate_recipe(&settings, "convert to yaml", SAMPLE, None)
            .await
            .unwrap();
        assert!(g.used_fallback);
        assert_eq!(g.provider, Provider::Rules);
        assert!(g.fallback_reason.unwrap().contains("Ollama"));
        assert_eq!(g.recipe.output.format, OutputFormat::Yaml);

        let strict = AiSettings {
            fallback_to_rules: false,
            ..settings
        };
        assert!(generate_recipe(&strict, "convert to yaml", SAMPLE, None)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn gives_up_after_three_attempts() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ollama_reply("garbage"))
            .mount(&server)
            .await;
        let settings = AiSettings {
            fallback_to_rules: false,
            ..ollama(&server.uri())
        };
        let err = generate_recipe(&settings, "whatever", SAMPLE, None)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("3 attempts"));
        assert_eq!(server.received_requests().await.unwrap().len(), 3);
    }
}
