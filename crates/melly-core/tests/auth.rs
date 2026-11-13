use melly_core::{
    auth,
    credentials::{MemorySecrets, SecretStore},
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio_util::sync::CancellationToken;

#[test]
fn memory_vault_keeps_credentials_separate_and_redacts_debug() {
    let vault = MemorySecrets::default();
    vault.set("first", "synthetic-first-secret").unwrap();
    vault.set("second", "synthetic-second-secret").unwrap();
    assert_eq!(
        vault.get("first").unwrap().as_deref(),
        Some("synthetic-first-secret")
    );
    assert_eq!(
        vault.get("second").unwrap().as_deref(),
        Some("synthetic-second-secret")
    );
    assert!(!format!("{vault:?}").contains("synthetic"));
    vault.delete("first").unwrap();
    vault.delete("first").unwrap();
    assert!(vault.get("first").unwrap().is_none());
    assert!(vault.get("second").unwrap().is_some());
}

#[test]
fn empty_or_malformed_session_is_not_connected() {
    let vault = MemorySecrets::default();
    assert!(!auth::is_connected(&vault));
    vault.set("chatgpt.session.v1", "not valid JSON").unwrap();
    assert!(!auth::is_connected(&vault));
}

#[test]
fn saved_plan_status_reads_metadata_only_and_never_decrypts_credentials() {
    struct MetadataOnly;
    impl SecretStore for MetadataOnly {
        fn contains(&self, id: &str) -> anyhow::Result<bool> {
            assert_eq!(id, "chatgpt.saved-plan.v1");
            Ok(true)
        }
        fn get(&self, _: &str) -> anyhow::Result<Option<String>> {
            panic!("startup must not read secret bytes")
        }
        fn set(&self, _: &str, _: &str) -> anyhow::Result<()> {
            panic!("startup must not change credentials")
        }
        fn delete(&self, _: &str) -> anyhow::Result<()> {
            panic!("startup must not change credentials")
        }
    }
    assert!(auth::has_saved_plan(&MetadataOnly));
    assert!(!auth::has_saved_plan(&MemorySecrets::default()));
}

#[tokio::test]
async fn cancellation_never_opens_the_browser_or_connects() {
    let vault = Arc::new(MemorySecrets::default());
    let opened = Arc::new(AtomicBool::new(false));
    let observer = opened.clone();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result = auth::sign_in(
        vault.clone(),
        Arc::new(move |_| {
            observer.store(true, Ordering::SeqCst);
        }),
        cancel,
    )
    .await;
    assert!(result.unwrap_err().to_string().contains("cancelled"));
    assert!(!opened.load(Ordering::SeqCst));
    assert!(!auth::is_connected(vault.as_ref()));
}

#[tokio::test]
async fn empty_session_requires_sign_in_without_network() {
    let error = auth::access_token(Arc::new(MemorySecrets::default()))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("sign in"));
}

#[test]
fn disconnect_removes_tokens_and_retains_registration_and_provider_keys() {
    let vault = MemorySecrets::default();
    vault
        .set("chatgpt.session.v1", "synthetic-session")
        .unwrap();
    vault.set("chatgpt.saved-plan.v1", "saved").unwrap();
    vault
        .set("chatgpt.registration.v1", "synthetic-registration")
        .unwrap();
    vault.set("provider-key", "synthetic-provider-key").unwrap();
    auth::disconnect(&vault).unwrap();
    assert!(vault.get("chatgpt.session.v1").unwrap().is_none());
    assert_eq!(
        vault.get("chatgpt.registration.v1").unwrap().as_deref(),
        Some("synthetic-registration")
    );
    assert!(vault.get("provider-key").unwrap().is_some());
    assert!(!auth::is_connected(&vault));
    assert!(!auth::has_saved_plan(&vault));
}
