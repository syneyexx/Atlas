use crate::error::AtlasResult;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use super::db::Store;

impl Store {
    pub fn objects_dir(data_dir: &Path) -> PathBuf {
        data_dir.join("objects")
    }

    pub fn content_hash(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    }

    /// Content-addressed put. Returns (hash, path, newly_written).
    pub fn put_object(
        &self,
        data_dir: &Path,
        bytes: &[u8],
    ) -> AtlasResult<(String, PathBuf, bool)> {
        let hash = Self::content_hash(bytes);
        let dir = Self::objects_dir(data_dir).join(&hash[..2.min(hash.len())]);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(&hash);
        if path.exists() {
            return Ok((hash, path, false));
        }
        std::fs::write(&path, bytes)?;
        Ok((hash, path, true))
    }

    #[allow(dead_code)]
    pub fn get_object_path(&self, data_dir: &Path, hash: &str) -> Option<PathBuf> {
        if hash.len() < 2 {
            return None;
        }
        let path = Self::objects_dir(data_dir).join(&hash[..2]).join(hash);
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn read_object(&self, data_dir: &Path, hash: &str) -> AtlasResult<Option<Vec<u8>>> {
        match self.get_object_path(data_dir, hash) {
            Some(path) => Ok(Some(std::fs::read(path)?)),
            None => Ok(None),
        }
    }
}
