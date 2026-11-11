//! Opt-in live integration: one Sol lead, two automatic Lunas, one manual Luna.
//! Run only against a disposable directory containing a.txt, b.txt and c.txt.
#[path = "../src/rpc.rs"]
mod rpc;

use anyhow::{Context, Result, bail};
use melly::{
    delegation::{Harness, WorkerMode},
    session::Speaker,
};
use serde_json::json;
use std::{ffi::OsString, path::PathBuf, time::Duration};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let cwd = PathBuf::from(
        std::env::args()
            .nth(1)
            .context("Pass a disposable fixture directory")?,
    )
    .canonicalize()?;
    for file in ["a.txt", "b.txt", "c.txt"] {
        if !cwd.join(file).is_file() {
            bail!("Missing {file}");
        }
    }
    let bin = std::env::var_os("MELLY_CODEX_BIN").unwrap_or_else(|| OsString::from("codex"));
    let mut rpc = rpc::Rpc::connect(&bin, &cwd)?;
    let mut h = Harness::new(
        cwd.to_string_lossy().into_owned(),
        Some("gpt-6.1-sol".into()),
        None,
    );
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    let mut started = false;
    loop {
        h.tick();
        if !started && !h.agents[0].session.connecting() && h.agents[0].session.quota_loaded {
            h.spawn_manual("Read c.txt in this directory. Reply with its exact single-line value. Do not edit files.".into(),WorkerMode::Read,vec![]).map_err(anyhow::Error::msg)?;
            if !h.submit(0,"Integration check: use melly_agents.spawn exactly twice, both read mode, with independent bounded tasks. One Luna must read a.txt and return its value; the other must read b.txt and return its value. Include a clear reason and expected_output in each brief. Use melly_agents.wait for both ids, then reply with MELLY_TEAM_OK followed by the two values. Do not read those two files yourself and do not edit anything. A separate manual Luna is already checking c.txt; you do not need to wait for it.") {bail!("Lead prompt was rejected");}
            started = true;
        }
        for request in h.drain_requests() {
            rpc.send(&request).await?;
        }
        if started && !h.busy() && h.agents.len() >= 4 {
            let lead = &h.agents[0].session;
            if h.auto_launches() != 2 {
                bail!("Expected two automatic launches");
            }
            let answer = lead
                .entries
                .iter()
                .filter(|e| e.speaker == Speaker::Codex)
                .map(|e| e.text.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            if !answer.contains("MELLY_TEAM_OK") {
                bail!("Lead did not collect results");
            }
            for file in ["a.txt", "b.txt"] {
                let expected = std::fs::read_to_string(cwd.join(file))?;
                if !answer.contains(expected.trim()) {
                    bail!("Missing {file} value in lead answer");
                }
            }
            if h.agents
                .iter()
                .skip(1)
                .any(|a| !a.finished || a.result.as_deref().unwrap_or("").is_empty())
            {
                bail!("Missing a worker result");
            }
            let expected = std::fs::read_to_string(cwd.join("c.txt"))?;
            if !h.agents[1]
                .result
                .as_deref()
                .unwrap_or("")
                .contains(expected.trim())
            {
                bail!("Manual worker did not inspect c.txt");
            }
            println!(
                "{}",
                json!({"ok":true,"auto_launches":h.auto_launches(),"agents":h.agents.iter().map(|a|json!({"id":a.id,"source":a.source.label(),"model":a.session.model,"status":a.session.status,"tokens":a.session.tokens})).collect::<Vec<_>>()})
            );
            return Ok(());
        }
        tokio::select! {
            message=rpc.receive()=>match message? {Some(value)=>{h.receive(value);},None=>bail!("App Server disconnected")},
            _=tokio::time::sleep(Duration::from_millis(100))=>{},
            _=tokio::time::sleep_until(deadline)=>{
                for a in &h.agents {eprintln!("{} {} result={} chars",a.id,a.session.status,a.result.as_ref().map_or(0,|s|s.len()));}
                bail!("Timed out waiting for delegated work; auto-block reason: {:?}",h.automatic_block_reason);
            }
        }
    }
}
