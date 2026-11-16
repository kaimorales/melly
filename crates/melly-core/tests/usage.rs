use melly_core::{
    models::Connection,
    storage::Store,
    usage::{Rates, Usage},
};
use serde_json::json;
#[test]
fn cumulative_counts_unknown_cost_and_endpoint_scoping_are_honest() {
    let store = Store::memory().unwrap();
    let c = Connection {
        provider_id: "custom".into(),
        base_url: "https://api.example.com/v1".into(),
        model_id: "model".into(),
    };
    let mut usage = Usage::from_value(
        &json!({"input_tokens":100,"cache_read_input_tokens":20,"output_tokens":1}),
    )
    .unwrap();
    usage.merge(Usage::from_value(&json!({"output_tokens":40})).unwrap());
    usage.merge(Usage::from_value(&json!({"output_tokens":25})).unwrap());
    assert_eq!(usage.input, Some(120));
    assert_eq!(usage.output, Some(40));
    store.record_usage("request-a", &c, "chat", &usage).unwrap();
    store.record_usage("request-a", &c, "chat", &usage).unwrap();
    let summary = store.usage_summary(&c).unwrap();
    assert_eq!(summary.requests, 1);
    assert_eq!(summary.unpriced_requests, 1);
    assert_eq!(summary.reported_cost, 0.);
    store
        .save_rates(
            &c,
            Rates {
                input_per_million: 2.,
                output_per_million: 8.,
            },
        )
        .unwrap();
    store.record_usage("request-b", &c, "chat", &usage).unwrap();
    let summary = store.usage_summary(&c).unwrap();
    assert!((summary.estimated_cost - 0.00056).abs() < 0.0000001);
    assert_eq!(summary.unpriced_requests, 1);
    let mut changed = c.clone();
    changed.base_url = "https://another.example.com/v1".into();
    assert_eq!(store.usage_summary(&changed).unwrap().requests, 0);
    store
        .record_usage("request-c", &c, "chat", &Usage::default())
        .unwrap();
    assert_eq!(store.usage_summary(&c).unwrap().missing_usage, 1);
    store
        .record_usage(
            "request-d",
            &c,
            "chat",
            &Usage::from_value(&json!({"prompt_tokens":10,"completion_tokens":5,"cost":0.25}))
                .unwrap(),
        )
        .unwrap();
    assert_eq!(store.usage_summary(&c).unwrap().reported_cost, 0.25);
    assert_eq!(
        Usage::from_value(&json!({"prompt_eval_count":25,"eval_count":5}))
            .unwrap()
            .input,
        Some(25)
    );
}

#[test]
fn unfinished_requests_remain_unknown_and_reported_cost_replaces_estimates() {
    let store = Store::memory().unwrap();
    let c = Connection {
        provider_id: "custom".into(),
        base_url: "https://api.example.com/v1".into(),
        model_id: "model".into(),
    };
    store
        .record_usage("crash", &c, "chat", &Usage::default())
        .unwrap();
    assert_eq!(store.usage_summary(&c).unwrap().missing_usage, 1);
    store
        .save_rates(
            &c,
            Rates {
                input_per_million: 0.,
                output_per_million: 0.,
            },
        )
        .unwrap();
    store
        .record_usage(
            "crash",
            &c,
            "chat",
            &Usage {
                input: Some(5),
                output: Some(2),
                ..Default::default()
            },
        )
        .unwrap();
    let summary = store.usage_summary(&c).unwrap();
    assert_eq!(
        (
            summary.requests,
            summary.missing_usage,
            summary.estimated_requests,
            summary.unpriced_requests
        ),
        (1, 0, 1, 0)
    );
    assert_eq!(summary.estimated_cost, 0.);
    store
        .record_usage(
            "crash",
            &c,
            "chat",
            &Usage {
                cost_usd: Some(0.),
                ..Default::default()
            },
        )
        .unwrap();
    let summary = store.usage_summary(&c).unwrap();
    assert_eq!(
        (
            summary.input,
            summary.output,
            summary.reported_requests,
            summary.estimated_requests
        ),
        (5, 2, 1, 0)
    );
}
