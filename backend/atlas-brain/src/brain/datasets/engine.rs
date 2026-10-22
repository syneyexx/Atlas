use crate::error::{AtlasError, AtlasResult};
use crate::store::{
    DatasetEdge, DatasetRecipe, DatasetRecord, DatasetRecordMeta, Store,
};
use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct StepResult {
    pub metrics: Value,
    pub cache_hits: usize,
    pub rows_out: usize,
}

/// Read JSONL shard rows from disk for a dataset.
pub fn read_all_rows(store: &Store, ds: &DatasetRecord) -> AtlasResult<Vec<(String, Value)>> {
    let shards = store.list_shards(&ds.id)?;
    let mut out = Vec::new();
    for shard in shards {
        let path = store.datasets_dir().join(&ds.id).join(&shard.name);
        if !path.exists() {
            // try relative path field
            let alt = store.data_dir.join("datasets").join(&shard.path);
            if !alt.exists() {
                continue;
            }
            let text = std::fs::read_to_string(&alt)?;
            for (i, line) in text.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                let v: Value = serde_json::from_str(line)?;
                out.push((format!("{}:{}", shard.split, i), v));
            }
            continue;
        }
        let text = std::fs::read_to_string(&path)?;
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let v: Value = serde_json::from_str(line)?;
            out.push((format!("{}:{}", shard.split, i), v));
        }
    }
    Ok(out)
}

pub fn text_from_row(row: &Value, recipe: &DatasetRecipe) -> String {
    let cols = recipe
        .body
        .pointer("/schema/columns")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut parts = Vec::new();
    for col in cols {
        let role = col.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let name = col.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if matches!(role, "text" | "title") && !name.is_empty() {
            if let Some(s) = row.get(name).and_then(|v| v.as_str()) {
                parts.push(s.to_string());
            }
        }
    }
    if parts.is_empty() {
        for key in ["text", "instruction", "prompt", "response", "code", "title"] {
            if let Some(s) = row.get(key).and_then(|v| v.as_str()) {
                parts.push(s.to_string());
            }
        }
    }
    parts.join("\n")
}

pub fn payload_hash(row: &Value) -> String {
    let raw = serde_json::to_vec(row).unwrap_or_default();
    let mut h = Sha256::new();
    h.update(&raw);
    hex::encode(h.finalize())
}

pub fn cache_key(
    recipe_version: &str,
    step_id: &str,
    step_version: &str,
    input_hash: &str,
    params_hash: &str,
    model_ids: &str,
) -> String {
    let raw = format!(
        "{recipe_version}|{step_id}|{step_version}|{input_hash}|{params_hash}|{model_ids}"
    );
    Store::content_hash(raw.as_bytes())
}

fn detect_lang(text: &str) -> String {
    let lower = text.to_lowercase();
    let dutch_hits = [" de ", " het ", " een ", " van ", " niet ", " voor "]
        .iter()
        .filter(|w| lower.contains(*w))
        .count();
    let code_hits = ["def ", "fn ", "console.", "=>", "import ", "{"]
        .iter()
        .filter(|w| text.contains(*w))
        .count();
    if code_hits >= 2 {
        return "code".into();
    }
    if dutch_hits >= 2 {
        return "nl".into();
    }
    "en".into()
}

fn token_count(text: &str) -> i64 {
    // Deterministic approx: whitespace tokens (tokenizer registry later)
    text.split_whitespace().count().max(1) as i64
}

fn quality_score(text: &str) -> f64 {
    let len = text.chars().count();
    if len < 8 {
        return 0.2;
    }
    if len > 4000 {
        return 0.45;
    }
    let mut score: f64 = 0.55;
    if text.contains('?') || text.contains('.') {
        score += 0.1;
    }
    if len > 40 && len < 800 {
        score += 0.15;
    }
    score.min(0.98)
}

fn minhash_sig(text: &str, bands: usize) -> Vec<u64> {
    // Lightweight char-ngram MinHash (not cryptographic; for near-dup clustering)
    let norm: String = text
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect();
    let bytes = norm.as_bytes();
    let mut sig = Vec::with_capacity(bands);
    for i in 0..bands {
        let mut best = u64::MAX;
        if bytes.len() < 3 {
            sig.push((i as u64).wrapping_mul(2654435761));
            continue;
        }
        for w in bytes.windows(3) {
            let mut h = Sha256::new();
            h.update([i as u8]);
            h.update(w);
            let dig = h.finalize();
            let mut arr = [0u8; 8];
            arr.copy_from_slice(&dig[..8]);
            let v = u64::from_le_bytes(arr);
            if v < best {
                best = v;
            }
        }
        sig.push(best);
    }
    sig
}

fn jaccard_sig(a: &[u64], b: &[u64]) -> f64 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let eq = a.iter().zip(b.iter()).filter(|(x, y)| x == y).count();
    eq as f64 / a.len() as f64
}

/// Propose recipe from features + stratified sample (heuristics-first).
pub fn propose_recipe(
    store: &Store,
    ds: &DatasetRecord,
) -> AtlasResult<(DatasetRecipe, Value)> {
    let rows = read_all_rows(store, ds)?;
    if rows.is_empty() {
        return Err(AtlasError::BadRequest(
            "Geen shards/rijen — acquire eerst.".into(),
        ));
    }

    let features = ds
        .meta
        .pointer("/card/features")
        .cloned()
        .unwrap_or_else(|| infer_features(&rows));

    let columns = infer_columns(&features, &rows);
    let sample_n = 64usize.min(rows.len());
    let sample: Vec<Value> = stratified_sample(&rows, sample_n);

    let mut null_rates = serde_json::Map::new();
    for col in &columns {
        let name = col.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let missing = sample
            .iter()
            .filter(|r| r.get(name).map(|v| v.is_null()).unwrap_or(true))
            .count();
        null_rates.insert(name.into(), json!(missing as f64 / sample.len().max(1) as f64));
    }

    let labels: HashMap<String, usize> = {
        let mut m = HashMap::new();
        for row in &sample {
            if let Some(l) = row.get("label").and_then(|v| v.as_str()) {
                *m.entry(l.to_string()).or_default() += 1;
            }
        }
        m
    };

    let report = json!({
        "n_rows_total": rows.len(),
        "n_sample": sample.len(),
        "null_rates": null_rates,
        "label_counts": labels,
        "preview": sample.iter().take(5).cloned().collect::<Vec<_>>(),
        "leakage_hints": sample.iter().filter_map(|r| {
            let t = text_blob(r);
            if t.contains("capital of France") { Some("possible_eval_leak") } else { None }
        }).collect::<Vec<_>>(),
    });

    let body = json!({
        "recipe_version": "1.0.0",
        "dataset": {
            "source": ds.source,
            "id": ds.hub_id,
            "revision": ds.revision
        },
        "schema": {
            "columns": columns,
            "sample": { "strategy": "stratified", "n": sample_n }
        },
        "steps": [
            {"id": "normalize", "version": 1},
            {"id": "langdetect", "version": 1},
            {"id": "dedupe_exact", "version": 1},
            {"id": "dedupe_minhash", "version": 1, "params": {"threshold": 0.87}},
            {"id": "enrich_title", "version": 1, "params": {"ladder": ["field","small","large","verify"]}},
            {"id": "quality_classify", "version": 1},
            {"id": "decontaminate", "version": 1, "params": {"eval_guards": ["mmlu","hellaswag","gsm8k"]}}
        ],
        "human_gate": "after_sample",
        "budgets": {
            "max_disk_bytes": 50_000_000_000u64,
            "max_cost_usd": 25.0,
            "max_tokens": 5_000_000
        }
    });

    let now = Utc::now().to_rfc3339();
    let recipe = DatasetRecipe {
        id: Uuid::new_v4().to_string(),
        dataset_id: ds.id.clone(),
        version: "1.0.0".into(),
        status: "draft".into(),
        body,
        sample_report: report.clone(),
        verified: false,
        created_at: now.clone(),
        updated_at: now,
    };
    store.upsert_recipe(&recipe)?;
    Ok((recipe, report))
}

fn infer_features(rows: &[(String, Value)]) -> Value {
    let mut keys = HashSet::new();
    for (_, r) in rows.iter().take(20) {
        if let Some(obj) = r.as_object() {
            for k in obj.keys() {
                keys.insert(k.clone());
            }
        }
    }
    let mut feat = serde_json::Map::new();
    for k in keys {
        feat.insert(k, json!({"dtype": "string"}));
    }
    Value::Object(feat)
}

fn infer_columns(features: &Value, rows: &[(String, Value)]) -> Vec<Value> {
    let mut cols = Vec::new();
    if let Some(obj) = features.as_object() {
        for (name, _) in obj {
            let role = guess_role(name);
            let modality = guess_modality(name, rows);
            cols.push(json!({
                "name": name,
                "role": role,
                "modality": modality
            }));
        }
    }
    if cols.is_empty() {
        cols.push(json!({"name":"text","role":"text","modality":"text"}));
    }
    cols
}

fn guess_role(name: &str) -> &'static str {
    let n = name.to_lowercase();
    if n.contains("title") {
        "title"
    } else if n.contains("label") || n == "category" {
        "label"
    } else if n == "id" || n.ends_with("_id") {
        "id"
    } else if n.contains("time") || n.contains("date") {
        "time"
    } else if n.contains("lang") {
        "lang"
    } else if n.contains("parent") {
        "parent"
    } else if n.contains("child") {
        "child"
    } else if matches!(
        n.as_str(),
        "text" | "instruction" | "prompt" | "response" | "content" | "code" | "input" | "output"
    ) {
        "text"
    } else {
        "meta"
    }
}

fn guess_modality(name: &str, rows: &[(String, Value)]) -> &'static str {
    let n = name.to_lowercase();
    if n.contains("code") || n == "language" {
        return "code";
    }
    if n.contains("image") {
        return "image";
    }
    if n.contains("audio") {
        return "audio";
    }
    for (_, r) in rows.iter().take(5) {
        if let Some(s) = r.get(name).and_then(|v| v.as_str()) {
            if s.contains("def ") || s.contains("fn ") || s.contains("console.") {
                return "code";
            }
        }
    }
    "text"
}

fn stratified_sample(rows: &[(String, Value)], n: usize) -> Vec<Value> {
    let mut by_label: HashMap<String, Vec<&Value>> = HashMap::new();
    for (_, r) in rows {
        let key = r
            .get("label")
            .or_else(|| r.get("language"))
            .or_else(|| r.get("split"))
            .and_then(|v| v.as_str())
            .unwrap_or("all")
            .to_string();
        by_label.entry(key).or_default().push(r);
    }
    let mut out = Vec::new();
    let groups = by_label.len().max(1);
    let per = (n / groups).max(1);
    for (_k, mut vals) in by_label {
        vals.truncate(per);
        for v in vals {
            out.push(v.clone());
            if out.len() >= n {
                return out;
            }
        }
    }
    if out.len() < n {
        for (_, r) in rows {
            if out.len() >= n {
                break;
            }
            if !out.iter().any(|x| x == r) {
                out.push(r.clone());
            }
        }
    }
    out
}

fn text_blob(row: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(obj) = row.as_object() {
        for (k, v) in obj {
            if let Some(s) = v.as_str() {
                parts.push(format!("{k}:{s}"));
            }
        }
    }
    parts.join(" ")
}

pub fn verify_recipe(recipe: &DatasetRecipe) -> AtlasResult<Value> {
    let mut errors = Vec::new();
    let body = &recipe.body;
    if body.get("recipe_version").and_then(|v| v.as_str()).is_none() {
        errors.push("missing recipe_version");
    }
    if body.pointer("/schema/columns").and_then(|v| v.as_array()).is_none() {
        errors.push("missing schema.columns");
    }
    let steps = body
        .get("steps")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if steps.is_empty() {
        errors.push("steps empty");
    }
    let has_decontam = steps.iter().any(|s| {
        s.get("id").and_then(|v| v.as_str()) == Some("decontaminate")
    });
    if !has_decontam {
        errors.push("decontaminate step required before export-capable recipes");
    }
    if body.get("budgets").is_none() {
        errors.push("missing budgets");
    }
    if body.get("human_gate").and_then(|v| v.as_str()) != Some("after_sample") {
        errors.push("human_gate must be after_sample");
    }
    let ok = errors.is_empty();
    Ok(json!({ "ok": ok, "errors": errors }))
}

/// Run one processing unit for a full/sample job. Mutates store records.
pub fn run_pipeline_pass(
    store: &Store,
    ds: &DatasetRecord,
    recipe: &DatasetRecipe,
    sample_only: bool,
) -> AtlasResult<StepResult> {
    let mut rows = read_all_rows(store, ds)?;
    if sample_only {
        let n = recipe
            .body
            .pointer("/schema/sample/n")
            .and_then(|v| v.as_u64())
            .unwrap_or(64) as usize;
        rows = stratified_sample_pairs(&rows, n);
    }

    let mut cache_hits = 0usize;
    let recipe_ver = recipe.version.as_str();
    let guard_hashes = store.eval_guard_hashes()?;

    // normalize + exact dedupe
    let mut seen_exact = HashSet::new();
    let mut working: Vec<(String, Value, String)> = Vec::new(); // split_row, row, hash
    for (key, row) in rows {
        let split = key.split(':').next().unwrap_or("train").to_string();
        let row_uid = row
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| key.clone());
        let h = payload_hash(&row);
        if !seen_exact.insert(h.clone()) {
            // exact dup dropped
            store.upsert_dataset_edge(&DatasetEdge {
                id: Uuid::new_v4().to_string(),
                src: format!("record:{}:{}", ds.id, row_uid),
                dst: format!("dataset:{}", ds.id),
                rel: "near_duplicate".into(),
                score: 1.0,
                provenance: json!({"step":"dedupe_exact","hash": h}),
                auditor_status: "unreviewed".into(),
                created_at: Utc::now().to_rfc3339(),
            })?;
            continue;
        }
        working.push((format!("{split}|{row_uid}"), row, h));
    }

    // MinHash near-dup
    let threshold = recipe
        .body
        .pointer("/steps")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .find(|s| s.get("id").and_then(|x| x.as_str()) == Some("dedupe_minhash"))
        .and_then(|s| s.pointer("/params/threshold"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.87);

    let sigs: Vec<(usize, Vec<u64>)> = working
        .iter()
        .map(|(_, row, _)| minhash_sig(&text_from_row(row, recipe), 32))
        .enumerate()
        .map(|(i, s)| (i, s))
        .collect();

    let mut drop_idx = HashSet::new();
    for i in 0..sigs.len() {
        if drop_idx.contains(&i) {
            continue;
        }
        for j in (i + 1)..sigs.len() {
            if drop_idx.contains(&j) {
                continue;
            }
            let sim = jaccard_sig(&sigs[i].1, &sigs[j].1);
            if sim >= threshold {
                // keep higher quality
                let ti = text_from_row(&working[i].1, recipe);
                let tj = text_from_row(&working[j].1, recipe);
                let drop = if quality_score(&ti) >= quality_score(&tj) {
                    j
                } else {
                    i
                };
                drop_idx.insert(drop);
                let (keep_uid, _, _) = &working[if drop == i { j } else { i }];
                let (drop_uid, _, _) = &working[drop];
                store.upsert_dataset_edge(&DatasetEdge {
                    id: Uuid::new_v4().to_string(),
                    src: format!("record:{}:{}", ds.id, drop_uid),
                    dst: format!("record:{}:{}", ds.id, keep_uid),
                    rel: "near_duplicate".into(),
                    score: sim,
                    provenance: json!({"step":"dedupe_minhash"}),
                    auditor_status: "unreviewed".into(),
                    created_at: Utc::now().to_rfc3339(),
                })?;
            }
        }
    }

    let mut kept = Vec::new();
    for (i, item) in working.into_iter().enumerate() {
        if !drop_idx.contains(&i) {
            kept.push(item);
        }
    }

    // enrich + quality + decontaminate + persist
    for (key, row, hash) in &kept {
        let (split, row_uid) = key.split_once('|').unwrap_or(("train", key.as_str()));
        let text = text_from_row(row, recipe);
        let lang = detect_lang(&text);
        let tokens = token_count(&text);
        let quality = quality_score(&text);

        // enrichment ladder for title
        let (title, hit) = enrich_title(store, recipe_ver, hash, row, &text)?;
        if hit {
            cache_hits += 1;
        }

        let tags = infer_tags(row, &text, &lang);
        let text_hash = Store::content_hash(text.as_bytes());
        let contaminated = guard_hashes.contains(&text_hash)
            || text.contains("capital of France")
            || text.contains("ducks lay 16 eggs")
            || text.contains("opens the fridge to get milk");

        if contaminated {
            store.upsert_dataset_edge(&DatasetEdge {
                id: Uuid::new_v4().to_string(),
                src: format!("record:{}:{}", ds.id, row_uid),
                dst: "evalguard:mmlu".into(),
                rel: "contaminates".into(),
                score: 1.0,
                provenance: json!({"step":"decontaminate"}),
                auditor_status: "pass".into(),
                created_at: Utc::now().to_rfc3339(),
            })?;
        }

        let mut meta = row.clone();
        if let Some(obj) = meta.as_object_mut() {
            obj.insert("text".into(), json!(text));
        }

        let rec = DatasetRecordMeta {
            id: Uuid::new_v4().to_string(),
            dataset_id: ds.id.clone(),
            split: split.to_string(),
            row_uid: row_uid.to_string(),
            payload_hash: hash.clone(),
            language: lang,
            modality: ds.modality.clone(),
            token_count: tokens,
            quality,
            title,
            tags_json: json!(tags),
            contaminated,
            meta,
            created_at: Utc::now().to_rfc3339(),
        };
        store.upsert_dataset_record(&rec)?;
    }

    // dataset-level same_source edge to self-revision (lineage)
    store.upsert_dataset_edge(&DatasetEdge {
        id: Uuid::new_v4().to_string(),
        src: format!("dataset:{}", ds.id),
        dst: format!("hub:{}:{}@{}", ds.source, ds.hub_id, ds.revision),
        rel: "same_source".into(),
        score: 1.0,
        provenance: json!({"step":"pipeline"}),
        auditor_status: "pass".into(),
        created_at: Utc::now().to_rfc3339(),
    })?;

    Ok(StepResult {
        metrics: json!({
            "rows_in": kept.len() + drop_idx.len() + /* approx */ 0,
            "rows_out": kept.len(),
            "near_dup_dropped": drop_idx.len(),
            "cache_hits": cache_hits,
            "contaminated": kept.iter().filter(|(_,r,_)| {
                let t = text_from_row(r, recipe);
                t.contains("capital of France")
            }).count()
        }),
        cache_hits,
        rows_out: kept.len(),
    })
}

fn stratified_sample_pairs(
    rows: &[(String, Value)],
    n: usize,
) -> Vec<(String, Value)> {
    let sample = stratified_sample(rows, n);
    let mut out = Vec::new();
    for s in sample {
        if let Some((k, _)) = rows.iter().find(|(_, r)| r == &s) {
            out.push((k.clone(), s));
        } else {
            out.push((format!("sample:{}", out.len()), s));
        }
    }
    out
}

fn enrich_title(
    store: &Store,
    recipe_ver: &str,
    input_hash: &str,
    row: &Value,
    text: &str,
) -> AtlasResult<(String, bool)> {
    // 1. existing field
    if let Some(t) = row
        .get("title")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
    {
        return Ok((t.to_string(), true));
    }

    let key = cache_key(recipe_ver, "enrich_title", "1", input_hash, "ladder", "stub");
    if let Some((val, _, _)) = store.get_enrichment_cache(&key)? {
        if let Some(s) = val.as_str() {
            return Ok((s.to_string(), true));
        }
    }

    // 2. small heuristic / 3. "large" only for hard cases
    let title = if text.chars().count() < 12 {
        // "large" path — still deterministic stub
        let t = format!("Item · {}", text.chars().take(24).collect::<String>());
        store.put_enrichment_cache(&key, "title", &json!(t), "stub-large", 0.7)?;
        t
    } else {
        // small model distill stub
        let first = text.lines().next().unwrap_or(text);
        let t: String = first.chars().take(72).collect();
        store.put_enrichment_cache(&key, "title", &json!(t), "stub-small", 0.85)?;
        t
    };
    Ok((title, false))
}

fn infer_tags(row: &Value, text: &str, lang: &str) -> Vec<String> {
    let mut tags = vec![lang.to_string()];
    if let Some(l) = row.get("label").and_then(|v| v.as_str()) {
        tags.push(l.to_string());
    }
    if text.contains('?') {
        tags.push("question".into());
    }
    if lang == "code" {
        tags.push("code".into());
    }
    tags
}

pub fn build_datamap(store: &Store, dataset_id: &str) -> AtlasResult<Value> {
    let ds = store
        .get_dataset(dataset_id)?
        .ok_or_else(|| AtlasError::NotFound(format!("dataset {dataset_id}")))?;
    let facets = store.record_facets(dataset_id)?;
    let shards = store.list_shards(dataset_id)?;
    let recipes = store.list_recipes(dataset_id)?;
    let edges = store.list_dataset_edges(Some(&format!("dataset:{dataset_id}")), None, 50)?;
    Ok(json!({
        "dataset": ds,
        "facets": facets,
        "shards": shards.len(),
        "recipes": recipes.iter().map(|r| json!({
            "id": r.id, "version": r.version, "status": r.status, "verified": r.verified
        })).collect::<Vec<_>>(),
        "edge_sample": edges,
        "generated_at": Utc::now().to_rfc3339()
    }))
}

pub fn compile_mix(store: &Store, mix_id: &str) -> AtlasResult<Value> {
    let mut mix = store
        .get_mix(mix_id)?
        .ok_or_else(|| AtlasError::NotFound(format!("mix {mix_id}")))?;

    let datasets = mix
        .definition
        .get("datasets")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let mut export_rows = Vec::new();
    let mut blocked = Vec::new();
    let allow_contaminated = mix
        .definition
        .get("allow_contaminated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    for entry in datasets {
        let ds_id = entry.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let weight = entry.get("weight").and_then(|v| v.as_f64()).unwrap_or(1.0);
        let limit = entry
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(1000) as usize;
        let records = store.list_dataset_records(ds_id, limit, 0)?;
        for r in records {
            if r.contaminated && !allow_contaminated {
                blocked.push(json!({"dataset_id": ds_id, "row_uid": r.row_uid}));
                continue;
            }
            // curriculum: prefer higher difficulty proxy = lower quality? use token_count bands
            export_rows.push(json!({
                "dataset_id": ds_id,
                "row_uid": r.row_uid,
                "title": r.title,
                "text": r.meta.get("text").cloned().unwrap_or(json!("")),
                "weight": weight,
                "token_count": r.token_count,
                "quality": r.quality,
                "tags": r.tags_json,
                "payload_hash": r.payload_hash
            }));
        }
    }

    // curriculum sort: easier (shorter) first
    export_rows.sort_by(|a, b| {
        let ta = a.get("token_count").and_then(|v| v.as_i64()).unwrap_or(0);
        let tb = b.get("token_count").and_then(|v| v.as_i64()).unwrap_or(0);
        ta.cmp(&tb)
    });

    let body = export_rows
        .iter()
        .map(|r| serde_json::to_string(r).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    let bytes = format!("{body}\n");
    let (object_hash, _, _) = store.put_object(&store.data_dir, bytes.as_bytes())?;

    let manifest = json!({
        "format": "jsonl",
        "object_hash": object_hash,
        "n_rows": export_rows.len(),
        "blocked_contaminated": blocked.len(),
        "blocked": blocked,
        "curriculum": "token_count_asc",
        "compiled_at": Utc::now().to_rfc3339()
    });

    if !blocked.is_empty() && !allow_contaminated {
        // hard fail export if any contamination without override — still save manifest
        mix.status = "blocked".into();
    } else {
        mix.status = "compiled".into();
    }
    mix.export_manifest = manifest.clone();
    mix.updated_at = Utc::now().to_rfc3339();
    store.upsert_mix(&mix)?;

    if mix.status == "blocked" {
        return Err(AtlasError::BadRequest(format!(
            "Export geblokkeerd: {} contaminated rows. Zet allow_contaminated=true + audit override.",
            blocked.len()
        )));
    }

    let _ = store.enqueue_outbox(
        "dataset.mix_compiled",
        &json!({ "mix_id": mix.id, "object_hash": object_hash }),
    );
    Ok(manifest)
}
