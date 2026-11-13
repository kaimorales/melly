//! Secret stores. Production uses only the operating system credential vault.
use anyhow::{Result, anyhow, bail};
use std::{collections::HashMap, sync::Mutex};

const SERVICE: &str = "app.melly.desktop.credentials";

pub trait SecretStore: Send + Sync {
    fn get(&self, id: &str) -> Result<Option<String>>;
    /// Presence only. Production must not decrypt secrets or show auth UI here.
    fn contains(&self, id: &str) -> Result<bool> {
        Ok(self.get(id)?.is_some())
    }
    fn set(&self, id: &str, value: &str) -> Result<()>;
    fn delete(&self, id: &str) -> Result<()>;
}

/// In-memory test double. The desktop application must use `KeychainStore`.
#[derive(Default)]
pub struct MemorySecrets(Mutex<HashMap<String, String>>);

impl std::fmt::Debug for MemorySecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MemorySecrets([redacted])")
    }
}

impl SecretStore for MemorySecrets {
    fn get(&self, id: &str) -> Result<Option<String>> {
        Ok(self
            .0
            .lock()
            .map_err(|_| anyhow!("Credential vault unavailable"))?
            .get(id)
            .cloned())
    }
    fn set(&self, id: &str, value: &str) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| anyhow!("Credential vault unavailable"))?
            .insert(id.to_owned(), value.to_owned());
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<()> {
        self.0
            .lock()
            .map_err(|_| anyhow!("Credential vault unavailable"))?
            .remove(id);
        Ok(())
    }
}

/// Uses a Melly-specific generic-password service in the user's macOS Keychain.
/// There is deliberately no disk, environment-variable, or other-app fallback.
#[derive(Default, Debug)]
pub struct KeychainStore;

impl KeychainStore {
    pub fn new() -> Self {
        Self
    }
}

fn validate_id(id: &str) -> Result<()> {
    if id.is_empty() || id.len() > 2048 || id.chars().any(char::is_control) {
        bail!("Invalid credential identifier");
    }
    Ok(())
}

/// Also covers the legacy login Keychain used by local, ad-hoc-signed builds.
/// Never allow Security.framework to ask for the user's Mac password. A denied
/// operation stays denied; there is no interactive retry or plaintext fallback.
#[cfg(target_os = "macos")]
fn noninteractive_options(id: &str) -> security_framework::passwords::PasswordOptions {
    use core_foundation::{
        base::TCFType,
        string::{CFString, CFStringRef},
    };
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        static kSecUseAuthenticationUI: CFStringRef;
        static kSecUseAuthenticationUIFail: CFStringRef;
    }
    let mut options =
        security_framework::passwords::PasswordOptions::new_generic_password(SERVICE, id);
    // The wrapper has no authentication-UI setter. These documented framework
    // constants are process-lifetime CFStrings; wrap_under_get_rule retains them.
    #[allow(deprecated)]
    unsafe {
        options.query.push((
            CFString::wrap_under_get_rule(kSecUseAuthenticationUI),
            CFString::wrap_under_get_rule(kSecUseAuthenticationUIFail).into_CFType(),
        ));
    }
    options
}

#[cfg(target_os = "macos")]
fn quiet_keychain<T>(operation: impl FnOnce() -> Result<T>) -> Result<T> {
    use security_framework::os::macos::keychain::SecKeychain;
    // Legacy SecItemCopyMatching_osx ignores kSecUseAuthenticationUIFail for
    // some login-Keychain ACLs. Serialize this short process-wide policy guard
    // so one operation cannot re-enable interaction underneath another.
    static OPERATIONS: Mutex<()> = Mutex::new(());
    let _operation = OPERATIONS
        .lock()
        .map_err(|_| anyhow!("Secure storage is unavailable"))?;
    let allowed = SecKeychain::user_interaction_allowed()
        .map_err(|_| anyhow!("Secure storage is unavailable"))?;
    let _no_ui = if allowed {
        Some(
            SecKeychain::disable_user_interaction()
                .map_err(|_| anyhow!("Secure storage is unavailable"))?,
        )
    } else {
        None
    };
    // The guard restores the prior setting on success, error, or unwind.
    operation()
}

impl SecretStore for KeychainStore {
    fn contains(&self, id: &str) -> Result<bool> {
        validate_id(id)?;
        #[cfg(target_os = "macos")]
        {
            quiet_keychain(|| {
                use security_framework::item::{ItemClass, ItemSearchOptions, Limit};
                let result = ItemSearchOptions::new()
                    .class(ItemClass::generic_password())
                    .service(SERVICE)
                    .account(id)
                    .load_attributes(true)
                    .load_data(false)
                    .limit(Limit::Max(1))
                    .skip_authenticated_items(true)
                    .search();
                match result {
                    Ok(items) => Ok(!items.is_empty()),
                    Err(e) if e.code() == -25300 || e.code() == -25308 => Ok(false),
                    Err(_) => bail!("macOS Keychain status is unavailable"),
                }
            })
        }
        #[cfg(not(target_os = "macos"))]
        bail!("Melly credentials require macOS Keychain")
    }
    fn get(&self, id: &str) -> Result<Option<String>> {
        validate_id(id)?;
        #[cfg(target_os = "macos")]
        {
            quiet_keychain(|| {
                match security_framework::passwords::generic_password(noninteractive_options(id)) {
                    Ok(value) => String::from_utf8(value)
                        .map(Some)
                        .map_err(|_| anyhow!("Stored credential is invalid")),
                    Err(error) if error.code() == -25300 => Ok(None),
                    Err(_) => bail!(
                        "This saved connection is unavailable. Reconnect the provider in Settings."
                    ),
                }
            })
        }
        #[cfg(not(target_os = "macos"))]
        bail!("Melly credentials require macOS Keychain")
    }

    fn set(&self, id: &str, value: &str) -> Result<()> {
        validate_id(id)?;
        if value.is_empty() || value.len() > 256 * 1024 {
            bail!("Invalid credential value");
        }
        #[cfg(target_os = "macos")]
        {
            quiet_keychain(|| {
                security_framework::passwords::set_generic_password_options(
                    value.as_bytes(),
                    noninteractive_options(id),
                )
                .map_err(|_| {
                    anyhow!(
                        "Secure storage is unavailable. Nothing was saved; you can keep using Demo."
                    )
                })
            })
        }
        #[cfg(not(target_os = "macos"))]
        bail!("Melly credentials require macOS Keychain")
    }

    fn delete(&self, id: &str) -> Result<()> {
        validate_id(id)?;
        #[cfg(target_os = "macos")]
        {
            quiet_keychain(
                || match security_framework::passwords::delete_generic_password_options(
                    noninteractive_options(id),
                ) {
                    Ok(()) => Ok(()),
                    Err(error) if error.code() == -25300 => Ok(()),
                    Err(_) => bail!("macOS Keychain could not remove the credential"),
                },
            )
        }
        #[cfg(not(target_os = "macos"))]
        bail!("Melly credentials require macOS Keychain")
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn legacy_keychain_policy_is_noninteractive_and_restored_after_error() {
        use security_framework::os::macos::keychain::SecKeychain;
        let before = SecKeychain::user_interaction_allowed().unwrap();
        let result: Result<()> = quiet_keychain(|| {
            assert!(!SecKeychain::user_interaction_allowed().unwrap());
            bail!("Synthetic failure")
        });
        assert!(result.is_err());
        assert_eq!(SecKeychain::user_interaction_allowed().unwrap(), before);
    }

    // Opt in only: touches one UUID-named synthetic credential, never real keys.
    // The same no-UI policy is used by every operation, including the update path.
    #[test]
    #[ignore = "native Keychain smoke check; run explicitly on a Mac"]
    fn native_keychain_roundtrip_without_authentication_ui() {
        let vault = KeychainStore::new();
        let id = format!("synthetic-check-{}", uuid::Uuid::new_v4());
        assert!(!vault.contains(&id).unwrap());
        assert!(vault.get(&id).unwrap().is_none());
        vault.set(&id, "synthetic-not-a-provider-key").unwrap();
        assert!(vault.contains(&id).unwrap());
        assert_eq!(
            vault.get(&id).unwrap().as_deref(),
            Some("synthetic-not-a-provider-key")
        );
        vault.set(&id, "synthetic-replacement").unwrap();
        assert_eq!(
            vault.get(&id).unwrap().as_deref(),
            Some("synthetic-replacement")
        );
        vault.delete(&id).unwrap();
        assert!(!vault.contains(&id).unwrap());
    }
}
