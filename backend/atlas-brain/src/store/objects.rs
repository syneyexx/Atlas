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

    /// Content-addressed put via configured object backend (filesystem or S3/MinIO).
    pub fn put_object(
        &self,
        _data_dir: &Path,
        bytes: &[u8],
    ) -> AtlasResult<(String, PathBuf, bool)> {
        let hash = Self::content_hash(bytes);
        let (path, newly) = self.engines.objects.put(&hash, bytes)?;
        Ok((hash, path, newly))
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

    pub fn read_object(&self, _data_dir: &Path, hash: &str) -> AtlasResult<Option<Vec<u8>>> {
        self.engines.objects.get(hash)
    }
}
