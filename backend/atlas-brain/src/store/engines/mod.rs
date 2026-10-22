//! Multi-engine runtime (M5): Postgres durability, Redis hot path, S3/MinIO objects.
pub mod pg_mirror;
mod object_s3;
mod postgres_schema;
mod redis_hot;

pub use object_s3::ObjectBackend;
pub use postgres_schema::{migrate_postgres, PostgresHandle};
pub use redis_hot::RedisHot;

use crate::config::BrainConfig;
use crate::error::AtlasResult;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbBackend {
    Sqlite,
    Postgres,
    Dual,
}

impl DbBackend {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "postgres" | "postgresql" | "pg" => Self::Postgres,
            "dual" => Self::Dual,
            _ => Self::Sqlite,
        }
    }
}

pub struct EngineBundle {
    pub db_backend: DbBackend,
    pub postgres: Option<PostgresHandle>,
    pub redis: Option<RedisHot>,
    pub objects: ObjectBackend,
    pub cache_ttl_secs: u64,
}

impl EngineBundle {
    pub fn from_config(cfg: &BrainConfig) -> AtlasResult<Self> {
        let db_backend = DbBackend::parse(&cfg.db_backend);
        let postgres = if !cfg.database_url.trim().is_empty()
            && matches!(db_backend, DbBackend::Postgres | DbBackend::Dual)
        {
            let handle = PostgresHandle::connect(&cfg.database_url)?;
            migrate_postgres(&handle)?;
            tracing::info!(backend = ?db_backend, "Postgres engine ready");
            Some(handle)
        } else {
            None
        };

        let redis = if !cfg.redis_url.trim().is_empty() {
            match RedisHot::connect(&cfg.redis_url) {
                Ok(r) => {
                    tracing::info!("Redis hot path ready");
                    Some(r)
                }
                Err(err) => {
                    tracing::error!("Redis connect failed: {err}");
                    return Err(err);
                }
            }
        } else {
            None
        };

        let objects = ObjectBackend::from_config(cfg)?;
        Ok(Self {
            db_backend,
            postgres,
            redis,
            objects,
            cache_ttl_secs: cfg.semantic_cache_ttl_secs.max(60),
        })
    }

    pub fn status_json(&self) -> serde_json::Value {
        serde_json::json!({
            "db_backend": match self.db_backend {
                DbBackend::Sqlite => "sqlite",
                DbBackend::Postgres => "postgres",
                DbBackend::Dual => "dual",
            },
            "postgres": self.postgres.is_some(),
            "redis": self.redis.is_some(),
            "objects": self.objects.kind_name(),
            "cache_ttl_secs": self.cache_ttl_secs,
        })
    }
}
