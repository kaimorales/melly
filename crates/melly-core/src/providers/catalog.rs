use crate::models::{Connection, ProviderSpec};
use anyhow::{Result, bail, ensure};
use sha2::{Digest, Sha256};
use url::Url;

pub fn catalog() -> Vec<ProviderSpec> {
    [
        (
            "mock",
            "Mock",
            "Mock",
            "mock://local",
            "melly-demo",
            "mock",
            false,
            true,
            None,
        ),
        (
            "chatgpt",
            "ChatGPT",
            "ChatGPT plan",
            "https://api.openai.com/v1",
            "gpt-6.1-sol",
            "responses",
            true,
            true,
            None,
        ),
        (
            "muse",
            "Meta Muse Code",
            "Plan · Unavailable",
            "",
            "",
            "disabled",
            false,
            false,
            Some("The official Muse host is not verified for chat-only use."),
        ),
        (
            "claude",
            "Claude",
            "API key · Anthropic",
            "https://api.anthropic.com/v1",
            "claude-sonnet-5-5",
            "messages",
            true,
            true,
            None,
        ),
        (
            "gemini",
            "Gemini",
            "API key · Google Gemini",
            "https://generativelanguage.googleapis.com/v1beta",
            "gemini-4-hydrogen",
            "interactions",
            true,
            true,
            None,
        ),
        (
            "grok",
            "Grok",
            "API key · xAI",
            "https://api.x.ai/v1",
            "grok-4.7",
            "responses",
            true,
            true,
            None,
        ),
        (
            "openrouter",
            "OpenRouter",
            "API key · OpenRouter",
            "https://openrouter.ai/api/v1",
            "openai/gpt-6.1-sol",
            "chat-completions",
            false,
            true,
            None,
        ),
        (
            "deepseek",
            "DeepSeek",
            "API key · DeepSeek",
            "https://api.deepseek.com",
            "deepseek-flash",
            "chat-completions",
            false,
            true,
            None,
        ),
        (
            "qwen-standard",
            "QwenCloud · Standard API",
            "API key · Standard API (Singapore)",
            "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
            "qwen3.8-flash",
            "chat-completions",
            false,
            true,
            None,
        ),
        (
            "qwen-token-plan",
            "QwenCloud · Token Plan",
            "Token Plan (Singapore)",
            "https://token-plan.ap-southeast-1.maas.aliyuncs.com/compatible-mode/v1",
            "auto",
            "chat-completions",
            false,
            true,
            None,
        ),
        (
            "moonshot",
            "Kimi / Moonshot",
            "API key · Moonshot",
            "https://api.moonshot.ai/v1",
            "kimi-k3.1-pro",
            "chat-completions",
            false,
            true,
            None,
        ),
        (
            "ollama",
            "Ollama Cloud",
            "API key · plan or usage credits",
            "https://ollama.com/v1",
            "gemma4:31b",
            "chat-completions",
            false,
            true,
            None,
        ),
        (
            "custom",
            "Custom compatible",
            "API key · Selected endpoint (unverified)",
            "",
            "",
            "chat-completions",
            false,
            false,
            None,
        ),
    ]
    .into_iter()
    .map(
        |(id, name, billing, base, model, protocol, search, tools, disabled)| ProviderSpec {
            id: id.into(),
            name: name.into(),
            billing_route: billing.into(),
            base_url: base.into(),
            default_model: model.into(),
            protocol: protocol.into(),
            native_search: search,
            tools,
            disabled_reason: disabled.map(str::to_owned),
        },
    )
    .collect()
}

pub(crate) fn spec(connection: &Connection) -> Result<ProviderSpec> {
    catalog()
        .into_iter()
        .find(|p| p.id == connection.provider_id)
        .ok_or_else(|| anyhow::anyhow!("Unknown provider"))
}

pub fn validate_connection(connection: &Connection) -> Result<()> {
    let provider = spec(connection)?;
    if provider.disabled_reason.is_some() {
        bail!("Muse is unavailable for chat-only use");
    }
    if provider.id == "mock" {
        ensure!(
            connection.base_url == provider.base_url,
            "Mock endpoint is fixed"
        );
        return Ok(());
    }
    let url = super::public_url(&connection.base_url)?;
    ensure!(
        url.query().is_none() && url.fragment().is_none(),
        "Provider base URL cannot contain a query or fragment"
    );
    if provider.id != "custom" {
        ensure!(
            normalized_base(&connection.base_url) == normalized_base(&provider.base_url),
            "{} endpoint is fixed; select its own billing connection",
            provider.name
        );
    }
    Ok(())
}

pub(crate) fn normalized_base(value: &str) -> String {
    Url::parse(value)
        .map(|u| u.to_string().trim_end_matches('/').to_owned())
        .unwrap_or_else(|_| value.to_owned())
}

pub fn credential_id(connection: &Connection) -> String {
    let mut digest = Sha256::new();
    digest.update(connection.provider_id.as_bytes());
    digest.update([0]);
    digest.update(normalized_base(&connection.base_url).as_bytes());
    format!("melly.provider.{:x}", digest.finalize())
}

/// Conservative documented allowlist, not proof of account entitlement.
pub fn supports_native_search(connection: &Connection) -> bool {
    match connection.provider_id.as_str() {
        "chatgpt" => matches!(connection.model_id.as_str(), "gpt-6.1-sol"),
        "claude" => matches!(
            connection.model_id.as_str(),
            "claude-sonnet-5-5"
                | "claude-opus-5-5"
                | "claude-haiku-5-5"
                | "claude-sonnet-5"
                | "claude-opus-5"
                | "claude-sonnet-4-6"
                | "claude-opus-4-6"
                | "claude-haiku-4-5-20251001"
        ),
        "gemini" => matches!(connection.model_id.as_str(), "gemini-4-hydrogen"),
        "grok" => matches!(connection.model_id.as_str(), "grok-4.7"),
        _ => false,
    }
}
