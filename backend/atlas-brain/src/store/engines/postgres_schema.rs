use crate::error::{AtlasError, AtlasResult};
use postgres::{Client, NoTls};
use std::sync::Mutex;

pub struct PostgresHandle {
    pub client: Mutex<Client>,
}

impl PostgresHandle {
    pub fn connect(url: &str) -> AtlasResult<Self> {
        let client = Client::connect(url, NoTls)
            .map_err(|e| AtlasError::Upstream(format!("postgres connect: {e}")))?;
        Ok(Self {
            client: Mutex::new(client),
        })
    }

    pub fn execute(&self, sql: &str, params: &[&(dyn postgres::types::ToSql + Sync)]) -> AtlasResult<u64> {
        let mut client = self.client.lock().expect("pg lock");
        let n = client
            .execute(sql, params)
            .map_err(|e| AtlasError::Upstream(format!("postgres execute: {e}")))?;
        Ok(n)
    }
}

/// Full logical schema for Atlas on Postgres (+ pgvector when available).
pub fn migrate_postgres(handle: &PostgresHandle) -> AtlasResult<()> {
    let mut client = handle.client.lock().expect("pg lock");
    // pgvector is best-effort — schema still works with BYTEA vectors if extension missing.
    let _ = client.batch_execute("CREATE EXTENSION IF NOT EXISTS vector");
    client
        .batch_execute(POSTGRES_DDL)
        .map_err(|e| AtlasError::Upstream(format!("postgres migrate: {e}")))?;
    Ok(())
}

const POSTGRES_DDL: &str = r#"
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    messages_json TEXT NOT NULL,
    project_id TEXT,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    seq INT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    callout_json TEXT,
    created_at TIMESTAMPTZ,
    trace_id TEXT,
    model_id TEXT,
    token_in INT,
    token_out INT,
    latency_ms INT
);
CREATE INDEX IF NOT EXISTS idx_pg_messages_conv ON messages(conversation_id, seq);

CREATE TABLE IF NOT EXISTS knowledge (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    tags_json TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_nodes (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    tags_json TEXT NOT NULL,
    source TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    parent_ids_json TEXT NOT NULL,
    confidence DOUBLE PRECISION NOT NULL,
    scope TEXT NOT NULL,
    conversation_id TEXT,
    project_id TEXT,
    salience DOUBLE PRECISION NOT NULL,
    decay DOUBLE PRECISION NOT NULL,
    archived_at TIMESTAMPTZ,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_pg_memory_kind ON memory_nodes(kind);

CREATE TABLE IF NOT EXISTS memory_embeddings (
    node_id TEXT PRIMARY KEY REFERENCES memory_nodes(id) ON DELETE CASCADE,
    dim INT NOT NULL,
    vector_json BYTEA NOT NULL,
    embedding_model_id TEXT,
    stale INT NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS working_memory (
    conversation_id TEXT PRIMARY KEY,
    state_json TEXT NOT NULL,
    query_embedding BYTEA,
    updated_at TIMESTAMPTZ NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS memory_edges (
    id TEXT PRIMARY KEY,
    src_id TEXT NOT NULL,
    rel TEXT NOT NULL,
    dst_id TEXT NOT NULL,
    weight DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    provenance_json TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE(src_id, rel, dst_id)
);

CREATE TABLE IF NOT EXISTS traces (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    conversation_id TEXT,
    model_id TEXT,
    payload_json TEXT NOT NULL,
    token_in INT,
    token_out INT,
    latency_ms INT,
    cost_usd DOUBLE PRECISION,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS outbox (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    processed_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    source TEXT NOT NULL,
    media_type TEXT NOT NULL,
    object_hash TEXT NOT NULL,
    metadata_json TEXT NOT NULL,
    project_id TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS document_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    seq INT NOT NULL,
    content TEXT NOT NULL,
    memory_node_id TEXT,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS tools (
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    description TEXT NOT NULL,
    schema_json TEXT NOT NULL,
    policy TEXT NOT NULL,
    capabilities_json TEXT NOT NULL,
    enabled INT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY(name, version)
);

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    status TEXT NOT NULL,
    conversation_id TEXT,
    parent_id TEXT,
    project_id TEXT,
    plan_json TEXT NOT NULL,
    state_json TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS task_checkpoints (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    snapshot_json TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS prompt_versions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    version INT NOT NULL,
    content TEXT NOT NULL,
    model_id TEXT,
    active INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE(name, version)
);
CREATE TABLE IF NOT EXISTS model_registry (
    id TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    label TEXT NOT NULL,
    meta_json TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS eval_cases (
    id TEXT PRIMARY KEY,
    suite TEXT NOT NULL,
    name TEXT NOT NULL,
    query TEXT NOT NULL,
    expect_json TEXT NOT NULL,
    tags_json TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS eval_runs (
    id TEXT PRIMARY KEY,
    suite TEXT NOT NULL,
    case_id TEXT NOT NULL,
    score DOUBLE PRECISION NOT NULL,
    passed INT NOT NULL,
    trace_id TEXT,
    detail_json TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS feedback (
    id TEXT PRIMARY KEY,
    trace_id TEXT,
    conversation_id TEXT,
    kind TEXT NOT NULL,
    note TEXT NOT NULL,
    correction TEXT,
    created_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS training_examples (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    input TEXT NOT NULL,
    output TEXT NOT NULL,
    trace_id TEXT,
    meta_json TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS semantic_cache (
    cache_key TEXT PRIMARY KEY,
    response TEXT NOT NULL,
    model_id TEXT NOT NULL,
    hit_count INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

-- Gap stores (21–25, secrets, code)
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL DEFAULT '',
    meta_json TEXT NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT PRIMARY KEY,
    project_id TEXT,
    conversation_id TEXT,
    task_id TEXT,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    object_hash TEXT,
    content TEXT,
    meta_json TEXT NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS cost_ledger (
    id TEXT PRIMARY KEY,
    trace_id TEXT,
    conversation_id TEXT,
    model_id TEXT,
    token_in INT NOT NULL DEFAULT 0,
    token_out INT NOT NULL DEFAULT 0,
    latency_ms INT NOT NULL DEFAULT 0,
    cost_usd DOUBLE PRECISION NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS secrets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    ciphertext TEXT NOT NULL,
    nonce TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS code_files (
    id TEXT PRIMARY KEY,
    project_id TEXT,
    path TEXT NOT NULL,
    language TEXT NOT NULL DEFAULT '',
    content_hash TEXT NOT NULL,
    size_bytes INT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE(project_id, path)
);
CREATE TABLE IF NOT EXISTS code_symbols (
    id TEXT PRIMARY KEY,
    file_id TEXT NOT NULL REFERENCES code_files(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    line_start INT NOT NULL,
    line_end INT NOT NULL,
    signature TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS idx_pg_code_symbols_name ON code_symbols(name);

-- Datasets plane (metadata; bulk Parquet/JSONL blijft object/filesystem)
CREATE TABLE IF NOT EXISTS datasets (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    hub_id TEXT NOT NULL,
    revision TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    license TEXT NOT NULL DEFAULT '',
    modality TEXT NOT NULL DEFAULT 'text',
    gated BOOLEAN NOT NULL DEFAULT FALSE,
    root_hash TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'draft',
    project_id TEXT,
    meta_json JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE(source, hub_id, revision)
);
CREATE TABLE IF NOT EXISTS dataset_jobs (
    id TEXT PRIMARY KEY,
    dataset_id TEXT NOT NULL,
    recipe_id TEXT,
    kind TEXT NOT NULL,
    status TEXT NOT NULL,
    phase TEXT NOT NULL,
    checkpoint_json JSONB NOT NULL DEFAULT '{}',
    budget_json JSONB NOT NULL DEFAULT '{}',
    metrics_json JSONB NOT NULL DEFAULT '{}',
    paused BOOLEAN NOT NULL DEFAULT FALSE,
    task_id TEXT,
    trace_id TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS dataset_recipes (
    id TEXT PRIMARY KEY,
    dataset_id TEXT NOT NULL,
    version TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    body_json JSONB NOT NULL,
    sample_report_json JSONB NOT NULL DEFAULT '{}',
    verified BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS dataset_edges (
    id TEXT PRIMARY KEY,
    src TEXT NOT NULL,
    dst TEXT NOT NULL,
    rel TEXT NOT NULL,
    score DOUBLE PRECISION NOT NULL DEFAULT 0,
    provenance_json JSONB NOT NULL DEFAULT '{}',
    auditor_status TEXT NOT NULL DEFAULT 'unreviewed',
    created_at TIMESTAMPTZ NOT NULL
);

-- FTS via generated tsvector columns
ALTER TABLE memory_nodes ADD COLUMN IF NOT EXISTS fts tsvector
    GENERATED ALWAYS AS (to_tsvector('simple', coalesce(title,'') || ' ' || coalesce(content,''))) STORED;
CREATE INDEX IF NOT EXISTS idx_pg_memory_fts ON memory_nodes USING GIN (fts);
ALTER TABLE messages ADD COLUMN IF NOT EXISTS fts tsvector
    GENERATED ALWAYS AS (to_tsvector('simple', coalesce(content,''))) STORED;
CREATE INDEX IF NOT EXISTS idx_pg_messages_fts ON messages USING GIN (fts);
"#;
