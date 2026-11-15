use async_trait::async_trait;
use melly_core::{
    credentials::MemorySecrets,
    engine::{Engine, Transport},
    models::*,
    storage::Store,
};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;
struct Summarizer {
    started: tokio::sync::Notify,
    wait: bool,
}
#[async_trait]
impl Transport for Summarizer {
    async fn stream(
        &self,
        r: ProviderRequest,
        _: Option<String>,
        sink: ProviderSink,
        cancel: CancellationToken,
    ) -> anyhow::Result<()> {
        assert_eq!(r.connection.provider_id, "mock");
        assert!(r.tools.is_empty());
        assert!(!r.web_search);
        assert!(r.messages[0].content.contains("Constraint zero"));
        self.started.notify_one();
        if self.wait {
            cancel.cancelled().await;
            return Ok(());
        }
        sink(ProviderEvent::Delta("Goal: retain the synthetic task. User constraints: Constraint zero. Decisions: use the selected route. Completed: earlier notes. Pending: the next step.".into()));
        sink(ProviderEvent::Done);
        Ok(())
    }
    async fn web_search(
        &self,
        _: &str,
        _: &str,
        _: CancellationToken,
    ) -> anyhow::Result<Vec<Source>> {
        anyhow::bail!("Unexpected network search")
    }
}
fn fixture(wait: bool) -> anyhow::Result<(Arc<Engine>, String, Arc<Summarizer>)> {
    let store = Store::memory()?;
    let chat = store.create_chat(None)?;
    for i in 0..12 {
        store.insert_message(
            &chat.id,
            if i % 2 == 0 { "user" } else { "assistant" },
            &format!("Constraint zero; synthetic item {i}"),
            "complete",
            None,
            None,
            None,
        )?;
    }
    let model = Arc::new(Summarizer {
        started: tokio::sync::Notify::new(),
        wait,
    });
    let engine = Arc::new(Engine::with_transport(
        Arc::new(Mutex::new(store)),
        Arc::new(MemorySecrets::default()),
        model.clone(),
    ));
    Ok((engine, chat.id, model))
}
#[tokio::test]
async fn explicit_summary_keeps_originals_and_reduces_only_model_context() -> anyhow::Result<()> {
    let (engine, chat, _) = fixture(false)?;
    let summary = engine.compact_context(&chat, "mock", "melly-demo").await?;
    assert_eq!(summary.messages, 8);
    assert_eq!(engine.active_count(), 0);
    let store = engine.store.lock().unwrap();
    assert_eq!(store.messages(&chat, None)?.len(), 12);
    assert_eq!(store.context(&chat)?.len(), 4);
    assert!(store.summary_reference(&chat)?.contains("Constraint zero"));
    store.clear_context_summary(&chat)?;
    assert_eq!(store.context(&chat)?.len(), 12);
    Ok(())
}
#[tokio::test]
async fn stopping_summary_preserves_full_context_and_releases_chat() -> anyhow::Result<()> {
    let (engine, chat, model) = fixture(true)?;
    let task_engine = engine.clone();
    let task_chat = chat.clone();
    let task = tokio::spawn(async move {
        task_engine
            .compact_context(&task_chat, "mock", "melly-demo")
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), model.started.notified()).await?;
    assert!(engine.is_active(&chat));
    engine.stop(&chat);
    assert!(task.await?.is_err());
    assert_eq!(engine.active_count(), 0);
    let store = engine.store.lock().unwrap();
    assert!(store.context_summary(&chat)?.is_none());
    assert_eq!(store.context(&chat)?.len(), 12);
    Ok(())
}
