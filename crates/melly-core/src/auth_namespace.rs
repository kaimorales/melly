//! Immutable ChatGPT credential namespaces; disk stores UUID selectors only.
use crate::{
    auth,
    credentials::{MemorySecrets, SecretStore},
};
use anyhow::{Result, anyhow, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const METADATA_ERROR: &str = "ChatGPT connection selection could not be saved";
const STARTUP_ERROR: &str =
    "ChatGPT connection metadata unavailable; continue with ChatGPT to reconnect";
const MAX_METADATA: u64 = 4096;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    active: Option<Uuid>,
    pending: Option<Uuid>,
}

struct ScopedSecrets {
    base: Arc<dyn SecretStore>,
    namespace: Uuid,
}
impl ScopedSecrets {
    fn key(&self, key: &str) -> Result<String> {
        ensure!(
            matches!(
                key,
                "chatgpt.host.v1"
                    | "chatgpt.registration.v1"
                    | "chatgpt.session.v1"
                    | "chatgpt.generation.v1"
                    | "chatgpt.saved-plan.v1"
            ),
            "Invalid ChatGPT credential record"
        );
        Ok(format!("chatgpt.ns.{}/{key}", self.namespace))
    }
}
impl SecretStore for ScopedSecrets {
    fn get(&self, key: &str) -> Result<Option<String>> {
        self.base.get(&self.key(key)?)
    }
    fn contains(&self, key: &str) -> Result<bool> {
        self.base.contains(&self.key(key)?)
    }
    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.base.set(&self.key(key)?, value)
    }
    fn delete(&self, key: &str) -> Result<()> {
        self.base.delete(&self.key(key)?)
    }
}

/// Captures one namespace for the complete OAuth operation; it never follows
/// changes to the active selection. No secret-bearing type implements Debug.
pub struct AuthAttempt {
    namespace: Uuid,
    generation: u64,
    vault: Arc<dyn SecretStore>,
}
/// Only successful identity + plan verification can produce this receipt.
pub struct AuthorizedPlan(AuthAttempt);
impl AuthAttempt {
    pub async fn sign_in(
        self,
        on_url: Arc<dyn Fn(String) + Send + Sync>,
        cancel: CancellationToken,
    ) -> Result<AuthorizedPlan> {
        auth::sign_in(self.vault.clone(), on_url, cancel.clone()).await?;
        ensure!(!cancel.is_cancelled(), "ChatGPT sign-in cancelled");
        Ok(AuthorizedPlan(self))
    }
}

/// Own under the desktop login lock when beginning, activating or detaching.
/// `open` reads bounded public metadata only, never a Keychain value or marker.
pub struct AuthNamespaces {
    base: Arc<dyn SecretStore>,
    path: PathBuf,
    selection: Selection,
    process_pending: Option<Uuid>,
    generation: u64,
    startup_error: Option<&'static str>,
}
impl AuthNamespaces {
    pub fn open(base: Arc<dyn SecretStore>, path: PathBuf) -> Result<Self> {
        // A damaged selector affects only ChatGPT, never healthy local history
        // or other providers. Preserve it until explicit sign-in/disconnect.
        let (selection, startup_error) = match read_selection(&path) {
            Ok(selection) => (selection, None),
            Err(_) => (Selection::default(), Some(STARTUP_ERROR)),
        };
        Ok(Self {
            base,
            path,
            selection,
            process_pending: None,
            generation: 0,
            startup_error,
        })
    }

    pub fn startup_error(&self) -> Option<&'static str> {
        self.startup_error
    }

    fn preserve_invalid_metadata(&self) -> Result<()> {
        if self.startup_error.is_none() {
            return Ok(());
        }
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(anyhow!(METADATA_ERROR)),
        };
        // Never follow a substituted link or move a directory. Leave an unsafe
        // path intact and report recovery failure while the rest of Melly works.
        ensure!(metadata.file_type().is_file(), METADATA_ERROR);
        let parent = self.path.parent().ok_or_else(|| anyhow!(METADATA_ERROR))?;
        let backup = parent.join(format!(
            "chatgpt-selection-preserved-{}.json",
            Uuid::new_v4()
        ));
        fs::rename(&self.path, &backup).map_err(|_| anyhow!(METADATA_ERROR))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&backup, fs::Permissions::from_mode(0o600))
                .map_err(|_| anyhow!(METADATA_ERROR))?;
        }
        Ok(())
    }

    fn scoped(&self, namespace: Uuid) -> Arc<dyn SecretStore> {
        Arc::new(ScopedSecrets {
            base: self.base.clone(),
            namespace,
        })
    }

    pub fn selected_vault(&self) -> Arc<dyn SecretStore> {
        self.selection
            .active
            .map(|id| self.scoped(id))
            // Disconnected sentinel only: native auth never writes into it.
            .unwrap_or_else(|| Arc::new(MemorySecrets::default()))
    }

    /// Explicit user sign-in only. A pending namespace can be reused in this
    /// process; a restarted/rebuilt app never probes old pending credentials.
    pub fn begin(&mut self) -> Result<AuthAttempt> {
        let namespace = self.process_pending.unwrap_or_else(Uuid::new_v4);
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| anyhow!("Restart Melly before signing in again"))?;
        let selection = Selection {
            active: self.selection.active,
            pending: Some(namespace),
        };
        self.preserve_invalid_metadata()?;
        write_selection(&self.path, &selection)?;
        self.selection = selection;
        self.process_pending = Some(namespace);
        self.generation = generation;
        self.startup_error = None;
        Ok(AuthAttempt {
            namespace,
            generation,
            vault: self.scoped(namespace),
        })
    }

    /// Caller must also hold the native LoginState generation guard. Persist
    /// before switching the engine snapshot, so failure keeps the active route.
    pub fn activate(&mut self, authorized: AuthorizedPlan) -> Result<Arc<dyn SecretStore>> {
        let attempt = authorized.0;
        ensure!(
            self.generation == attempt.generation
                && self.process_pending == Some(attempt.namespace)
                && self.selection.pending == Some(attempt.namespace),
            "ChatGPT sign-in was cancelled or replaced"
        );
        let selection = Selection {
            active: Some(attempt.namespace),
            pending: None,
        };
        write_selection(&self.path, &selection)?;
        self.selection = selection;
        self.process_pending = None;
        Ok(attempt.vault)
    }

    /// Detaches locally without touching old protected records. A detached
    /// refresh may finish in its captured namespace but cannot reselect itself.
    /// Even a metadata-write failure leaves this process detached; report it.
    pub fn detach(&mut self) -> Result<()> {
        self.selection = Selection::default();
        self.process_pending = None;
        self.generation = self.generation.saturating_add(1);
        self.preserve_invalid_metadata()?;
        write_selection(&self.path, &self.selection)?;
        self.startup_error = None;
        Ok(())
    }
}

fn read_selection(path: &Path) -> Result<Selection> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Selection::default());
        }
        Err(_) => return Err(anyhow!("ChatGPT connection selection could not be read")),
    };
    ensure!(
        metadata.file_type().is_file() && metadata.len() <= MAX_METADATA,
        "ChatGPT connection selection is invalid"
    );
    let mut bytes = Vec::new();
    File::open(path)
        .and_then(|file| file.take(MAX_METADATA + 1).read_to_end(&mut bytes))
        .map_err(|_| anyhow!("ChatGPT connection selection could not be read"))?;
    ensure!(
        bytes.len() <= MAX_METADATA as usize,
        "ChatGPT connection selection is invalid"
    );
    serde_json::from_slice(&bytes).map_err(|_| anyhow!("ChatGPT connection selection is invalid"))
}

fn write_selection(path: &Path, selection: &Selection) -> Result<()> {
    let parent = path.parent().ok_or_else(|| anyhow!(METADATA_ERROR))?;
    let temporary = parent.join(format!(".chatgpt-selection-{}.tmp", Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&serde_json::to_vec(selection)?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|_| anyhow!(METADATA_ERROR))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::MemorySecrets;

    fn authorized(attempt: AuthAttempt) -> AuthorizedPlan {
        // The auth module independently tests real signature/scope validation.
        // These receipts exercise only namespace selection and persistence.
        AuthorizedPlan(attempt)
    }

    #[test]
    fn malformed_selection_keeps_history_available_and_explicit_recovery_preserves_it() {
        use crate::{engine::Engine, storage::Store};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        let damaged = b"synthetic damaged selector";
        fs::write(&path, damaged).unwrap();
        let vault = Arc::new(MemorySecrets::default());
        let mut state = AuthNamespaces::open(vault.clone(), path.clone()).unwrap();
        assert!(state.startup_error().is_some());
        assert_eq!(fs::read(&path).unwrap(), damaged);
        let store = Arc::new(std::sync::Mutex::new(Store::memory().unwrap()));
        store.lock().unwrap().create_chat(None).unwrap();
        let engine = Engine::new(store, vault);
        engine.select_plan_vault(state.selected_vault());
        let bootstrap = engine.bootstrap().unwrap();
        assert_eq!(bootstrap.chats.len(), 1);
        assert!(
            bootstrap
                .providers
                .iter()
                .any(|p| p.spec.id == "mock" && p.configured)
        );
        assert!(
            bootstrap
                .providers
                .iter()
                .any(|p| p.spec.id == "chatgpt" && !p.configured)
        );
        state.begin().unwrap();
        assert!(state.startup_error().is_none());
        let backup = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|entry| entry != &path)
            .unwrap();
        assert_eq!(fs::read(&backup).unwrap(), damaged);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(backup).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(read_selection(&path).unwrap().pending.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_selection_opens_disconnected_and_recovers_without_reading_old_contents() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        fs::write(&path, b"synthetic inaccessible selector").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o0)).unwrap();
        let mut state =
            AuthNamespaces::open(Arc::new(MemorySecrets::default()), path.clone()).unwrap();
        assert!(state.startup_error().is_some());
        assert!(!auth::has_saved_plan(state.selected_vault().as_ref()));
        state.begin().unwrap();
        let backup = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|entry| entry != &path)
            .unwrap();
        assert_eq!(
            fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::read(backup).unwrap(),
            b"synthetic inaccessible selector"
        );
    }

    #[cfg(unix)]
    #[test]
    fn unsafe_metadata_path_stays_disconnected_and_never_changes_its_target() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        let target = dir.path().join("unrelated.json");
        fs::write(&target, b"synthetic unrelated file").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
        symlink(&target, &path).unwrap();
        let mut state =
            AuthNamespaces::open(Arc::new(MemorySecrets::default()), path.clone()).unwrap();
        assert!(state.startup_error().is_some());
        assert!(state.begin().is_err());
        assert!(fs::symlink_metadata(path).unwrap().file_type().is_symlink());
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o644
        );
        assert_eq!(fs::read(target).unwrap(), b"synthetic unrelated file");
    }

    #[tokio::test]
    async fn rebuilt_app_can_open_browser_without_reading_denied_active_or_legacy_records() {
        struct Guarded {
            values: MemorySecrets,
            denied: std::sync::Mutex<Option<String>>,
        }
        impl Guarded {
            fn allowed(&self, id: &str) -> Result<()> {
                ensure!(
                    id.starts_with("chatgpt.ns.")
                        && !self
                            .denied
                            .lock()
                            .unwrap()
                            .as_ref()
                            .is_some_and(|prefix| id.starts_with(prefix)),
                    "Synthetic ACL denied"
                );
                Ok(())
            }
        }
        impl SecretStore for Guarded {
            fn get(&self, id: &str) -> Result<Option<String>> {
                self.allowed(id)?;
                self.values.get(id)
            }
            fn contains(&self, id: &str) -> Result<bool> {
                self.allowed(id)?;
                self.values.contains(id)
            }
            fn set(&self, id: &str, value: &str) -> Result<()> {
                self.allowed(id)?;
                self.values.set(id, value)
            }
            fn delete(&self, id: &str) -> Result<()> {
                self.allowed(id)?;
                self.values.delete(id)
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        let base = Arc::new(Guarded {
            values: MemorySecrets::default(),
            denied: std::sync::Mutex::new(None),
        });
        let mut old = AuthNamespaces::open(base.clone(), path.clone()).unwrap();
        let prior = old.begin().unwrap();
        let denied = format!("chatgpt.ns.{}/", prior.namespace);
        prior
            .vault
            .set("chatgpt.host.v1", "synthetic-old-host")
            .unwrap();
        old.activate(authorized(prior)).unwrap();
        *base.denied.lock().unwrap() = Some(denied);
        let mut rebuilt = AuthNamespaces::open(base, path).unwrap();
        let fresh = rebuilt.begin().unwrap();
        let opened = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = opened.clone();
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        let result = fresh
            .sign_in(
                Arc::new(move |_| {
                    observed.store(true, std::sync::atomic::Ordering::SeqCst);
                    trigger.cancel();
                }),
                cancel,
            )
            .await;
        assert!(result.err().unwrap().to_string().contains("cancelled"));
        assert!(opened.load(std::sync::atomic::Ordering::SeqCst));
        // Disconnect also succeeds although the old records cannot be deleted.
        rebuilt.detach().unwrap();
        assert!(
            rebuilt
                .selected_vault()
                .get("chatgpt.session.v1")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn namespaces_isolate_every_auth_record_and_preserve_legacy_items() {
        let dir = tempfile::tempdir().unwrap();
        let base = Arc::new(MemorySecrets::default());
        base.set("chatgpt.host.v1", "synthetic-legacy-host")
            .unwrap();
        base.set("provider-key", "synthetic-api-key").unwrap();
        let mut state = AuthNamespaces::open(base.clone(), dir.path().join("auth.json")).unwrap();
        let attempt = state.begin().unwrap();
        let vault = attempt.vault.clone();
        for key in [
            "chatgpt.host.v1",
            "chatgpt.registration.v1",
            "chatgpt.session.v1",
            "chatgpt.generation.v1",
            "chatgpt.saved-plan.v1",
        ] {
            assert!(vault.get(key).unwrap().is_none());
            vault.set(key, "synthetic-new-record").unwrap();
        }
        assert!(vault.set("provider-key", "must-not-write").is_err());
        assert!(vault.get("provider-key").is_err());
        assert_eq!(
            base.get("chatgpt.host.v1").unwrap().as_deref(),
            Some("synthetic-legacy-host")
        );
        assert_eq!(
            base.get("provider-key").unwrap().as_deref(),
            Some("synthetic-api-key")
        );
    }

    #[test]
    fn activation_persists_only_uuid_selectors_in_a_private_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        let base = Arc::new(MemorySecrets::default());
        let mut state = AuthNamespaces::open(base.clone(), path.clone()).unwrap();
        let attempt = state.begin().unwrap();
        attempt
            .vault
            .set("chatgpt.session.v1", "synthetic-session-secret")
            .unwrap();
        let snapshot = state.activate(authorized(attempt)).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 2);
        assert!(Uuid::parse_str(value["active"].as_str().unwrap()).is_ok());
        assert!(value["pending"].is_null());
        assert!(!raw.contains("synthetic"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let reopened = AuthNamespaces::open(base, path).unwrap();
        assert_eq!(
            reopened.selected_vault().get("chatgpt.session.v1").unwrap(),
            snapshot.get("chatgpt.session.v1").unwrap()
        );
    }

    #[test]
    fn retry_reuses_pending_only_in_this_process_and_restart_skips_old_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        let base = Arc::new(MemorySecrets::default());
        let mut state = AuthNamespaces::open(base.clone(), path.clone()).unwrap();
        let first = state.begin().unwrap();
        first
            .vault
            .set("chatgpt.registration.v1", "synthetic-issued-registration")
            .unwrap();
        let retry = state.begin().unwrap();
        assert_eq!(retry.namespace, first.namespace);
        assert_eq!(
            retry
                .vault
                .get("chatgpt.registration.v1")
                .unwrap()
                .as_deref(),
            Some("synthetic-issued-registration")
        );
        let mut restarted = AuthNamespaces::open(base, path).unwrap();
        let fresh = restarted.begin().unwrap();
        assert_ne!(fresh.namespace, first.namespace);
        assert!(
            fresh
                .vault
                .get("chatgpt.registration.v1")
                .unwrap()
                .is_none()
        );
        assert!(
            first
                .vault
                .get("chatgpt.registration.v1")
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn failed_or_cancelled_authorization_keeps_the_active_connection() {
        for cancelled in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("auth.json");
            let base = Arc::new(MemorySecrets::default());
            let mut state = AuthNamespaces::open(base.clone(), path.clone()).unwrap();
            let initial = state.begin().unwrap();
            initial
                .vault
                .set("chatgpt.session.v1", "synthetic-active-session")
                .unwrap();
            state.activate(authorized(initial)).unwrap();
            let attempt = state.begin().unwrap();
            let cancel = CancellationToken::new();
            if cancelled {
                cancel.cancel();
            }
            let on_url = Arc::new(|url: String| {
                let url = url::Url::parse(&url).unwrap();
                let query: std::collections::HashMap<_, _> =
                    url.query_pairs().into_owned().collect();
                let mut callback = url::Url::parse(&query["redirect_uri"]).unwrap();
                callback
                    .query_pairs_mut()
                    .append_pair("state", "synthetic-wrong-state")
                    .append_pair("code", "synthetic-code")
                    .append_pair("client_id", "oaiapp_test");
                tokio::spawn(async move {
                    let _ = reqwest::get(callback).await;
                });
            });
            assert!(attempt.sign_in(on_url, cancel).await.is_err());
            let reopened = AuthNamespaces::open(base, path).unwrap();
            assert_eq!(
                reopened
                    .selected_vault()
                    .get("chatgpt.session.v1")
                    .unwrap()
                    .as_deref(),
                Some("synthetic-active-session")
            );
        }
    }

    #[test]
    fn stale_attempts_cannot_activate_and_late_refresh_stays_in_its_captured_namespace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        let base = Arc::new(MemorySecrets::default());
        let mut state = AuthNamespaces::open(base.clone(), path.clone()).unwrap();
        let old = state.begin().unwrap();
        let captured_refresh = state.activate(authorized(old)).unwrap();
        let stale = state.begin().unwrap();
        state.detach().unwrap();
        let current = state.begin().unwrap();
        current
            .vault
            .set("chatgpt.session.v1", "synthetic-current-pair")
            .unwrap();
        assert!(state.activate(authorized(stale)).is_err());
        state.activate(authorized(current)).unwrap();
        captured_refresh
            .set("chatgpt.session.v1", "synthetic-rotated-old-pair")
            .unwrap();
        assert_eq!(
            state
                .selected_vault()
                .get("chatgpt.session.v1")
                .unwrap()
                .as_deref(),
            Some("synthetic-current-pair")
        );
        state.detach().unwrap();
        captured_refresh
            .set("chatgpt.saved-plan.v1", "saved")
            .unwrap();
        assert!(!auth::has_saved_plan(state.selected_vault().as_ref()));
        assert!(!auth::has_saved_plan(
            AuthNamespaces::open(base, path)
                .unwrap()
                .selected_vault()
                .as_ref()
        ));
    }

    #[test]
    fn metadata_failure_never_selects_an_unpersisted_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("auth.json");
        let mut state =
            AuthNamespaces::open(Arc::new(MemorySecrets::default()), path.clone()).unwrap();
        let first = state.begin().unwrap();
        first
            .vault
            .set("chatgpt.session.v1", "synthetic-active")
            .unwrap();
        state.activate(authorized(first)).unwrap();
        let next = state.begin().unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(state.activate(authorized(next)).is_err());
        assert_eq!(
            state
                .selected_vault()
                .get("chatgpt.session.v1")
                .unwrap()
                .as_deref(),
            Some("synthetic-active")
        );
        assert!(state.detach().is_err());
        assert!(
            state
                .selected_vault()
                .get("chatgpt.session.v1")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn engine_plan_snapshot_does_not_change_base_api_key_routing() {
        use crate::{engine::Engine, storage::Store};
        let base = Arc::new(MemorySecrets::default());
        base.set("provider-key", "synthetic-api-key").unwrap();
        let engine = Engine::new(
            Arc::new(std::sync::Mutex::new(Store::memory().unwrap())),
            base.clone(),
        );
        let first = Arc::new(MemorySecrets::default());
        first.set("chatgpt.saved-plan.v1", "saved").unwrap();
        engine.select_plan_vault(first.clone());
        let captured = engine.plan_vault();
        engine.select_plan_vault(Arc::new(MemorySecrets::default()));
        captured
            .set("chatgpt.session.v1", "synthetic-late-refresh")
            .unwrap();
        assert!(
            engine
                .plan_vault()
                .get("chatgpt.session.v1")
                .unwrap()
                .is_none()
        );
        assert!(
            !engine
                .bootstrap()
                .unwrap()
                .providers
                .iter()
                .find(|p| p.spec.id == "chatgpt")
                .unwrap()
                .configured
        );
        assert!(first.get("chatgpt.session.v1").unwrap().is_some());
        assert_eq!(
            engine.vault.get("provider-key").unwrap().as_deref(),
            Some("synthetic-api-key")
        );
    }
}
