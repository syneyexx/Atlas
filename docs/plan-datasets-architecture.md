# Atlas Brain — Datasets Plane Plan (World-Class)

> Planmodus deliverable. Doel: de **Datasets**-pagina onder Data als Cursor-klasse data-OS — acquisitie → recept → verwerking → verrijking → relatiegraaf → mix-export — op **één brain**, request-driven, zonder parallelle ETL-stack.
>
> **UI-graphics:** bewust out-of-scope tot de frontend-upload. Dit plan specificeert contracts, stores, engines, agentrollen en bouwvolgorde. De pagina is een view op die contracts.

---

## 0. Context (source is leidend)

| Kapitaal vandaag | Hergebruik voor Datasets |
|------------------|--------------------------|
| Document ingest + content-addressed objects | Shard/record blobs, hash-idempotentie |
| Memory Cortex + Write Governor | Semantic index, provenance, conflict/truth |
| `memory_edges` (+ hop-2) | Dataset-/record-relaties (licht starten) |
| Agent-OS: tools · tasks · checkpoints | Schema-agent, pipeline-ticks, human gate |
| Research plane (tick/budget orchestrator) | **Patroon** voor lange jobs zonder daemon |
| Evals · feedback · `training_examples` · cost ledger | Kwaliteitslus + export-sink |
| Secrets · projects · artifacts · engines | HF-tokens, project-scope, recipe artifacts |

**Gap:** geen Hub-catalogus, geen Parquet/Arrow-pipeline, geen recepten, geen dedupe/decontaminatie, geen mixbouwer, geen dataset-MCP. `training_examples` is alleen feedback-correcties — geen trainingsmix.

**Productpositie:** onder Data naast Research. Research = web→rapport. Datasets = Hub/files→curated trainingsmateriaal + graph. Zelfde brain, andere plane.

---

## 1. Verdict op Claudes 10-puntenvoorstel

**Sterk als productvisie.** Acquisitie met commit-pin, schema-agent + versioned recipe, kostenladder, decontaminatie, hybrid serving + MCP, en **één human checkpoint na steekproef** zijn precies Cursor-niveau.

**Niet wereldklasse genoeg als je het 1:1 bouwt.** Zes fouten zouden Atlas kapot maken:

| # | Claude-default | Waarom fout voor Atlas | Expert-correctie |
|---|----------------|------------------------|------------------|
| 1 | Ray **of** Polars + **Dagster/Prefect** | Schendt one-brain / geen daemons; tweede orchestrator naast Agent-OS | **Polars/DuckDB/Arrow in-process**; DAG = Atlas tasks + outbox + checkpoints; Ray alleen meetbare cloud-adapter |
| 2 | Neo4j/Memgraph + Qdrant + lakeFS/DVC + Postgres **nu** | Premature multi-engine; zelfde valkuil als stores-plan | SQLite-first → Parquet op disk → pgvector dual; graph-tabel eerst; lake/Qdrant/Neo4j **pas bij meting** |
| 3 | “Verwerkingsengine” als aparte runtime | Background workers, health-pings, split brain | **Request-driven ticks** (Research-patroon): `POST …/tick` met step-budget |
| 4 | Schema-agent = “Claude leest alles” | Kostenexplosie + non-determinisme | Features → gestratificeerde steekproef → **heuristics-first**, LM alleen voor rollen/ambiguïteit; recipe JSON Schema + verifier |
| 5 | Relatiegraaf met NER+Wikidata+triples overal | Te vroeg zwaar; precisie onbekend | Structureel + overlap eerst; semantic ANN op embeddings die we al hebben; NER/EL als **opt-in recipe steps** + auditor |
| 6 | Pagina als gelijke pilaar in bouwvolgorde | UI zonder contracts = theater | Contracts + sample-run **vóór** UI; UI consumeert alleen stabiele API |

**Richting:** Claudes taxonomy behouden, **Atlas invariants hard afdwingen**, 7 ontbrekende stukken toevoegen (§3), engines gefaseerd, niets weglaten — wel **gefaseerd materialiseren**.

---

## 2. Wat er al goed is (behouden uit Claude)

1. **Semantisch Hub-zoeken + filters** — cataloguskaart ≠ gedownloade data.
2. **Parallel shard-download** met checksums, resume, disk-budget, gated token.
3. **Commit-SHA pin** per dataset — geen stille upstream drift.
4. **Schema-agent:** features → stratified sample → rollen + modaliteit → versioned recipe (YAML/JSON).
5. **Idempotente, content-addressed stappen** + checkpoints.
6. **Kostenladder** voor titles/samenvattingen/tags/domein/moeilijkheid/tokens.
7. **Relatietypen** structureel / semantisch / feiten / dataset-niveau; edges met type·score·bron·tijd.
8. **Decontaminatie tegen evalsets** — non-negotiable voor trainingsmix.
9. **Hybrid search + GraphRAG + MCP tools.**
10. **Agentrollen + één human gate** na steekproef, vóór full run.
11. **Afhankelijkheidsvolgorde:** schema → engine → verrijking → graph → pagina.

---

## 3. Wat mist (toevoegen — Cursor-moat)

Zonder dit bouw je een Hub-downloader met LLM-labels, geen data-OS.

| # | Toevoeging | Waarom |
|---|------------|--------|
| A | **Dataset identity plane** | `dataset_id` = `source:org/name@revision` + content root-hash. Elke record/edge/job refereert dit. Zonder pin = giftige lineage. |
| B | **Recipe as code + contract** | JSON Schema + semver + `steps[]` met `input_hash → output_hash`. UI mag editen; runtime mag alleen gevalideerde recipes draaien. |
| C | **Job state machine (Research-patroon)** | `draft → sampling → await_approval → running → paused → completed\|failed`. Ticks, geen worker-daemon. |
| D | **Cache key universeel** | `hash(recipe_semver + step_id + step_version + input_content_hash + model_ids)`. Verrijking/dedupe nooit dubbel betaald. |
| E | **License · PII · safety gates** | Vóór export: license allowlist, PII classifier opt-in, toxicity/NSFW flags. Mixbouwer weigert gated exports zonder expliciete override + audit trace. |
| F | **Eval-set registry + decontaminatie-index** | Eerste-klas store van “verboden” eval-teksten (hash + MinHash + embedding). Training export faalt hard bij hit boven drempel. |
| G | **Mix / curriculum compiler** | Gewogen sampling, difficulty curriculum, chat-template, tokenized shards — product-einde, niet bijzaak. |
| H | **Offline/stub Hub** | CI en local dignity: stub catalogus + tiny Parquet fixtures; geen netwerk vereist. |
| I | **Cost & disk governors** | Hard caps per job (tokens, $, bytes). Cost ledger al aanwezig — datasets schrijft erin. |
| J | **Auditor + golden sets** | Niet alleen “LLM-judge”: vaste gouden titels/relaties + menselijke steekproef API; regressie per recipe-bump. |

---

## 4. Atlas invariants (hard — niet onderhandelbaar)

1. **One brain** — Datasets is een plane in `atlas-brain`, geen aparte service.
2. **Request-driven** — geen background ETL/embed/dedupe sweepers; wel `tick`, outbox drain on request, expliciete consolidate/export API’s.
3. **Één schrijfdeur per domein** — records via dataset ingest; memory via Write Governor; costs/secrets via bestaande API’s.
4. **Trace spine** — elke sample/full run, enrichment call, auditor-pass → `trace_id`.
5. **Scope** — datasets default **project-scoped**; catalog cards mogen global-read.
6. **Local dignity** — SQLite + `data/datasets/` + stub Hub draait full path offline.
7. **Engines follow scale** — taxonomy ≠ physical systems.
8. **Secrets** — HF tokens in secrets store; nooit in traces/recipes/UI logs.
9. **UI honesty** — frontend verzint geen pipeline-events; alleen Brain events streamen.
10. **Source leidend** — docs volgen code zodra D0+ landt; tot die tijd is dit plan de source of intent.

---

## 5. Logische stores (uitbreiding op de 28)

Nieuwe logische stores — **nog steeds dezelfde 1–4 engines**.

| # | Store | Inhoud |
|---|--------|--------|
| 29 | **Dataset catalog** | Hub/local cards: id, title, description, tags, modality, license, downloads, embedding van card-tekst, filters |
| 30 | **Dataset revisions** | Pin: `revision` (commit SHA / local mtime fingerprint), root manifest, shard list + checksums |
| 31 | **Dataset files / shards** | Paths, sizes, checksum, download state, object_hash koppeling |
| 32 | **Records (logical)** | Content-addressed record metadata (split, row_uid, modality, language, token_count, quality); bulk in Parquet |
| 33 | **Recipes** | Versioned YAML/JSON + schema_version + status (draft/approved/deprecated) |
| 34 | **Dataset jobs** | State machine, budgets, ETA, costs, checkpoint pointer |
| 35 | **Recipe run artifacts** | Per-step outputs, metrics, sample reports, datakaart |
| 36 | **Enrichment cache** | Key → value (title, summary, tags, …) + model_id + confidence |
| 37 | **Dataset edges** | typed edges (record↔record, dataset↔dataset, record↔entity) |
| 38 | **Eval-guard index** | Decontaminatie corpora + hashes/sketches |
| 39 | **Mix definitions** | Weights, filters, curriculum, export format, output manifest |

**Mapping op bestaande stores:** objects (blobs), artifacts (recipe snapshots, datakaarten, export manifests), tasks/checkpoints (job ticks), traces/costs, secrets (tokens), projects (scope), training_examples (menselijke correcties → enrichment labels), evals (golden + regressie).

---

## 6. Engines — gefaseerd (Datasets-specifiek)

| Fase | Engines | Wanneer |
|------|---------|---------|
| **D-A Local** | SQLite metadata + **Parquet/Arrow op filesystem** (`data/datasets/{dataset_id}/…`) + bestaande objects | Default. Polars of DuckDB in-process voor transforms |
| **D-B Local+** | + sqlite-vec / bestaande embeddings voor card + record ANN | Semantisch zoeken zonder Qdrant |
| **D-C Team** | Postgres dual-write metadata · S3 voor shards/Parquet · Redis optioneel voor job-lease cache | Multi-instance |
| **D-D Scale** | Ray worker pool **als adapter** · Qdrant/pgvector dedicated · AGE/Neo4j | Alleen met gemeten pijn (throughput, edge QPS, vector recall@scale) |
| **D-E Lake** | lakeFS **of** DVC-achtige refs boven content-addressed tree | Pas als meerdere consumers dezelfde lake delen |

**Default compute:** Polars lazy + Arrow streaming. DuckDB SQL waar aggregaties/facet-counts sneller zijn. **Geen** Dagster/Prefect — de DAG is data in `dataset_jobs` + step registry in code.

**Download:** `hf_transfer` / parallel range-GETs achter tool `datasets.acquire`; resume via shard checksum state; disk-budget check vóór elke shard.

---

## 7. Domeinmodel (contracts)

### 7.1 Identity

```
DatasetRef = {
  source: "hf" | "local" | "url",
  id: "org/name",          # of local slug
  revision: "<commit-sha>|fingerprint",
}
DatasetKey = sha256(canonical_json(DatasetRef))
RecordKey  = sha256(DatasetKey + split + stable_row_id + payload_hash)
```

### 7.2 Recipe (gevalideerd)

```yaml
recipe_version: "1.0.0"
dataset: { source: hf, id: "...", revision: "abc123" }
schema:
  columns:
    - name: text
      role: text          # title|text|label|id|time|parent|child|lang|meta|ignore
      modality: text      # text|code|image|audio|conversation|tool_use|other
  sample:
    strategy: stratified
    n: 64
    strata: [split, label]
steps:
  - id: normalize
    version: 1
  - id: langdetect
    version: 1
  - id: dedupe_exact
    version: 1
  - id: dedupe_minhash
    version: 1
    params: { threshold: 0.87 }
  - id: enrich_title
    version: 1
    params: { ladder: [field, small, large, verify] }
  - id: quality_classify
    version: 1
  - id: decontaminate
    version: 1
    params: { eval_guards: ["mmlu", "hellaswag"] }
human_gate: after_sample   # verplicht
budgets:
  max_disk_bytes: 50_000_000_000
  max_cost_usd: 25
  max_tokens: 5_000_000
```

### 7.3 Step contract (idempotent)

```
StepFn(ctx, input_ref) -> {
  output_ref,           # content-addressed
  metrics,              # rows_in/out, bytes, latency
  cache_hit: bool,
  trace_id
}
```

Hetzelfde `(step_id, step_version, input_hash, params_hash, model_ids)` → byte-identical output of hard fail.

### 7.4 Job ticks (Research-patroon)

```
POST /v1/atlas/datasets/jobs/{id}/tick  { max_steps?: 1..32 }
```

Elke tick: 1..N step-units binnen budget; schrijft events; stopt op `await_approval`, pause, budget, of completion. **Geen** background loop.

### 7.5 Edge model

```
DatasetEdge {
  src, dst,           # dataset:… | record:… | entity:…
  rel,                # derived_from | same_source | same_split | overlap |
                      # near_duplicate | same_cluster | topic_child |
                      # mentions | contaminates | …
  score,              # 0..1
  provenance,         # step_id, model_id, evidence_span?
  created_at,
  auditor_status?     # unreviewed|pass|fail
}
```

Start in SQL (`dataset_edges`). Neo4j alleen in D-D.

---

## 8. Pipeline (volledig — niets weggelaten, wel gefaseerd)

```
[Catalog search] → [Pin revision] → [Acquire shards]
        → [Schema-agent: features + sample] → [Recipe draft]
        → ❑ HUMAN GATE (sample metrics + preview rows)
        → [Full run DAG]
             normalize → langdetect → exact dedupe → MinHash-LSH
             → semantic dedupe → quality → enrich* (ladder)
             → decontaminate → entity/graph steps (opt-in)
        → [Datakaart + artifacts] → [Index for serving]
        → [Mix compile / export]
```

### 8.1 Acquisitie
- Hub card search: BM25 op metadata + vector op card-embedding; filters (license, modality, size, language, gated).
- Parallel download + checksum + resume; gated via secrets; disk governor.
- Manifest pinned; wijziging upstream zonder revision-bump = nieuwe dataset-versie, nooit silent overwrite.

### 8.2 Schema-agent
1. Lees `features` / Parquet schema (deterministisch).
2. Stratified sample (splits/labels/length buckets).
3. Role + modality inference: rules → small LM → large LM alleen bij lage confidence.
4. Emit recipe + **sample report** (null rates, length hist, label imbalance, leakage hints).
5. RecipeVerifier (aparte agentrol): schema valid, budgets aanwezig, decontaminate step verplicht vóór export-capable recipes.

### 8.3 Verwerkingsengine
- Streaming Arrow/Parquet; spill naar disk; never load full corpus in RAM by default.
- Exact dedupe: payload hash.
- MinHash-LSH: near-dup clusters; keep-policy = highest quality / canonical source.
- Semantic dedupe: embedding cosine binnen cluster-kandidaten (niet O(n²) full).
- Quality classifier: heuristics + model ladder.
- Decontaminatie: eval-guard index (exact + MinHash + optional embed).

### 8.4 Verrijking (kostenladder — overal hetzelfde patroon)
1. Bestaand veld / deterministische regel  
2. Small/distilled model  
3. Large model (moeilijke gevallen / lage confidence)  
4. Verifier + cache op content hash  

Toepassen op: title, summary, tags, domain, difficulty, token_count (token_count bij voorkeur altijd deterministisch via tokenizer registry).

### 8.5 Relatiegraaf
- **Structureel eerst:** derived_from, same_source/split, acquire lineage.
- **Dataset-overlap:** Jaccard op MinHash signatures van corpora.
- **Semantic:** dense (+ later sparse) → HNSW wanneer D-B/D-D; clusters → hierarchical topic tree als artifact.
- **Feiten (opt-in):** NER → entity link (Wikidata) → triples met evidence + confidence; cross-dataset entity resolution via canonical IDs.
- **Auditor-agent:** steekproefprecisie per edge-type; scores terug in eval store.

### 8.6 Serving
- Hybrid: BM25 (FTS) + vector + optional graph expand (cortex-achtige fuse).
- GraphRAG: subgraph retrieve → residual frames voor LM.
- MCP tools (Claude/Cursor): `search_datasets`, `get_record`, `neighbors`, `subgraph`, `get_recipe`, `tick_job`, `compile_mix`.
- Export: JSONL / Parquet / chat-templates; weighted sample; curriculum by difficulty; tokenized shards met tokenizer pin.

---

## 9. Agentrollen (mapped op Agent-OS)

| Rol | Verantwoordelijkheid | Mechanisme |
|-----|----------------------|------------|
| **Planner** | Job plan, step order, budgets | Lead task; schrijft job checkpoint |
| **Schema-agent** | Features + sample → recipe draft | Tool + LM; artifact `recipe.draft` |
| **Recipe-verifier** | Schema/budget/safety gates | Pure + LM; blokkeert gate bij fail |
| **Verrijker** | Ladder steps | Cache-aware; cost ledger |
| **Relatie-extractor** | Edges + optional NER/EL | Opt-in steps |
| **Auditor** | Precision samples, golden diffs | Eval store |
| **Kostenbewaker** | Hard stop op budget | Elke tick; pause + event |
| **Human** | Enige verplichte checkpoint | `await_approval` na sample |

Subagents = child tasks (`parent_id`), zelfde patroon als Research.

---

## 10. API-oppervlak (contract-first; UI later)

```
# Catalog
GET  /v1/atlas/datasets/catalog/search?q=&filters…
GET  /v1/atlas/datasets/catalog/{source}/{id}

# Pins & acquire
POST /v1/atlas/datasets                    # create from catalog/local
POST /v1/atlas/datasets/{id}/acquire
GET  /v1/atlas/datasets/{id}
GET  /v1/atlas/datasets/{id}/shards

# Schema & recipes
POST /v1/atlas/datasets/{id}/sample
POST /v1/atlas/datasets/{id}/recipes/propose
GET/PUT /v1/atlas/datasets/{id}/recipes/{recipe_id}
POST /v1/atlas/datasets/{id}/recipes/{recipe_id}/verify

# Jobs
POST /v1/atlas/datasets/jobs               # sample|full
GET  /v1/atlas/datasets/jobs/{job_id}
POST /v1/atlas/datasets/jobs/{job_id}/tick
POST /v1/atlas/datasets/jobs/{job_id}/pause
POST /v1/atlas/datasets/jobs/{job_id}/approve   # human gate
POST /v1/atlas/datasets/jobs/{job_id}/reject

# Explore & graph
GET  /v1/atlas/datasets/{id}/records…
GET  /v1/atlas/datasets/{id}/facets
GET  /v1/atlas/datasets/{id}/edges…
GET  /v1/atlas/datasets/{id}/datamap

# Mix & export
POST /v1/atlas/datasets/mixes
POST /v1/atlas/datasets/mixes/{id}/compile
GET  /v1/atlas/datasets/exports/{id}

# MCP mirrors above tools
```

Alle mutaties: traces + outbox events (`dataset.acquired`, `recipe.proposed`, `job.awaiting_approval`, `job.completed`, …).

---

## 11. Pagina (spec only — graphics later)

Wanneer UI landt, vier zones — **geen** dashboard-clutter in de eerste viewport van Hub-zoeken:

1. **Hub-zoeker** — query + filters; resultaatkaarten; voorbeeldrijen; voorgesteld recept.
2. **Job console** — live DAG uit Brain events; throughput, ETA, kosten; pauze/hervat/tick.
3. **Explorer** — facetten → records; graph: cluster → record (drill-down).
4. **Quality · lineage · versions · mixbouwer** — eigen secties, elk één job.

UI leest uitsluitend API; mag geen sample/full verwarren; mag gated tokens niet tonen.

---

## 12. Roadmap (technisch, geen kalender)

Afhankelijkheden dwingen volgorde. **Niets weggelaten** — wel pas materialiseren wanneer dependency groen is.

### D0 — Identity + schema stores
- Tabellen: catalog_cache, datasets, revisions, shards, recipes, jobs, events, dataset_edges (leeg), enrichment_cache, eval_guards, mixes
- Project scope + secrets hook voor HF token
- Stub catalog fixtures
- APIs: CRUD skeleton + search stub
- **Done when:** pin + list + empty job state machine werkt offline

### D1 — Acquire
- Hub provider (`stub` | `hf`); parallel download; checksums; resume; disk budget
- Manifest + object/parquet paths
- **Done when:** gated+ungated fixture acquire idempotent is; revision pin hard is

### D2 — Schema-agent + recipes
- Features read + stratified sample
- Role/modality heuristics + LM assist
- Recipe JSON Schema + verifier
- Human gate status `await_approval`
- **Done when:** sample report + editable recipe + verify/approve path groen is

### D3 — Engine core
- Step registry; normalize; langdetect; exact dedupe; checkpoints; cache keys
- Tick runner met budgets
- **Done when:** sample→approve→full op fixture Parquet reproduceerbaar is (byte-stable outputs)

### D4 — Near-dup + quality + decontaminate
- MinHash-LSH; quality classifier; eval-guard index; decontaminate step
- **Done when:** contaminatie tegen fixture evalset gedetecteerd wordt; export geblokkeerd zonder override

### D5 — Enrichment ladder
- Title/summary/tags/domain/difficulty; tokenizer registry; cost ledger; verifier+cache
- Distill path: large labels → small model train export (hergebruik training_examples)
- **Done when:** zelfde input_hash nooit dubbel large-model kost

### D6 — Graph light + serving
- Structurele + overlap edges; semantic ANN (in-DB); hybrid search; MCP tools v1
- Auditor steekproef API
- **Done when:** `neighbors` + `search_datasets` nuttig op ≥2 pinned datasets

### D7 — Mix compiler + datakaart
- Weighted mix, curriculum, export shards, automatic datamap artifact
- Golden evals + recipe regression harness
- **Done when:** één compile → tokenized shards + manifest + cost report

### D8 — UI page
- Consume D0–D7 contracts; graphics uit latere upload
- **Done when:** Hub → sample → approve → full → explore → mix zonder UI-leugens

### D9 — Scale adapters (optioneel)
- Ray / Qdrant / Neo4j / lakeFS achter feature flags + metingen
- **Done when:** adapters achterzelfde traits; local path blijft default

---

## 13. Non-goals (bewust)

- Geen aparte dataset-microservice of Airflow/Dagster/Prefect-control-plane.
- Geen background full-corpus embed bij acquire.
- Geen Neo4j/Qdrant/lakeFS in D0–D7 defaults.
- Geen ethiek-theater; wél license/PII/decontaminatie als **technische gates**.
- Geen pixel-UI in deze planfase.
- Geen silent re-download bij floating `main` — altijd pin.

---

## 14. Succescriteria (wereldklasse)

1. **Pin integrity:** twee acquires van dezelfde revision → identieke root manifest hash.
2. **Idempotence:** herhaalde full run zonder recipe-change → 100% step cache hits.
3. **Gate:** full run onmogelijk zonder `approve` na sample (tenzij expliciete dev flag + trace).
4. **Decontaminatie:** bekende eval-leak in fixture → detect + block export.
5. **Cost:** enrichment ladder toont ≥X% large-model calls vermeden vs naïve large-only op golden set.
6. **Lineage:** van export shard → record → step → recipe → revision → Hub id, volledig traverseerbaar.
7. **Local dignity:** stub provider + fixtures → D0–D7 path zonder netwerk.
8. **Engine swap:** metadata SQLite→Postgres zonder API-breuk (`/v1/atlas/datasets*` stabiel).
9. **Isolation:** project A datasets niet zichtbaar in project B zonder scope.
10. **UI honesty (D8):** elk DAG-event heeft Brain `research`-achtige event-rij; geen client-side fake progress.

---

## 15. Beslissingen (nu vastleggen)

| Beslissing | Keuze |
|------------|--------|
| Orchestratie | Atlas jobs + ticks (Research-patroon), geen Dagster |
| Compute default | Polars/DuckDB/Arrow in-process |
| Bulk format | Parquet (+ Arrow streaming) |
| Metadata engine | SQLite → Postgres dual (bestaande engines) |
| Vectors | Bestaande embed path / pgvector; Qdrant later |
| Graph | `dataset_edges` SQL eerst |
| Recipes | Versioned YAML/JSON + JSON Schema; artifacts snapshot |
| Human gate | Verplicht na sample |
| Hub | `stub` + `hf` providers; token in secrets |
| Export sink | Mix compiler → objects + manifest; training_examples alleen voor labels/feedback |
| UI | Contract-first; graphics later |
| Claudes scope | Alles behouden als **capability map**; physical stack Atlas-first |

---

## 16. Kritiek op Claude — samenvatting in één alinea

Claude tekende het juiste **product**; hij tekende de verkeerde **runtime**. Ray/Dagster/Neo4j/Qdrant/lakeFS als defaults zijn een klassieke “AI data platform”-reflex en botsen met Atlas’ one-brain, request-driven, SQLite-first filosofie die we bij Stores al hard gecorrigeerd hebben. Dit plan houdt zijn acquisitie-, recipe-, ladder-, decontaminatie- en MCP-visie intact, dwingt Research-achtige ticks af, hergebruikt Store Plane M0–M5, en phase’t zware engines achter metingen — zodat Datasets de nieuwe Cursor-data-laag wordt zonder een tweede bedrijf te bouwen.

---

## 17. Implementatiestatus

| MS | Inhoud | Status |
|----|--------|--------|
| D0 | Identity + stores + stub APIs | **DONE** |
| D1 | Acquire (stub fixtures, checksums, disk budget, gated→`hf_token`) | **DONE** |
| D2 | Schema-agent + recipes + verify + human gate | **DONE** |
| D3 | Engine core + job ticks (Research-patroon) | **DONE** |
| D4 | MinHash + quality + decontaminate + eval-guards | **DONE** |
| D5 | Enrichment ladder + cache + cost ledger | **DONE** |
| D6 | `dataset_edges` + FTS search + neighbors/subgraph + tool seeds | **DONE** |
| D7 | Mix compiler + datamap artifact + contaminated block | **DONE** |
| D8 | UI page | PLAN (wacht op frontend-upload) |
| D9 | Scale adapters (Ray/Qdrant/Neo4j/lakeFS) | PLAN (optioneel) |

**Shipped API:** `/v1/atlas/datasets/*` (catalog, pin/acquire, sample/recipes/approve, jobs/tick/pause, records/facets/datamap, edges/neighbors/subgraph, mixes/compile, eval-guards).

**Bulk format nu:** JSONL shards onder `data/datasets/{id}/` (streaming line-format; Parquet crate later zonder API-breuk). Hub provider default `stub`.

> Na elke implementatie-werkbeurt: deze tabel + `docs/datanames.md` + `docs/filenames.md` bijwerken. **Source (code) is leidend.**
