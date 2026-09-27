//! The devices someone chose to always allow.
//!
//! Two parts, so the session can read what it needs without root:
//!
//! - `allowed.toml`, readable by everyone: each device's UUID, and its vendor
//!   and name as it gave them, for Settings to show.
//! - `keys/<uuid>`, readable by root alone: the key stored in a device that
//!   can prove who it is, which is worth nothing once anyone else has read it.
//!
//! Only `alpymist-thunderbolt-helper`, as root, writes either.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Where it is kept, under the root.
pub const DIR: &str = "var/lib/alpymist/thunderbolt";

/// A device allowed always.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Allowed {
    /// The UUID it reported.
    pub uuid: String,
    /// Its vendor, as it said then.
    #[serde(default)]
    pub vendor: String,
    /// Its name, as it said then.
    #[serde(default)]
    pub model: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default, rename = "device")]
    devices: Vec<Allowed>,
}

/// The store under a root.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    /// The store under `root` (`/` on a real system).
    #[must_use]
    pub fn new(root: &Path) -> Self {
        Self {
            dir: root.join(DIR),
        }
    }

    fn list_path(&self) -> PathBuf {
        self.dir.join("allowed.toml")
    }

    fn key_path(&self, uuid: &str) -> PathBuf {
        self.dir.join("keys").join(uuid)
    }

    /// Every device allowed always. A missing or unreadable file is none.
    #[must_use]
    pub fn allowed(&self) -> Vec<Allowed> {
        std::fs::read_to_string(self.list_path())
            .ok()
            .and_then(|s| toml::from_str::<File>(&s).ok())
            .map(|f| f.devices)
            .unwrap_or_default()
    }

    /// The device with `uuid`, if it is allowed always.
    #[must_use]
    pub fn get(&self, uuid: &str) -> Option<Allowed> {
        self.allowed().into_iter().find(|a| a.uuid == uuid)
    }

    /// The key stored for `uuid`: root only.
    #[must_use]
    pub fn key(&self, uuid: &str) -> Option<String> {
        std::fs::read_to_string(self.key_path(uuid))
            .ok()
            .map(|k| k.trim().to_owned())
            .filter(|k| valid_key(k))
    }

    /// Allow `device` always, with the key stored in it, if it took one.
    ///
    /// # Errors
    /// The files could not be written.
    pub fn add(&self, device: Allowed, key: Option<&str>) -> Result<(), String> {
        let mut list = self.allowed();
        list.retain(|a| a.uuid != device.uuid);
        let uuid = device.uuid.clone();
        list.push(device);
        self.write_list(&list)?;
        let path = self.key_path(&uuid);
        match key {
            Some(k) => write_private(&path, k),
            None => remove(&path),
        }
    }

    /// Stop allowing `uuid`, and forget its key. Forgetting what was never
    /// allowed is not an error.
    ///
    /// # Errors
    /// The files could not be written.
    pub fn remove(&self, uuid: &str) -> Result<(), String> {
        let mut list = self.allowed();
        let before = list.len();
        list.retain(|a| a.uuid != uuid);
        if list.len() != before {
            self.write_list(&list)?;
        }
        remove(&self.key_path(uuid))
    }

    fn write_list(&self, list: &[Allowed]) -> Result<(), String> {
        let text = toml::to_string(&File {
            devices: list.to_vec(),
        })
        .map_err(|e| e.to_string())?;
        let header = "# Thunderbolt and USB4 devices allowed always. Written by\n\
                      # alpymist-thunderbolt-helper; change it with Settings or\n\
                      # `alpymist-thunderbolt forget UUID`.\n\n";
        write_atomic(&self.list_path(), &format!("{header}{text}"), 0o644)
    }
}

/// Whether `k` is a key as the kernel takes one: 64 hex digits, 32 bytes.
#[must_use]
pub fn valid_key(k: &str) -> bool {
    k.len() == 64 && k.chars().all(|c| c.is_ascii_hexdigit())
}

/// A fresh key from the kernel's random source.
///
/// # Errors
/// `/dev/urandom` could not be read.
pub fn new_key() -> Result<String, String> {
    use std::io::Read;
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| format!("/dev/urandom: {e}"))?;
    Ok(bytes.iter().fold(String::with_capacity(64), |mut hex, b| {
        use std::fmt::Write;
        let _ = write!(hex, "{b:02x}");
        hex
    }))
}

fn write_private(path: &Path, key: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        create_dir(dir, 0o700)?;
    }
    write_atomic(path, &format!("{key}\n"), 0o600)
}

fn remove(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("{}: {e}", path.display()))
        }
        _ => Ok(()),
    }
}

fn create_dir(dir: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(mode)
        .create(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))
}

/// Write `contents` to `path` with `mode` from the first byte, through a
/// temporary file renamed over it, so a reader never sees half a file and a
/// key is never readable by anyone else even for a moment.
fn write_atomic(path: &Path, contents: &str, mode: u32) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let dir = path.parent().ok_or("no directory")?;
    create_dir(dir, 0o755)?;
    let tmp = dir.join(format!(
        ".{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy()
    ));
    let err = |e: std::io::Error| format!("{}: {e}", path.display());
    std::fs::remove_file(&tmp).ok();
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&tmp)
        .map_err(err)?;
    file.write_all(contents.as_bytes()).map_err(err)?;
    file.sync_all().map_err(err)?;
    std::fs::rename(&tmp, path).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::{Allowed, Store, new_key, valid_key};
    use crate::sysfs::tests::{DOCK, Fake};
    use std::os::unix::fs::PermissionsExt;

    fn dock() -> Allowed {
        Allowed {
            uuid: DOCK.into(),
            vendor: "Lenovo".into(),
            model: "ThinkPad Thunderbolt 3 Dock".into(),
        }
    }

    #[test]
    fn an_empty_store_allows_nothing() {
        let f = Fake::new("store-empty");
        let s = Store::new(f.root());
        assert!(s.allowed().is_empty());
        assert_eq!(s.get(DOCK), None);
        assert_eq!(s.key(DOCK), None);
    }

    #[test]
    fn a_device_is_added_once_and_forgotten_with_its_key() {
        let f = Fake::new("store-add");
        let s = Store::new(f.root());
        let key = "ab".repeat(32);
        s.add(dock(), Some(&key)).unwrap();
        s.add(dock(), Some(&key)).unwrap();
        assert_eq!(s.allowed(), [dock()]);
        assert_eq!(s.key(DOCK), Some(key));

        let key_file = f.root().join(super::DIR).join("keys").join(DOCK);
        let mode = std::fs::metadata(&key_file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "a key is root's alone");
        let list = f.root().join(super::DIR).join("allowed.toml");
        let mode = std::fs::metadata(&list).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o644, "the list is for the session to read");
        assert!(!std::fs::read_to_string(&list).unwrap().contains("abab"));

        s.remove(DOCK).unwrap();
        assert!(s.allowed().is_empty());
        assert_eq!(s.key(DOCK), None);
        s.remove(DOCK).unwrap();
    }

    #[test]
    fn added_again_without_a_key_drops_the_old_one() {
        let f = Fake::new("store-rekey");
        let s = Store::new(f.root());
        s.add(dock(), Some(&"cd".repeat(32))).unwrap();
        s.add(dock(), None).unwrap();
        assert_eq!(s.key(DOCK), None);
    }

    #[test]
    fn keys_are_32_random_bytes_in_hex() {
        let a = new_key().unwrap();
        let b = new_key().unwrap();
        assert!(valid_key(&a));
        assert_ne!(a, b);
        assert!(!valid_key("abc"));
        assert!(!valid_key(&"zz".repeat(32)));
    }
}
