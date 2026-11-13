//! Repeatable local-only performance probe. Synthetic data, no provider calls.
use melly_core::{models::Settings, storage::Store};
use std::time::Instant;
fn main() -> anyhow::Result<()> {
    let start = Instant::now();
    let db = Store::memory()?;
    let opened = start.elapsed();
    let f = db.create_folder("Synthetic benchmark")?;
    let first = db.create_chat(Some(&f.id))?;
    let query = db.create_chat(Some(&f.id))?;
    for n in 0..10_000 {
        db.insert_message(
            &first.id,
            "user",
            &format!(
                "Synthetic reference {n}: transit project result improved forecast accuracy by 18%."
            ),
            "complete",
            None,
            None,
            None,
        )?;
    }
    let mut insert = Vec::new();
    let mut search = Vec::new();
    let mut history = Vec::new();
    for _ in 0..100 {
        let t = Instant::now();
        db.insert_message(
            &query.id,
            "user",
            "Synthetic benchmark turn",
            "complete",
            None,
            None,
            None,
        )?;
        insert.push(t.elapsed().as_micros());
        let t = Instant::now();
        db.search_folder(&query.id, "forecast", 3)?;
        search.push(t.elapsed().as_micros());
        let t = Instant::now();
        db.messages(&first.id, None)?;
        history.push(t.elapsed().as_micros());
    }
    insert.sort();
    search.sort();
    history.sort();
    db.save_settings(&Settings::default())?;
    println!(
        "Synthetic SQLite in-memory · 10,000 messages · 100 samples\nOpen/schema: {} us\nInsert p50/p95: {}/{} us\nFolder search p50/p95: {}/{} us\n60-message page p50/p95: {}/{} us",
        opened.as_micros(),
        insert[50],
        insert[95],
        search[50],
        search[95],
        history[50],
        history[95]
    );
    Ok(())
}
