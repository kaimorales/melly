use async_trait::async_trait;
use melly_core::{
    coding::{Call, CodingTools},
    credentials::MemorySecrets,
    engine::{Engine, Transport},
    models::{ProviderEvent, ProviderRequest, ProviderSink, SendInput, Source},
    storage::Store,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Model(Mutex<usize>);

#[async_trait]
impl Transport for Model {
    async fn stream(
        &self,
        request: ProviderRequest,
        _: Option<String>,
        sink: ProviderSink,
        _: CancellationToken,
    ) -> anyhow::Result<()> {
        assert!(request.system_prompt.contains("melly-tool"));
        let mut round = self.0.lock().unwrap();
        match *round {
            0 => {
                sink(ProviderEvent::Delta(
                    "I'll inspect the tests.\n\n```mell".into(),
                ));
                sink(ProviderEvent::Delta(
                    "y-tool\n{\"tool\":\"search_repo\",\"query\":\"test\"}\n```".into(),
                ));
            }
            1 => {
                assert!(
                    request
                        .messages
                        .last()
                        .unwrap()
                        .content
                        .contains("test_example.py")
                );
                sink(ProviderEvent::Delta("{\"tool\":\"run_tests\"}".into()));
            }
            _ => {
                assert!(
                    request
                        .messages
                        .last()
                        .unwrap()
                        .content
                        .contains("synthetic result")
                );
                sink(ProviderEvent::Delta(
                    "Tests passed.\n\n```text\nok\n```".into(),
                ));
            }
        }
        *round += 1;
        sink(ProviderEvent::Done);
        Ok(())
    }

    async fn web_search(
        &self,
        _: &str,
        _: &str,
        _: CancellationToken,
    ) -> anyhow::Result<Vec<Source>> {
        anyhow::bail!("Unexpected web search")
    }
}

struct Tools;

#[async_trait]
impl CodingTools for Tools {
    async fn execute(
        &self,
        _: &str,
        _: &str,
        call: Call,
        _: CancellationToken,
    ) -> anyhow::Result<Value> {
        Ok(match call {
            Call::SearchRepo { query } => {
                assert_eq!(query, "test");
                json!({"matches":[{"path":"test_example.py"}]})
            }
            Call::RunTests => json!({"passed":true,"output":"synthetic result"}),
            _ => anyhow::bail!("Unexpected call"),
        })
    }
}

#[cfg(unix)]
#[tokio::test]
async fn coding_rounds_hide_tool_requests_and_use_test_results() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let profile = tempfile::tempdir()?;
    let store = Store::open(&profile.path().join("fixture.sqlite3"))?;
    let folder = store.create_folder("Synthetic linked project")?;
    store.set_project_link(&folder.id, &root.path().canonicalize()?)?;
    let chat = store.create_chat(Some(&folder.id))?;
    let model = Arc::new(Model::default());
    let engine = Arc::new(Engine::with_transport(
        Arc::new(Mutex::new(store)),
        Arc::new(MemorySecrets::default()),
        model.clone(),
    ));
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let reply = engine.start_with_coding_tools(
        SendInput {
            chat_id: chat.id,
            provider_id: "mock".into(),
            model_id: "melly-demo".into(),
            content: "Run the project tests.".into(),
            web_search: false,
            folder_context: true,
            retry: false,
        },
        None,
        Arc::new(move |update| {
            let _ = tx.send(update);
        }),
        Some(Arc::new(Tools)),
    )?;
    let visible = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        let mut visible = String::new();
        while let Some(update) = rx.recv().await {
            if update.kind == "delta" {
                visible.push_str(&update.text);
            }
            if update.kind == "done" {
                return visible;
            }
            assert_ne!(update.kind, "error", "{}", update.text);
        }
        visible
    })
    .await?;
    assert_eq!(*model.0.lock().unwrap(), 3);
    assert!(visible.contains("Tests passed."));
    assert!(!visible.contains("melly-tool"));
    assert!(visible.ends_with("```"));
    assert_eq!(
        engine.store.lock().unwrap().message(&reply.id)?.content,
        visible
    );
    Ok(())
}
