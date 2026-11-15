//! Per-chat context preferences and the limit applied to the selected model.
use crate::{models::Connection, providers::compatible, storage::Store};
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub const MAX_TOKENS: usize = 100_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ContextMode {
    Auto,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ContextLimit {
    Tokens(usize),
    Mode(ContextMode),
}

impl ContextLimit {
    pub fn validate(self) -> Result<Self> {
        if let Self::Tokens(tokens) = self {
            ensure!(
                (1024..=MAX_TOKENS).contains(&tokens),
                "Enter a context limit of 1,024 to 100,000,000 tokens"
            );
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSelection {
    pub choice: Option<ContextLimit>,
    pub ceiling: Option<usize>,
    pub model_default: Option<usize>,
    pub effective: Option<usize>,
}

impl ContextSelection {
    pub(crate) fn transport_limit(self) -> Option<usize> {
        (self.choice.is_some() || self.model_default.is_some())
            .then_some(self.effective)
            .flatten()
    }
    pub fn new(
        choice: Option<ContextLimit>,
        model_default: Option<usize>,
        ceiling: Option<usize>,
    ) -> Self {
        let requested = match choice {
            Some(ContextLimit::Tokens(tokens)) => Some(tokens),
            Some(ContextLimit::Mode(ContextMode::Auto)) => None,
            None => model_default,
        };
        let effective = match (requested, ceiling) {
            (Some(choice), Some(maximum)) => Some(choice.min(maximum)),
            (choice, maximum) => choice.or(maximum),
        };
        Self {
            choice,
            ceiling,
            model_default,
            effective,
        }
    }
}

impl Store {
    pub fn set_chat_context_limit(&self, chat: &str, choice: Option<ContextLimit>) -> Result<()> {
        self.chat(chat)?;
        if let Some(choice) = choice {
            let database_value = match choice.validate()? {
                ContextLimit::Tokens(tokens) => tokens,
                ContextLimit::Mode(ContextMode::Auto) => 0,
            };
            self.conn.execute("INSERT INTO chat_context_limits(chat_id,tokens) VALUES(?1,?2) ON CONFLICT(chat_id) DO UPDATE SET tokens=excluded.tokens", params![chat,database_value])?;
        } else {
            self.conn
                .execute("DELETE FROM chat_context_limits WHERE chat_id=?1", [chat])?;
        }
        Ok(())
    }

    pub(crate) fn context_selection(
        &self,
        connection: &Connection,
        chat: Option<&str>,
        draft: Option<ContextLimit>,
        ceiling: Option<usize>,
    ) -> Result<ContextSelection> {
        let choice = if let Some(chat) = chat {
            let value: Option<usize> = self
                .conn
                .query_row(
                    "SELECT tokens FROM chat_context_limits WHERE chat_id=?1",
                    [chat],
                    |row| row.get(0),
                )
                .optional()?;
            value.map(|tokens| {
                if tokens == 0 {
                    ContextLimit::Mode(ContextMode::Auto)
                } else {
                    ContextLimit::Tokens(tokens)
                }
            })
        } else {
            draft.map(ContextLimit::validate).transpose()?
        };
        let model_default = if compatible::is_custom(&connection.provider_id) {
            self.compatible_connection(&connection.provider_id)?
                .and_then(|entry| {
                    entry
                        .models
                        .into_iter()
                        .find(|model| model.id == connection.model_id)
                        .and_then(|model| model.default_context_window)
                })
        } else {
            None
        };
        Ok(ContextSelection::new(choice, model_default, ceiling))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_uses_capacity_and_explicit_limits_are_clamped() {
        let selected = ContextSelection::new(None, Some(32768), Some(65536));
        assert_eq!(selected.effective, Some(32768));
        let automatic = ContextSelection::new(
            Some(ContextLimit::Mode(ContextMode::Auto)),
            Some(32768),
            Some(65536),
        );
        assert_eq!(automatic.effective, Some(65536));
        let explicit = ContextSelection::new(Some(ContextLimit::Tokens(65536)), None, Some(32768));
        assert_eq!(explicit.effective, Some(32768));
        assert_eq!(explicit.choice, Some(ContextLimit::Tokens(65536)));
    }

    #[test]
    fn changes_apply_to_one_chat_and_reset_to_the_model_default() {
        let store = Store::memory().unwrap();
        let first = store.create_chat(None).unwrap();
        let second = store.create_chat(None).unwrap();
        let connection = Connection {
            provider_id: "mock".into(),
            base_url: String::new(),
            model_id: "melly-demo".into(),
        };
        store
            .set_chat_context_limit(&first.id, Some(ContextLimit::Tokens(8192)))
            .unwrap();
        assert_eq!(
            store
                .context_selection(&connection, Some(&first.id), None, None)
                .unwrap()
                .effective,
            Some(8192)
        );
        assert_eq!(
            store
                .context_selection(&connection, Some(&second.id), None, None)
                .unwrap()
                .effective,
            None
        );
        store.set_chat_context_limit(&first.id, None).unwrap();
        assert_eq!(
            store
                .context_selection(&connection, Some(&first.id), None, None)
                .unwrap()
                .choice,
            None
        );
        assert!(
            store
                .set_chat_context_limit(&first.id, Some(ContextLimit::Tokens(100)))
                .is_err()
        );
    }
}
