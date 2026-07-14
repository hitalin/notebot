//! bot 用の小さな KV ストア (JSON ファイル + atomic rename)。
//!
//! SQLite にしない理由: rusqlite (libsqlite3-sys) は Cargo の `links` 制約で
//! 依存グラフに 1 バージョンしか共存できず、notecli とのロックステップを
//! 強いられる。bot の状態 (last-seen id、カウンタ程度) に SQLite は過剰。
//! 書き込みは同期 I/O だが、対象が小さいため async 文脈でも許容する。

use std::path::PathBuf;
use std::sync::Mutex;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::error::{NotebotError, Result};

pub struct Store {
    path: PathBuf,
    data: Mutex<Map<String, Value>>,
}

impl Store {
    pub(crate) fn open(path: PathBuf) -> Result<Self> {
        let data = match std::fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| {
                NotebotError::Store(format!("corrupt store file {}: {e}", path.display()))
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Map::new(),
            Err(e) => {
                return Err(NotebotError::Store(format!(
                    "failed to read {}: {e}",
                    path.display()
                )))
            }
        };
        Ok(Self {
            path,
            data: Mutex::new(data),
        })
    }

    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let data = self.data.lock().expect("store lock poisoned");
        match data.get(key) {
            Some(v) => Ok(Some(serde_json::from_value(v.clone()).map_err(|e| {
                NotebotError::Store(format!("key {key}: type mismatch: {e}"))
            })?)),
            None => Ok(None),
        }
    }

    pub fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        let v = serde_json::to_value(value)
            .map_err(|e| NotebotError::Store(format!("key {key}: {e}")))?;
        let mut data = self.data.lock().expect("store lock poisoned");
        data.insert(key.to_string(), v);
        self.persist(&data)
    }

    pub fn delete(&self, key: &str) -> Result<()> {
        let mut data = self.data.lock().expect("store lock poisoned");
        if data.remove(key).is_some() {
            self.persist(&data)?;
        }
        Ok(())
    }

    /// tmp に書いて rename — クラッシュしても壊れたファイルを残さない。
    fn persist(&self, data: &Map<String, Value>) -> Result<()> {
        let io_err = |e: std::io::Error| {
            NotebotError::Store(format!("failed to write {}: {e}", self.path.display()))
        };
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(io_err)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let body = serde_json::to_vec_pretty(&Value::Object(data.clone()))
            .map_err(|e| NotebotError::Store(e.to_string()))?;
        std::fs::write(&tmp, body).map_err(io_err)?;
        std::fs::rename(&tmp, &self.path).map_err(io_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_values() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("store.json")).unwrap();
        store.set("count", &42u32).unwrap();
        store.set("name", &"alice").unwrap();
        assert_eq!(store.get::<u32>("count").unwrap(), Some(42));
        assert_eq!(store.get::<String>("name").unwrap(), Some("alice".into()));
        assert_eq!(store.get::<u32>("missing").unwrap(), None);
    }

    #[test]
    fn persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.json");
        Store::open(path.clone()).unwrap().set("k", &"v").unwrap();
        let reopened = Store::open(path).unwrap();
        assert_eq!(reopened.get::<String>("k").unwrap(), Some("v".into()));
    }

    #[test]
    fn deletes_keys() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("store.json")).unwrap();
        store.set("k", &1).unwrap();
        store.delete("k").unwrap();
        assert_eq!(store.get::<i32>("k").unwrap(), None);
        store.delete("k").unwrap(); // 冪等
    }

    #[test]
    fn creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("a/b/store.json")).unwrap();
        store.set("k", &1).unwrap();
        assert!(dir.path().join("a/b/store.json").exists());
    }

    #[test]
    fn corrupt_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("store.json");
        std::fs::write(&path, "not json").unwrap();
        assert!(matches!(Store::open(path), Err(NotebotError::Store(_))));
    }
}
