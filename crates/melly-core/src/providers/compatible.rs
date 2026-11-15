//! User-defined Chat Completions connections. Routing stays in native code.
use crate::models::{Connection, ModelInfo, ProviderSpec};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use url::{Host, Url};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibleConnection {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub models: Vec<ModelInfo>,
    #[serde(default)]
    pub text_tool_calls: bool,
}

pub fn is_custom(id: &str) -> bool {
    id.strip_prefix("compatible:")
        .is_some_and(|value| uuid::Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value))
}

pub fn local(url: &Url) -> bool {
    match url.host() {
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    }
}

pub fn parse_base(value: &str) -> Result<Url> {
    ensure!(value.len() <= 1024, "Connection address is too long");
    let mut url =
        Url::parse(value.trim()).map_err(|_| anyhow::anyhow!("Enter a valid API base URL"))?;
    ensure!(
        url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none(),
        "Use a base URL without credentials, a query or a fragment"
    );
    if url.host_str() == Some("localhost") {
        url.set_host(Some("127.0.0.1"))
            .map_err(|_| anyhow::anyhow!("Invalid loopback address"))?;
    }
    if !(local(&url) && matches!(url.scheme(), "http" | "https")) {
        super::public_url(url.as_str())?;
    }
    Ok(url)
}

pub fn endpoint(base: &str, path: &str) -> Result<Url> {
    let base = parse_base(base)?;
    Ok(Url::parse(&format!(
        "{}/{path}",
        base.as_str().trim_end_matches('/')
    ))?)
}

pub fn client(url: &Url) -> Result<reqwest::Client> {
    if !local(url) {
        return super::network::client();
    }
    static LOCAL: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    if let Some(client) = LOCAL.get() {
        return Ok(client.clone());
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(5))
        .build()?;
    Ok(LOCAL.get_or_init(|| client).clone())
}

impl CompatibleConnection {
    pub fn spec(&self) -> ProviderSpec {
        let mut spec = wire_spec(&self.connection());
        spec.name = self.name.clone();
        spec.tools = !self.text_tool_calls;
        spec
    }

    pub fn connection(&self) -> Connection {
        Connection {
            provider_id: self.id.clone(),
            base_url: self.base_url.clone(),
            model_id: self
                .models
                .first()
                .map(|model| model.id.clone())
                .unwrap_or_default(),
        }
    }

    pub fn validate(&mut self) -> Result<()> {
        ensure!(is_custom(&self.id), "Invalid connection ID");
        self.name = self.name.trim().into();
        ensure!(
            !self.name.is_empty()
                && self.name.len() <= 100
                && !self.name.chars().any(char::is_control),
            "Enter a connection name"
        );
        self.base_url = parse_base(&self.base_url)?
            .as_str()
            .trim_end_matches('/')
            .to_owned();
        ensure!(
            self.models.len() <= 100,
            "A connection can have up to 100 saved models"
        );
        let mut seen = std::collections::HashSet::new();
        for model in &mut self.models {
            model.id = model.id.trim().to_owned();
            model.name = model.name.trim().to_owned();
            ensure!(
                !model.id.is_empty()
                    && model.id.len() <= 200
                    && !model.id.chars().any(char::is_control),
                "Enter a valid model ID"
            );
            ensure!(
                !model.name.is_empty()
                    && model.name.len() <= 200
                    && !model.name.chars().any(char::is_control),
                "Enter a display name for the model"
            );
            ensure!(
                seen.insert(model.id.clone()),
                "This model ID is already saved"
            );
            ensure!(
                model
                    .context_window
                    .is_none_or(|size| (1024..=100_000_000).contains(&size)),
                "Enter a context limit of 1,024 tokens or larger"
            );
        }
        for model in &self.models {
            if let Some(default) = model.default_context_window {
                crate::context::ContextLimit::Tokens(default).validate()?;
                ensure!(
                    model
                        .context_window
                        .is_none_or(|maximum| default <= maximum),
                    "Default context must fit the model's context limit"
                );
            }
        }
        Ok(())
    }
}

pub(super) fn wire_spec(connection: &Connection) -> ProviderSpec {
    ProviderSpec {
        id: connection.provider_id.clone(),
        name: "OpenAI-compatible".into(),
        billing_route: if parse_base(&connection.base_url).is_ok_and(|url| local(&url)) {
            "On this Mac"
        } else {
            "Remote API"
        }
        .into(),
        base_url: connection.base_url.clone(),
        default_model: connection.model_id.clone(),
        protocol: "chat-completions".into(),
        native_search: false,
        tools: true,
        disabled_reason: None,
    }
}
