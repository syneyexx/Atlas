use crate::config::BrainConfig;
use crate::error::{AtlasError, AtlasResult};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone)]
pub enum ObjectBackend {
    Filesystem {
        root: PathBuf,
    },
    S3 {
        endpoint: String,
        bucket: String,
        region: String,
        access_key: String,
        secret_key: String,
        /// Local mirror for fast read-after-write / offline.
        mirror_root: PathBuf,
    },
}

impl ObjectBackend {
    pub fn from_config(cfg: &BrainConfig) -> AtlasResult<Self> {
        let kind = cfg.object_backend.trim().to_lowercase();
        if kind == "s3" || kind == "minio" {
            if cfg.s3_endpoint.trim().is_empty()
                || cfg.s3_bucket.trim().is_empty()
                || cfg.s3_access_key.trim().is_empty()
            {
                return Err(AtlasError::BadRequest(
                    "object_backend=s3 vereist s3_endpoint, s3_bucket, s3_access_key, s3_secret_key"
                        .into(),
                ));
            }
            let mirror = cfg.data_dir.join("objects");
            std::fs::create_dir_all(&mirror)?;
            Ok(Self::S3 {
                endpoint: cfg.s3_endpoint.trim_end_matches('/').to_string(),
                bucket: cfg.s3_bucket.clone(),
                region: if cfg.s3_region.trim().is_empty() {
                    "us-east-1".into()
                } else {
                    cfg.s3_region.clone()
                },
                access_key: cfg.s3_access_key.clone(),
                secret_key: cfg.s3_secret_key.clone(),
                mirror_root: mirror,
            })
        } else {
            let root = cfg.data_dir.join("objects");
            std::fs::create_dir_all(&root)?;
            Ok(Self::Filesystem { root })
        }
    }

    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Filesystem { .. } => "filesystem",
            Self::S3 { .. } => "s3",
        }
    }

    pub fn put(&self, hash: &str, bytes: &[u8]) -> AtlasResult<(PathBuf, bool)> {
        match self {
            Self::Filesystem { root } => put_fs(root, hash, bytes),
            Self::S3 {
                endpoint,
                bucket,
                region,
                access_key,
                secret_key,
                mirror_root,
            } => {
                let (path, newly) = put_fs(mirror_root, hash, bytes)?;
                s3_put(
                    endpoint,
                    bucket,
                    region,
                    access_key,
                    secret_key,
                    hash,
                    bytes,
                )?;
                Ok((path, newly))
            }
        }
    }

    pub fn get(&self, hash: &str) -> AtlasResult<Option<Vec<u8>>> {
        match self {
            Self::Filesystem { root } => Ok(read_fs(root, hash)),
            Self::S3 {
                endpoint,
                bucket,
                region,
                access_key,
                secret_key,
                mirror_root,
            } => {
                if let Some(bytes) = read_fs(mirror_root, hash) {
                    return Ok(Some(bytes));
                }
                match s3_get(endpoint, bucket, region, access_key, secret_key, hash)? {
                    Some(bytes) => {
                        let _ = put_fs(mirror_root, hash, &bytes)?;
                        Ok(Some(bytes))
                    }
                    None => Ok(None),
                }
            }
        }
    }
}

fn object_path(root: &Path, hash: &str) -> PathBuf {
    root.join(&hash[..2.min(hash.len())]).join(hash)
}

fn put_fs(root: &Path, hash: &str, bytes: &[u8]) -> AtlasResult<(PathBuf, bool)> {
    let path = object_path(root, hash);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.exists() {
        return Ok((path, false));
    }
    std::fs::write(&path, bytes)?;
    Ok((path, true))
}

fn read_fs(root: &Path, hash: &str) -> Option<Vec<u8>> {
    let path = object_path(root, hash);
    std::fs::read(path).ok()
}

fn s3_put(
    endpoint: &str,
    bucket: &str,
    region: &str,
    access_key: &str,
    secret_key: &str,
    key: &str,
    bytes: &[u8],
) -> AtlasResult<()> {
    let url = format!("{endpoint}/{bucket}/{key}");
    let amz_date = amz_date();
    let date_stamp = &amz_date[..8];
    let payload_hash = hex::encode(Sha256::digest(bytes));
    let host = host_of(endpoint)?;

    let canonical_headers = format!(
        "content-type:application/octet-stream\nhost:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
    );
    let signed_headers = "content-type;host;x-amz-content-sha256;x-amz-date";
    let canonical_request = format!(
        "PUT\n/{bucket}/{key}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );
    let credential_scope = format!("{date_stamp}/{region}/s3/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
        hex::encode(Sha256::digest(canonical_request.as_bytes()))
    );
    let signature = sign_string(secret_key, date_stamp, region, "s3", &string_to_sign)?;
    let auth = format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );

    let client = reqwest::blocking::Client::new();
    let res = client
        .put(&url)
        .header("Content-Type", "application/octet-stream")
        .header("x-amz-content-sha256", &payload_hash)
        .header("x-amz-date", &amz_date)
        .header("Authorization", auth)
        .header("Host", host)
        .body(bytes.to_vec())
        .send()
        .map_err(|e| AtlasError::Upstream(format!("s3 put: {e}")))?;
    if !res.status().is_success() {
        return Err(AtlasError::Upstream(format!(
            "s3 put HTTP {}: {}",
            res.status(),
            res.text().unwrap_or_default()
        )));
    }
    Ok(())
}

fn s3_get(
    endpoint: &str,
    bucket: &str,
    region: &str,
    access_key: &str,
    secret_key: &str,
    key: &str,
) -> AtlasResult<Option<Vec<u8>>> {
    let url = format!("{endpoint}/{bucket}/{key}");
    let amz_date = amz_date();
    let date_stamp = &amz_date[..8];
    let payload_hash = hex::encode(Sha256::digest([]));
    let host = host_of(endpoint)?;
    let canonical_headers =
        format!("host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n");
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical_request = format!(
        "GET\n/{bucket}/{key}\n\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );
    let credential_scope = format!("{date_stamp}/{region}/s3/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
        hex::encode(Sha256::digest(canonical_request.as_bytes()))
    );
    let signature = sign_string(secret_key, date_stamp, region, "s3", &string_to_sign)?;
    let auth = format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );

    let client = reqwest::blocking::Client::new();
    let res = client
        .get(&url)
        .header("x-amz-content-sha256", &payload_hash)
        .header("x-amz-date", &amz_date)
        .header("Authorization", auth)
        .header("Host", host)
        .send()
        .map_err(|e| AtlasError::Upstream(format!("s3 get: {e}")))?;
    if res.status().as_u16() == 404 {
        return Ok(None);
    }
    if !res.status().is_success() {
        return Err(AtlasError::Upstream(format!(
            "s3 get HTTP {}",
            res.status()
        )));
    }
    Ok(Some(
        res.bytes()
            .map_err(|e| AtlasError::Upstream(format!("s3 body: {e}")))?
            .to_vec(),
    ))
}

fn host_of(endpoint: &str) -> AtlasResult<String> {
    let u = endpoint.trim_start_matches("https://").trim_start_matches("http://");
    Ok(u.split('/').next().unwrap_or(u).to_string())
}

fn amz_date() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let dt = chrono::DateTime::from_timestamp(secs, 0).unwrap_or_else(chrono::Utc::now);
    dt.format("%Y%m%dT%H%M%SZ").to_string()
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> AtlasResult<Vec<u8>> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|e| AtlasError::Upstream(format!("hmac: {e}")))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn sign_string(
    secret: &str,
    date: &str,
    region: &str,
    service: &str,
    string_to_sign: &str,
) -> AtlasResult<String> {
    let k_date = hmac_sha256(format!("AWS4{secret}").as_bytes(), date.as_bytes())?;
    let k_region = hmac_sha256(&k_date, region.as_bytes())?;
    let k_service = hmac_sha256(&k_region, service.as_bytes())?;
    let k_signing = hmac_sha256(&k_service, b"aws4_request")?;
    let sig = hmac_sha256(&k_signing, string_to_sign.as_bytes())?;
    Ok(hex::encode(sig))
}
