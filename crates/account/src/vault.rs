//! The app root stores only profile metadata and a cross-process lock.
use crate::MinecraftProfile;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(feature = "dev-simulation")]
use std::collections::BTreeMap;
use std::{
    fs::{self, File, TryLockError},
    io::{self, Read},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;
use zeroize::Zeroizing;

pub(super) type Secret = Zeroizing<String>;
pub(super) const ACCOUNT_LIMIT: usize = 16;
const METADATA_LIMIT: u64 = 16 * 1024;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedAccounts {
    pub active: Option<Uuid>,
    pub accounts: Vec<SavedProfile>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SavedProfile {
    pub id: Uuid,
    pub name: String,
}

impl From<&MinecraftProfile> for SavedProfile {
    fn from(profile: &MinecraftProfile) -> Self {
        Self {
            id: profile.id,
            name: profile.name.clone(),
        }
    }
}

impl SavedAccounts {
    pub fn includes(&self, id: Uuid) -> bool {
        self.accounts.iter().any(|profile| profile.id == id)
    }

    pub fn upsert(&mut self, profile: SavedProfile) -> Result<(), ()> {
        if let Some(existing) = self
            .accounts
            .iter_mut()
            .find(|existing| existing.id == profile.id)
        {
            *existing = profile;
        } else if self.accounts.len() < ACCOUNT_LIMIT {
            self.accounts.push(profile);
        } else {
            return Err(());
        }
        Ok(())
    }

    pub fn remove(&mut self, id: Uuid) {
        self.accounts.retain(|profile| profile.id != id);
        if self.active == Some(id) {
            self.active = self.accounts.first().map(|profile| profile.id);
        }
    }

    fn validate(&self) -> Result<(), ()> {
        if self.accounts.len() > ACCOUNT_LIMIT || self.active.is_some_and(|id| !self.includes(id)) {
            return Err(());
        }
        for (index, profile) in self.accounts.iter().enumerate() {
            if profile.id.is_nil()
                || !(1..=16).contains(&profile.name.len())
                || !profile
                    .name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                || self.accounts[..index]
                    .iter()
                    .any(|previous| previous.id == profile.id)
            {
                return Err(());
            }
        }
        Ok(())
    }
}

pub(super) struct Vault {
    root: PathBuf,
    client_id: Uuid,
    credentials: Box<dyn Credentials>,
}

trait Credentials: Send + Sync {
    fn read(&self, slot: &str) -> Result<Option<Secret>, ()>;
    fn write(&self, slot: &str, token: &str) -> Result<(), ()>;
    fn clear(&self, slot: &str) -> Result<(), ()>;
}

struct NativeCredentials;

struct LockedVault<'a> {
    _lock: File,
    registry: PathBuf,
    namespace: String,
    credentials: &'a dyn Credentials,
}

impl Vault {
    pub(super) fn new(root: &Path, client_id: Uuid) -> Self {
        Self {
            root: root.to_owned(),
            client_id,
            credentials: Box::new(NativeCredentials),
        }
    }

    /// Test-only credential backend. It never reaches the OS store and lives
    /// beside the guarded development fixture root. Tokens it holds are
    /// synthetic and have no provider or launch authority.
    #[cfg(feature = "dev-simulation")]
    pub(super) fn simulated(root: &Path, client_id: Uuid) -> Self {
        Self {
            root: root.to_owned(),
            client_id,
            credentials: Box::new(SimulatedCredentials::new(root)),
        }
    }

    fn lock(&self) -> Result<LockedVault<'_>, ()> {
        fs::create_dir_all(&self.root).map_err(|_| ())?;
        let identity = self.root.canonicalize().map_err(|_| ())?;
        let mut digest = Sha256::new();
        digest.update(self.client_id.as_bytes());
        digest.update(identity.as_os_str().as_encoded_bytes());
        let namespace = hex::encode(digest.finalize());
        let lock = File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(identity.join("account-vault.lock"))
            .map_err(|_| ())?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match lock.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(25));
                }
                Err(_) => return Err(()),
            }
        }
        let registry = identity.join(format!("accounts-{}.json", self.client_id.simple()));
        Ok(LockedVault {
            _lock: lock,
            registry,
            namespace,
            credentials: self.credentials.as_ref(),
        })
    }

    pub(super) fn load(&self) -> Result<SavedAccounts, ()> {
        self.lock()?.load()
    }

    pub(super) fn read(&self, id: Uuid) -> Result<Option<Secret>, ()> {
        let locked = self.lock()?;
        if !locked.load()?.includes(id) {
            return Ok(None);
        }
        locked.credentials.read(&locked.slot(id))
    }

    /// Record public ownership of the slot first. A crash can leave a row without
    /// a credential, which cannot authenticate and can still be removed explicitly.
    pub(super) fn add(&self, profile: SavedProfile, token: &str) -> Result<SavedAccounts, ()> {
        let locked = self.lock()?;
        let previous = locked.load()?;
        let mut updated = previous.clone();
        let slot = locked.slot(profile.id);
        updated.active = Some(profile.id);
        updated.upsert(profile)?;
        locked.save(&updated)?;
        if locked.credentials.write(&slot, token).is_err() {
            let _ = locked.save(&previous);
            return Err(());
        }
        Ok(updated)
    }

    /// Update only the credential that was used for verification.
    pub(super) fn update(
        &self,
        profile: SavedProfile,
        expected: &str,
        token: &str,
        activate: bool,
        required_active: Option<Uuid>,
    ) -> Result<Option<SavedAccounts>, ()> {
        let locked = self.lock()?;
        let mut updated = locked.load()?;
        if !updated.includes(profile.id)
            || required_active.is_some_and(|id| updated.active != Some(id))
        {
            return Ok(None);
        }
        let slot = locked.slot(profile.id);
        let current = locked.credentials.read(&slot)?;
        if current.as_ref().map(|current| current.as_str()) != Some(expected) {
            return Ok(None);
        }
        if activate {
            updated.active = Some(profile.id);
        }
        if token != expected {
            locked.credentials.write(&slot, token)?;
        }
        updated.upsert(profile)?;
        locked.save(&updated)?;
        Ok(Some(updated))
    }

    pub(super) fn select(&self, id: Uuid) -> Result<Option<SavedAccounts>, ()> {
        let locked = self.lock()?;
        let mut updated = locked.load()?;
        if !updated.includes(id) {
            return Ok(None);
        }
        updated.active = Some(id);
        locked.save(&updated)?;
        Ok(Some(updated))
    }

    /// Remove the OS credential before publishing removal. A failed deletion keeps
    /// the row visible and actionable, and a missing credential is recoverable.
    pub(super) fn remove(
        &self,
        id: Uuid,
        expected: Option<&str>,
    ) -> Result<Option<SavedAccounts>, ()> {
        let locked = self.lock()?;
        let mut updated = locked.load()?;
        if updated.includes(id) {
            let slot = locked.slot(id);
            if let Some(expected) = expected {
                let current = locked.credentials.read(&slot)?;
                if current.as_ref().map(|current| current.as_str()) != Some(expected) {
                    return Ok(None);
                }
            }
            locked.credentials.clear(&slot)?;
            updated.remove(id);
            locked.save(&updated)?;
        }
        Ok(Some(updated))
    }
}

impl LockedVault<'_> {
    fn slot(&self, id: Uuid) -> String {
        format!("{}-{}", self.namespace, id.simple())
    }

    fn load(&self) -> Result<SavedAccounts, ()> {
        let file = match File::open(&self.registry) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(SavedAccounts::default())
            }
            Err(_) => return Err(()),
        };
        let mut bytes = Vec::new();
        file.take(METADATA_LIMIT + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ())?;
        if bytes.len() as u64 > METADATA_LIMIT {
            return Err(());
        }
        let accounts: SavedAccounts = serde_json::from_slice(&bytes).map_err(|_| ())?;
        accounts.validate()?;
        Ok(accounts)
    }

    fn save(&self, accounts: &SavedAccounts) -> Result<(), ()> {
        accounts.validate()?;
        let parent = self.registry.parent().ok_or(())?;
        let mut next = tempfile::NamedTempFile::new_in(parent).map_err(|_| ())?;
        serde_json::to_writer(next.as_file_mut(), accounts).map_err(|_| ())?;
        next.as_file().sync_all().map_err(|_| ())?;
        next.persist(&self.registry).map_err(|_| ())?;
        #[cfg(unix)]
        File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| ())?;
        Ok(())
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
impl Credentials for NativeCredentials {
    fn read(&self, slot: &str) -> Result<Option<Secret>, ()> {
        let entry = native_entry(slot)?;
        match entry.get_secret() {
            Ok(bytes) => {
                let bytes = Zeroizing::new(bytes);
                let value = std::str::from_utf8(&bytes).map_err(|_| ())?;
                Ok(Some(Zeroizing::new(value.to_owned())))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(()),
        }
    }

    fn write(&self, slot: &str, token: &str) -> Result<(), ()> {
        native_entry(slot)?
            .set_secret(token.as_bytes())
            .map_err(|_| ())
    }

    fn clear(&self, slot: &str) -> Result<(), ()> {
        match native_entry(slot)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(()),
        }
    }
}

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
fn native_entry(slot: &str) -> Result<keyring::Entry, ()> {
    keyring::Entry::new("Carbon Desktop Microsoft account", slot).map_err(|_| ())
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
impl Credentials for NativeCredentials {
    fn read(&self, _: &str) -> Result<Option<Secret>, ()> {
        Err(())
    }
    fn write(&self, _: &str, _: &str) -> Result<(), ()> {
        Err(())
    }
    fn clear(&self, _: &str) -> Result<(), ()> {
        Err(())
    }
}

#[cfg(feature = "dev-simulation")]
const SIMULATED_CREDENTIALS_FILE: &str = "synthetic-credentials.json";
#[cfg(feature = "dev-simulation")]
const SIMULATED_CREDENTIALS_KIND: &str = "carbon-desktop-dev-account-simulation";
#[cfg(feature = "dev-simulation")]
const SIMULATED_CREDENTIALS_LIMIT: u64 = 256 * 1024;

#[cfg(feature = "dev-simulation")]
fn simulated_kind() -> String {
    SIMULATED_CREDENTIALS_KIND.to_owned()
}

#[cfg(feature = "dev-simulation")]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SimulatedCredentialFile {
    #[serde(default = "simulated_kind")]
    kind: String,
    #[serde(default)]
    credentials: BTreeMap<String, String>,
}

/// File-backed synthetic credentials used only by the developer simulation.
/// The file is inside the guarded fixture root and is never the OS vault.
#[cfg(feature = "dev-simulation")]
struct SimulatedCredentials {
    path: PathBuf,
}

#[cfg(feature = "dev-simulation")]
impl SimulatedCredentials {
    fn new(root: &Path) -> Self {
        Self {
            path: root.join(SIMULATED_CREDENTIALS_FILE),
        }
    }

    fn load(&self) -> Result<SimulatedCredentialFile, ()> {
        let metadata = match fs::symlink_metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(SimulatedCredentialFile {
                    kind: simulated_kind(),
                    credentials: BTreeMap::new(),
                });
            }
            Err(_) => return Err(()),
        };
        if !metadata.is_file()
            || metadata.is_symlink()
            || metadata.len() > SIMULATED_CREDENTIALS_LIMIT
        {
            return Err(());
        }
        let bytes = fs::read(&self.path).map_err(|_| ())?;
        if bytes.len() as u64 > SIMULATED_CREDENTIALS_LIMIT {
            return Err(());
        }
        let file: SimulatedCredentialFile = serde_json::from_slice(&bytes).map_err(|_| ())?;
        if file.kind != SIMULATED_CREDENTIALS_KIND {
            return Err(());
        }
        Ok(file)
    }

    fn store(&self, mut file: SimulatedCredentialFile) -> Result<(), ()> {
        file.kind = simulated_kind();
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|_| ())?;
        }
        let bytes = serde_json::to_vec(&file).map_err(|_| ())?;
        if bytes.len() as u64 > SIMULATED_CREDENTIALS_LIMIT {
            return Err(());
        }
        fs::write(&self.path, bytes).map_err(|_| ())
    }
}

#[cfg(feature = "dev-simulation")]
impl Credentials for SimulatedCredentials {
    fn read(&self, slot: &str) -> Result<Option<Secret>, ()> {
        Ok(self
            .load()?
            .credentials
            .get(slot)
            .map(|token| Secret::new(token.clone())))
    }

    fn write(&self, slot: &str, token: &str) -> Result<(), ()> {
        let mut file = self.load()?;
        file.credentials.insert(slot.into(), token.into());
        self.store(file)
    }

    fn clear(&self, slot: &str) -> Result<(), ()> {
        let mut file = self.load()?;
        if file.credentials.remove(slot).is_some() {
            self.store(file)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "vault_tests.rs"]
mod tests;

#[cfg(test)]
#[derive(Clone, Default)]
pub(super) struct MemoryCredentials(
    std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<String, Secret>>>,
);

#[cfg(test)]
impl MemoryCredentials {
    pub(super) fn vault(&self, root: &Path, client_id: Uuid) -> Vault {
        Vault {
            root: root.to_owned(),
            client_id,
            credentials: Box::new(self.clone()),
        }
    }
}

#[cfg(test)]
impl Credentials for MemoryCredentials {
    fn read(&self, slot: &str) -> Result<Option<Secret>, ()> {
        Ok(self.0.lock().unwrap().get(slot).cloned())
    }

    fn write(&self, slot: &str, token: &str) -> Result<(), ()> {
        self.0
            .lock()
            .unwrap()
            .insert(slot.into(), Secret::new(token.to_owned()));
        Ok(())
    }

    fn clear(&self, slot: &str) -> Result<(), ()> {
        self.0.lock().unwrap().remove(slot);
        Ok(())
    }
}
