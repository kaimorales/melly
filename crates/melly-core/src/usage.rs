//! Content-free accounting. Unknown usage is never treated as zero cost.
use crate::{
    models::{Connection, Source},
    storage::Store,
};
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub cached_input: Option<u64>,
    pub reasoning: Option<u64>,
    pub cost_usd: Option<f64>,
}
impl Usage {
    pub fn merge(&mut self, next: Self) {
        self.input = self.input.max(next.input);
        self.output = self.output.max(next.output);
        self.cached_input = self.cached_input.max(next.cached_input);
        self.reasoning = self.reasoning.max(next.reasoning);
        if next.cost_usd.is_some() {
            self.cost_usd = next.cost_usd;
        }
    }
    pub fn from_value(value: &Value) -> Option<Self> {
        let count = |names: &[&str]| {
            names
                .iter()
                .find_map(|name| value[*name].as_u64())
                .filter(|n| *n <= 100_000_000)
        };
        let mut input = count(&[
            "input_tokens",
            "prompt_tokens",
            "total_input_tokens",
            "promptTokenCount",
            "prompt_eval_count",
        ]);
        if value.get("cache_creation_input_tokens").is_some()
            || value.get("cache_read_input_tokens").is_some()
        {
            input = input.map(|n| {
                n.saturating_add(count(&["cache_creation_input_tokens"]).unwrap_or(0))
                    .saturating_add(count(&["cache_read_input_tokens"]).unwrap_or(0))
            });
        }
        let usage = Self {
            input,
            output: count(&[
                "output_tokens",
                "completion_tokens",
                "total_output_tokens",
                "candidatesTokenCount",
                "eval_count",
            ]),
            cached_input: count(&[
                "cache_read_input_tokens",
                "cachedContentTokenCount",
                "prompt_eval_cached_count",
            ])
            .or_else(|| value["prompt_tokens_details"]["cached_tokens"].as_u64())
            .or_else(|| value["input_tokens_details"]["cached_tokens"].as_u64()),
            reasoning: count(&["thoughtsTokenCount"])
                .or_else(|| value["completion_tokens_details"]["reasoning_tokens"].as_u64())
                .or_else(|| value["output_tokens_details"]["reasoning_tokens"].as_u64()),
            cost_usd: value["cost"]
                .as_f64()
                .filter(|v| v.is_finite() && *v >= 0. && *v <= 100000.),
        };
        (usage.input.is_some() || usage.output.is_some() || usage.cost_usd.is_some())
            .then_some(usage)
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rates {
    pub input_per_million: f64,
    pub output_per_million: f64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub input: u64,
    pub output: u64,
    pub requests: u64,
    pub missing_usage: u64,
    pub reported_cost: f64,
    pub estimated_cost: f64,
    pub unpriced_requests: u64,
    pub reported_requests: u64,
    pub estimated_requests: u64,
    pub rates: Option<Rates>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextPreview {
    pub selection: crate::context::ContextSelection,
    pub estimated_tokens: usize,
    pub capacity: Option<usize>,
    pub included_messages: usize,
    pub omitted_messages: usize,
    pub sources: Vec<Source>,
    pub reported_input: u64,
    pub reported_output: u64,
    pub note: String,
    pub warning: Option<String>,
}
impl Store {
    pub(crate) fn initialize_usage(&self) -> Result<()> {
        self.conn.execute_batch("CREATE TABLE IF NOT EXISTS usage_calls(id TEXT PRIMARY KEY,scope TEXT NOT NULL,model TEXT NOT NULL,chat_id TEXT NOT NULL,created_at INTEGER NOT NULL,input INTEGER,output INTEGER,cached_input INTEGER,reasoning INTEGER,cost REAL,estimate REAL);
            CREATE INDEX IF NOT EXISTS usage_scope ON usage_calls(scope,created_at);
            CREATE INDEX IF NOT EXISTS usage_chat ON usage_calls(chat_id);
            CREATE TABLE IF NOT EXISTS usage_rates(scope TEXT NOT NULL,model TEXT NOT NULL,input REAL NOT NULL,output REAL NOT NULL,PRIMARY KEY(scope,model));
            CREATE TABLE IF NOT EXISTS model_limits(scope TEXT NOT NULL,model TEXT NOT NULL,capacity INTEGER NOT NULL,PRIMARY KEY(scope,model));")?;
        Ok(())
    }
    pub fn save_rates(&self, c: &Connection, rates: Rates) -> Result<()> {
        ensure!(
            !c.model_id.trim().is_empty() && c.model_id.len() <= 256,
            "Choose a valid model for these rates"
        );
        ensure!(
            rates.input_per_million.is_finite()
                && rates.output_per_million.is_finite()
                && (0.0..=10000.0).contains(&rates.input_per_million)
                && (0.0..=10000.0).contains(&rates.output_per_million),
            "Enter valid USD rates per million tokens"
        );
        self.conn.execute("INSERT INTO usage_rates VALUES(?1,?2,?3,?4) ON CONFLICT(scope,model) DO UPDATE SET input=excluded.input,output=excluded.output",params![crate::providers::credential_id(c),c.model_id,rates.input_per_million,rates.output_per_million])?;
        Ok(())
    }
    fn rates(&self, c: &Connection) -> Result<Option<Rates>> {
        Ok(self
            .conn
            .query_row(
                "SELECT input,output FROM usage_rates WHERE scope=?1 AND model=?2",
                params![crate::providers::credential_id(c), c.model_id],
                |r| {
                    Ok(Rates {
                        input_per_million: r.get(0)?,
                        output_per_million: r.get(1)?,
                    })
                },
            )
            .optional()?)
    }
    pub fn record_usage(&self, id: &str, c: &Connection, chat: &str, usage: &Usage) -> Result<()> {
        let estimate = if usage.cost_usd.is_none() {
            self.rates(c)?.and_then(|r| {
                Some(
                    (usage.input? as f64 * r.input_per_million
                        + usage.output? as f64 * r.output_per_million)
                        / 1_000_000.,
                )
            })
        } else {
            None
        };
        self.conn.execute("INSERT INTO usage_calls VALUES(?1,?2,?3,?4,unixepoch('now')*1000,?5,?6,?7,?8,?9,?10) ON CONFLICT(id) DO UPDATE SET input=coalesce(excluded.input,usage_calls.input),output=coalesce(excluded.output,usage_calls.output),cached_input=coalesce(excluded.cached_input,usage_calls.cached_input),reasoning=coalesce(excluded.reasoning,usage_calls.reasoning),cost=coalesce(excluded.cost,usage_calls.cost),estimate=CASE WHEN coalesce(excluded.cost,usage_calls.cost) IS NOT NULL THEN NULL ELSE coalesce(excluded.estimate,usage_calls.estimate) END WHERE usage_calls.scope=excluded.scope AND usage_calls.model=excluded.model AND usage_calls.chat_id=excluded.chat_id",params![id,crate::providers::credential_id(c),c.model_id,chat,usage.input,usage.output,usage.cached_input,usage.reasoning,usage.cost_usd,estimate])?;
        Ok(())
    }
    pub fn usage_summary(&self, c: &Connection) -> Result<UsageSummary> {
        let mut result=self.conn.query_row("SELECT coalesce(sum(input),0),coalesce(sum(output),0),count(*),coalesce(sum(input IS NULL OR output IS NULL),0),coalesce(sum(cost),0),coalesce(sum(estimate),0),coalesce(sum(cost IS NULL AND estimate IS NULL),0),coalesce(sum(cost IS NOT NULL),0),coalesce(sum(estimate IS NOT NULL),0) FROM usage_calls WHERE scope=?1 AND created_at>=unixepoch('now','start of month')*1000",[crate::providers::credential_id(c)],|r|Ok(UsageSummary{input:r.get(0)?,output:r.get(1)?,requests:r.get(2)?,missing_usage:r.get(3)?,reported_cost:r.get(4)?,estimated_cost:r.get(5)?,unpriced_requests:r.get(6)?,reported_requests:r.get(7)?,estimated_requests:r.get(8)?,rates:None}))?;
        result.rates = self.rates(c)?;
        Ok(result)
    }
    pub fn chat_usage(&self, chat: &str) -> Result<(u64, u64)> {
        Ok(self.conn.query_row("SELECT coalesce(sum(input),0),coalesce(sum(output),0) FROM usage_calls WHERE chat_id=?1",[chat],|r|Ok((r.get(0)?,r.get(1)?)))?)
    }
    pub fn save_model_limit(&self, c: &Connection, capacity: usize) -> Result<()> {
        ensure!(
            (1024..=100_000_000).contains(&capacity),
            "Invalid model capacity"
        );
        self.conn.execute("INSERT INTO model_limits VALUES(?1,?2,?3) ON CONFLICT(scope,model) DO UPDATE SET capacity=excluded.capacity",params![crate::providers::credential_id(c),c.model_id,capacity])?;
        Ok(())
    }
    pub fn model_limit(&self, c: &Connection) -> Result<Option<usize>> {
        if crate::providers::compatible::is_custom(&c.provider_id)
            && let Some(model) = self
                .compatible_connection(&c.provider_id)?
                .and_then(|item| item.models.into_iter().find(|m| m.id == c.model_id))
            && model.context_window.is_some()
        {
            return Ok(model.context_window);
        }
        Ok(self
            .conn
            .query_row(
                "SELECT capacity FROM model_limits WHERE scope=?1 AND model=?2",
                params![crate::providers::credential_id(c), c.model_id],
                |r| r.get(0),
            )
            .optional()?)
    }
}
