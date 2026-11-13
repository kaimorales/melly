//! Secret stores. Production uses only the operating system credential vault.
use anyhow::{Result, anyhow, bail};
use std::{collections::HashMap, sync::Mutex};

const SERVICE: &str = "app.melly.desktop.credentials";

pub trait SecretStore: Send + Sync {
    fn get(&self, id: &str) -> Result<Option<String>>;
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

impl SecretStore for KeychainStore {
    fn get(&self, id: &str) -> Result<Option<String>> {
        validate_id(id)?;
        #[cfg(target_os = "macos")]
        {
            match security_framework::passwords::get_generic_password(SERVICE, id) {
                Ok(value) => String::from_utf8(value)
                    .map(Some)
                    .map_err(|_| anyhow!("Stored credential is invalid")),
                Err(error) if error.code() == -25300 => Ok(None),
                Err(_) => bail!("macOS Keychain could not read the credential"),
            }
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
            security_framework::passwords::set_generic_password(SERVICE, id, value.as_bytes())
                .map_err(|_| anyhow!("macOS Keychain could not save the credential"))
        }
        #[cfg(not(target_os = "macos"))]
        bail!("Melly credentials require macOS Keychain")
    }

    fn delete(&self, id: &str) -> Result<()> {
        validate_id(id)?;
        #[cfg(target_os = "macos")]
        {
            match security_framework::passwords::delete_generic_password(SERVICE, id) {
                Ok(()) => Ok(()),
                Err(error) if error.code() == -25300 => Ok(()),
                Err(_) => bail!("macOS Keychain could not remove the credential"),
            }
        }
        #[cfg(not(target_os = "macos"))]
        bail!("Melly credentials require macOS Keychain")
    }
}
