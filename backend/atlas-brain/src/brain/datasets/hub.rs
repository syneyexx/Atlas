use crate::config::BrainConfig;
use crate::error::{AtlasError, AtlasResult};
use crate::store::{DatasetRecord, DatasetShard, Store};
use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Seed offline Hub cards into catalog cache (idempotent).
pub fn seed_stub_catalog(store: &Store) -> AtlasResult<usize> {
    let cards = stub_catalog_cards();
    let mut n = 0;
    for card in &cards {
        let hub_id = card.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if hub_id.is_empty() {
            continue;
        }
        if store.get_catalog_card("hf", hub_id)?.is_none() {
            store.upsert_catalog_card("hf", hub_id, card)?;
            n += 1;
        }
    }
    Ok(n)
}

pub fn stub_catalog_cards() -> Vec<Value> {
    vec![
        json!({
            "source": "hf",
            "id": "atlas/tiny-instruct",
            "title": "Atlas Tiny Instruct",
            "description": "Offline fixture: short instruction/response pairs for pipeline tests.",
            "license": "apache-2.0",
            "modality": "text",
            "gated": false,
            "tags": ["instruction", "english", "fixture"],
            "downloads": 42,
            "revision": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "features": {
                "instruction": {"dtype": "string"},
                "response": {"dtype": "string"},
                "label": {"dtype": "string"}
            }
        }),
        json!({
            "source": "hf",
            "id": "atlas/code-snippets",
            "title": "Atlas Code Snippets",
            "description": "Offline fixture: code + natural language pairs.",
            "license": "mit",
            "modality": "code",
            "gated": false,
            "tags": ["code", "python", "fixture"],
            "downloads": 17,
            "revision": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "features": {
                "prompt": {"dtype": "string"},
                "code": {"dtype": "string"},
                "language": {"dtype": "string"}
            }
        }),
        json!({
            "source": "hf",
            "id": "atlas/gated-demo",
            "title": "Atlas Gated Demo",
            "description": "Offline gated fixture — requires hf_token secret to acquire.",
            "license": "other",
            "modality": "text",
            "gated": true,
            "tags": ["gated", "fixture"],
            "downloads": 3,
            "revision": "cccccccccccccccccccccccccccccccccccccccc",
            "features": {
                "text": {"dtype": "string"},
                "title": {"dtype": "string"}
            }
        }),
    ]
}

pub fn search_hub(
    store: &Store,
    cfg: &BrainConfig,
    query: &str,
    modality: Option<&str>,
    license: Option<&str>,
    limit: usize,
) -> AtlasResult<Vec<Value>> {
    let _ = seed_stub_catalog(store)?;
    // stub provider always; http/hf can extend later
    if cfg.datasets_hub_provider == "http" && !cfg.datasets_hub_url.is_empty() {
        // Best-effort remote; fall back to stub on failure
        if let Ok(remote) = try_http_search(cfg, query, limit) {
            if !remote.is_empty() {
                return Ok(remote);
            }
        }
    }
    store.search_catalog(query, modality, license, limit)
}

fn try_http_search(cfg: &BrainConfig, query: &str, limit: usize) -> AtlasResult<Vec<Value>> {
    let url = format!(
        "{}{}q={}&limit={}",
        cfg.datasets_hub_url,
        if cfg.datasets_hub_url.contains('?') {
            "&"
        } else {
            "?"
        },
        urlencoding_lite(query),
        limit
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| AtlasError::Upstream(e.to_string()))?;
    let resp = client
        .get(&url)
        .send()
        .map_err(|e| AtlasError::Upstream(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(AtlasError::Upstream(format!(
            "hub search HTTP {}",
            resp.status()
        )));
    }
    let v: Value = resp
        .json()
        .map_err(|e| AtlasError::Upstream(e.to_string()))?;
    Ok(v.as_array().cloned().unwrap_or_default())
}

fn urlencoding_lite(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".into(),
            _ => format!("%{:02X}", c as u8),
        })
        .collect()
}

pub fn create_pinned_dataset(
    store: &Store,
    cfg: &BrainConfig,
    source: &str,
    hub_id: &str,
    revision: Option<&str>,
    project_id: Option<String>,
) -> AtlasResult<DatasetRecord> {
    let _ = seed_stub_catalog(store)?;
    let card = store
        .get_catalog_card(source, hub_id)?
        .ok_or_else(|| AtlasError::NotFound(format!("Catalog card {source}:{hub_id} niet gevonden")))?;

    let rev = revision
        .map(|s| s.to_string())
        .or_else(|| {
            card.get("revision")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown".into());

    if let Some(existing) = store.get_dataset_by_ref(source, hub_id, &rev)? {
        return Ok(existing);
    }

    let now = Utc::now().to_rfc3339();
    let project = project_id
        .filter(|s| !s.is_empty())
        .or_else(|| {
            if cfg.default_project_id.is_empty() {
                None
            } else {
                Some(cfg.default_project_id.clone())
            }
        });

    let ds = DatasetRecord {
        id: Uuid::new_v4().to_string(),
        source: source.to_string(),
        hub_id: hub_id.to_string(),
        revision: rev,
        title: card
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or(hub_id)
            .to_string(),
        description: card
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        license: card
            .get("license")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        modality: card
            .get("modality")
            .and_then(|v| v.as_str())
            .unwrap_or("text")
            .to_string(),
        gated: card
            .get("gated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        root_hash: String::new(),
        status: "pinned".into(),
        project_id: project,
        meta: json!({ "card": card }),
        created_at: now.clone(),
        updated_at: now,
    };
    store.upsert_dataset(&ds)?;
    let _ = store.enqueue_outbox(
        "dataset.pinned",
        &json!({ "dataset_id": ds.id, "hub_id": hub_id, "revision": ds.revision }),
    );
    Ok(ds)
}

/// Acquire shards for a pinned dataset. Stub provider materializes JSONL fixtures.
pub fn acquire_dataset(
    store: &Store,
    cfg: &BrainConfig,
    dataset_id: &str,
    secrets_key: &str,
) -> AtlasResult<DatasetRecord> {
    let mut ds = store
        .get_dataset(dataset_id)?
        .ok_or_else(|| AtlasError::NotFound(format!("Dataset {dataset_id} niet gevonden")))?;

    if ds.gated {
        // Require hf_token secret (never logged)
        let token = store.get_secret("hf_token", secrets_key).ok().flatten();
        if token.as_ref().map(|t| t.trim().is_empty()).unwrap_or(true) {
            return Err(AtlasError::BadRequest(
                "Gated dataset vereist secret `hf_token` (PUT /v1/atlas/secrets).".into(),
            ));
        }
    }

    let existing = store.list_shards(dataset_id)?;
    if !existing.is_empty()
        && existing.iter().all(|s| s.download_state == "done")
        && !ds.root_hash.is_empty()
    {
        // Idempotent: already acquired
        return Ok(ds);
    }

    let fixture = stub_fixture_rows(&ds.hub_id);
    let base = store.datasets_dir().join(&ds.id);
    std::fs::create_dir_all(&base)?;

    let mut total_bytes: u64 = 0;
    let budget = cfg.datasets_disk_budget_bytes;
    let mut shard_manifest = Vec::new();

    for (split, rows) in fixture {
        let mut body = String::new();
        for row in &rows {
            body.push_str(&serde_json::to_string(row)?);
            body.push('\n');
        }
        let bytes = body.as_bytes();
        total_bytes += bytes.len() as u64;
        if total_bytes > budget {
            return Err(AtlasError::BadRequest(format!(
                "Disk budget overschreden ({total_bytes} > {budget})"
            )));
        }

        let checksum = {
            let mut h = Sha256::new();
            h.update(bytes);
            hex::encode(h.finalize())
        };
        let (object_hash, _, _) = store.put_object(&store.data_dir, bytes)?;
        let name = format!("{split}.jsonl");
        let rel = format!("{}/{name}", ds.id);
        let path = base.join(&name);
        std::fs::write(&path, bytes)?;

        let shard = DatasetShard {
            id: Uuid::new_v4().to_string(),
            dataset_id: ds.id.clone(),
            name: name.clone(),
            split: split.clone(),
            path: rel,
            size_bytes: bytes.len() as i64,
            checksum: checksum.clone(),
            object_hash: Some(object_hash),
            download_state: "done".into(),
            created_at: Utc::now().to_rfc3339(),
        };
        store.upsert_shard(&shard)?;
        shard_manifest.push(json!({
            "name": name,
            "split": split,
            "checksum": checksum,
            "size_bytes": bytes.len()
        }));
    }

    let root_raw = serde_json::to_vec(&json!({
        "dataset": format!("{}:{}@{}", ds.source, ds.hub_id, ds.revision),
        "shards": shard_manifest
    }))?;
    let root_hash = Store::content_hash(&root_raw);
    ds.root_hash = root_hash.clone();
    ds.status = "acquired".into();
    ds.updated_at = Utc::now().to_rfc3339();
    store.upsert_dataset(&ds)?;

    // Structural lineage edge
    store.upsert_dataset_edge(&crate::store::DatasetEdge {
        id: Uuid::new_v4().to_string(),
        src: format!("dataset:{}", ds.id),
        dst: format!("hub:{}:{}@{}", ds.source, ds.hub_id, ds.revision),
        rel: "derived_from".into(),
        score: 1.0,
        provenance: json!({ "step": "acquire" }),
        auditor_status: "pass".into(),
        created_at: Utc::now().to_rfc3339(),
    })?;

    let _ = store.enqueue_outbox(
        "dataset.acquired",
        &json!({ "dataset_id": ds.id, "root_hash": root_hash }),
    );
    Ok(ds)
}

fn stub_fixture_rows(hub_id: &str) -> Vec<(String, Vec<Value>)> {
    match hub_id {
        "atlas/tiny-instruct" => vec![
            (
                "train".into(),
                vec![
                    json!({"id":"t1","instruction":"Say hello","response":"Hello!","label":"greet"}),
                    json!({"id":"t2","instruction":"Add 2 and 2","response":"4","label":"math"}),
                    json!({"id":"t3","instruction":"Name a color","response":"Blue","label":"fact"}),
                    json!({"id":"t4","instruction":"Say hello","response":"Hello!","label":"greet"}), // exact dup
                    json!({"id":"t5","instruction":"What is the capital of France? Paris is the capital.","response":"Paris","label":"fact"}), // eval leak
                    json!({"id":"t6","instruction":"Write a haiku","response":"Soft rain on leaves\nQuiet streets remember spring\nWindows fog gently","label":"creative"}),
                    json!({"id":"t7","instruction":"List two fruits","response":"Apple, banana","label":"fact"}),
                    json!({"id":"t8","instruction":"Translate bonjour","response":"Hello","label":"lang"}),
                ],
            ),
            (
                "test".into(),
                vec![
                    json!({"id":"e1","instruction":"Say goodbye","response":"Goodbye!","label":"greet"}),
                    json!({"id":"e2","instruction":"What is 3+1","response":"4","label":"math"}),
                ],
            ),
        ],
        "atlas/code-snippets" => vec![(
            "train".into(),
            vec![
                json!({"id":"c1","prompt":"print hello","code":"print('hello')","language":"python"}),
                json!({"id":"c2","prompt":"add function","code":"def add(a,b): return a+b","language":"python"}),
                json!({"id":"c3","prompt":"hello world js","code":"console.log('hello')","language":"javascript"}),
                json!({"id":"c4","prompt":"print hello","code":"print('hello')","language":"python"}),
            ],
        )],
        "atlas/gated-demo" => vec![(
            "train".into(),
            vec![
                json!({"id":"g1","title":"Secret note","text":"This is gated fixture text one."}),
                json!({"id":"g2","title":"Another","text":"Gated fixture text two for enrichment."}),
            ],
        )],
        _ => vec![(
            "train".into(),
            vec![json!({"id":"x1","text":"Generic fixture row","title":"Generic"})],
        )],
    }
}
