//! Opt-in, loopback-only verification on a disposable project; never production data.
use super::*;
use melly_core::{
    credentials::MemorySecrets,
    engine::{Engine, Transport},
    models::{ModelInfo, ProviderEvent, ProviderRequest, ProviderSink, SendInput, Source},
    providers::{self, compatible::CompatibleConnection},
    storage::Store,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

struct LiveTransport(Arc<AtomicUsize>);
#[async_trait::async_trait]
impl Transport for LiveTransport {
    async fn stream(
        &self,
        r: ProviderRequest,
        k: Option<String>,
        s: ProviderSink,
        c: CancellationToken,
    ) -> Result<()> {
        self.stream_with_context(r, k, s, c, Some(32768)).await
    }
    async fn stream_with_context(
        &self,
        r: ProviderRequest,
        k: Option<String>,
        s: ProviderSink,
        c: CancellationToken,
        context: Option<usize>,
    ) -> Result<()> {
        ensure!(
            providers::compatible::local(&providers::compatible::parse_base(
                &r.connection.base_url
            )?),
            "Live fixture only permits localhost"
        );
        let calls = self.0.clone();
        melly_core::engine::CloudTransport
            .stream_with_context(
                r,
                k,
                Arc::new(move |event| {
                    if matches!(event, ProviderEvent::ToolCall(_)) {
                        calls.fetch_add(1, Ordering::Relaxed);
                    }
                    s(event);
                }),
                c,
                context,
            )
            .await
    }
    async fn web_search(&self, _: &str, _: &str, _: CancellationToken) -> Result<Vec<Source>> {
        bail!("No web in local fixture")
    }
}
struct Fixture {
    link: ProjectLink,
    reads: Mutex<HashMap<String, String>>,
    checks: AtomicUsize,
}
#[async_trait::async_trait]
impl CodingTools for Fixture {
    async fn execute(
        &self,
        _: &str,
        _: &str,
        call: Call,
        cancel: CancellationToken,
    ) -> Result<Value> {
        match call {
            Call::SearchRepo {
                query,
                path,
                cursor,
            } => search(
                &self.link,
                &query,
                path.as_deref(),
                cursor.as_deref(),
                &cancel,
            ),
            Call::ReadFile {
                path,
                start_line,
                start_column,
            } => {
                let (result, revision) = read(
                    &self.link,
                    &path,
                    start_line.unwrap_or(1),
                    start_column.unwrap_or(1),
                )?;
                self.reads.lock().unwrap().insert(path, revision);
                Ok(result)
            }
            call => match prepare(&self.link, call)? {
                Prepared::Tests(suite) => {
                    self.checks.fetch_add(1, Ordering::Relaxed);
                    suite.run(&self.link.path, cancel).await
                }
                Prepared::Change {
                    path,
                    target,
                    updated: Some(text),
                    ..
                } if path == "calc.py" => {
                    ensure!(
                        self.reads.lock().unwrap().get(&path)
                            == Some(&linked::revision(
                                target.original().unwrap_or_default().as_bytes()
                            )),
                        "Read current file first"
                    );
                    target.write(&text)?;
                    Ok(json!({"ok":true,"path":path}))
                }
                _ => bail!("Fixture only permits editing calc.py"),
            },
        }
    }
}
#[tokio::test]
#[ignore = "Requires an explicitly configured local model server; synthetic data only"]
async fn local_model_repairs_and_checks_a_disposable_project_with_native_tools() -> Result<()> {
    let base = std::env::var("MELLY_LIVE_BASE_URL")?;
    ensure!(
        providers::compatible::local(&providers::compatible::parse_base(&base)?),
        "Use a localhost server"
    );
    let model = std::env::var("MELLY_LIVE_MODEL")?;
    let root = tempfile::tempdir()?;
    let path = root.path().canonicalize()?;
    std::fs::write(
        path.join("calc.py"),
        "def add(left, right):\n    return left - right\n",
    )?;
    std::fs::write(
        path.join("test_calc.py"),
        "import unittest\nfrom calc import add\nclass Tests(unittest.TestCase):\n    def test_add(self):\n        self.assertEqual(add(2, 3), 5)\n",
    )?;
    assert_eq!(
        Suite::detect(&path, None)?
            .run(&path, CancellationToken::new())
            .await?["passed"],
        false
    );
    let store = Store::memory()?;
    let folder = store.create_folder("Disposable synthetic fixture")?;
    store.set_project_link(&folder.id, &path)?;
    let link = store.project_link(&folder.id)?.unwrap();
    let chat = store.create_chat(Some(&folder.id))?;
    let provider = "compatible:00000000-0000-4000-8000-000000000001";
    store.save_compatible_connection(CompatibleConnection {
        id: provider.into(),
        name: "Local fixture".into(),
        base_url: base,
        text_tool_calls: false,
        models: vec![ModelInfo {
            id: model.clone(),
            name: model.clone(),
            context_window: Some(32768),
            default_context_window: Some(32768),
        }],
    })?;
    let native_calls = Arc::new(AtomicUsize::new(0));
    let engine = Arc::new(Engine::with_transport(
        Arc::new(Mutex::new(store)),
        Arc::new(MemorySecrets::default()),
        Arc::new(LiveTransport(native_calls.clone())),
    ));
    let fixture = Arc::new(Fixture {
        link,
        reads: Mutex::default(),
        checks: AtomicUsize::new(0),
    });
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let start = std::time::Instant::now();
    engine.start_with_coding_tools(SendInput{chat_id:chat.id,provider_id:provider.into(),model_id:model,content:"Read calc.py and test_calc.py with the project functions. Fix add so it returns the sum instead of subtracting. Run the project tests with run_tests and report the actual outcome.".into(),folder_context:true,web_search:false,retry:false},None,Arc::new(move|u|{let _=tx.send(u);}),Some(fixture.clone()))?;
    let done = tokio::time::timeout(Duration::from_secs(180), async {
        while let Some(u) = rx.recv().await {
            if matches!(u.kind.as_str(), "done" | "stopped" | "error") {
                return Ok(u);
            }
        }
        bail!("Stream ended")
    })
    .await;
    engine.stop_all();
    let done = done??;
    assert_eq!(done.kind, "done", "{}", done.text);
    assert!(
        native_calls.load(Ordering::Relaxed) >= 3,
        "Expected native tool calls, not textual fallback"
    );
    assert!(
        fixture.checks.load(Ordering::Relaxed) > 0,
        "Model must run checks itself"
    );
    assert_eq!(
        Suite::detect(&path, None)?
            .run(&path, CancellationToken::new())
            .await?["passed"],
        true
    );
    eprintln!(
        "Verified local native coding: {} native calls; {:.1}s to independently passing test",
        native_calls.load(Ordering::Relaxed),
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

#[tokio::test]
#[ignore = "Requires an explicitly configured local model server; synthetic data only"]
async fn local_model_compacts_synthetic_constraints_without_losing_history() -> Result<()> {
    let base = std::env::var("MELLY_LIVE_BASE_URL")?;
    ensure!(
        providers::compatible::local(&providers::compatible::parse_base(&base)?),
        "Use localhost"
    );
    let model = std::env::var("MELLY_LIVE_MODEL")?;
    let store = Store::memory()?;
    let chat = store.create_chat(None)?;
    let messages = [
        "Goal: repair an offline report generator. User constraint: keep the exact report key violet-482. Never rename it.",
        "Decision: preserve that key and use existing JSON fixtures.",
        "Completed: changed parser.py to accept blank lines. A regression test passed.",
        "Recorded: blank-line regression passed; broader checks remain pending.",
        "Pending: handle negative quantities in report.py. Do not claim this part is done.",
        "Plan: inspect report.py, add the failing case, implement the fix, and rerun checks.",
        "Keep all files inside the linked project and ask before deleting anything.",
        "Understood. The next task is still pending.",
        "Latest question: what should we check next?",
        "We should read the quantity handling.",
        "Latest instruction: preserve this exchange verbatim.",
        "Ready for the next step.",
    ];
    for (i, text) in messages.iter().enumerate() {
        store.insert_message(
            &chat.id,
            if i % 2 == 0 { "user" } else { "assistant" },
            text,
            "complete",
            None,
            None,
            None,
        )?;
    }
    let provider = "compatible:00000000-0000-4000-8000-000000000001";
    store.save_compatible_connection(CompatibleConnection {
        id: provider.into(),
        name: "Local summary fixture".into(),
        base_url: base,
        text_tool_calls: false,
        models: vec![ModelInfo {
            id: model.clone(),
            name: model.clone(),
            context_window: Some(32768),
            default_context_window: Some(32768),
        }],
    })?;
    let engine = Arc::new(Engine::new(
        Arc::new(Mutex::new(store)),
        Arc::new(MemorySecrets::default()),
    ));
    let summary = engine.compact_context(&chat.id, provider, &model).await?;
    ensure!(
        summary.text.contains("violet-482")
            && summary.text.contains("report.py")
            && summary.text.to_lowercase().contains("pending"),
        "Summary omitted a required synthetic fact"
    );
    let store = engine.store.lock().unwrap();
    assert_eq!(store.messages(&chat.id, None)?.len(), 12);
    assert_eq!(store.context(&chat.id)?.len(), 4);
    eprintln!(
        "Local summary retained the synthetic report key and pending task; all 12 originals remain."
    );
    Ok(())
}
