# Atlas 999 — datanames (architectuurkaart)

> Na **elke** werkbeurt bijwerken. Dit is de navigatiekaart zodat AI/engineers niet het hele project hoeven te lezen.

## One-brain principe

Alles denkt via **Atlas Brain** (`backend/atlas-brain`). De React-UI is alleen het gezicht. Trading/Data/Coding/crawlers zijn later plugins op dezelfde brain — niet aparte daemons die elkaar pollen.

**Verboden:** periodieke health checks, periodieke API/model-pings, hardcoded SYSTEM_PROMPT, ethiek-/security-lagen (komen later), background embed/consolidate/TTL sweepers.

---

## Architecturen

### `frontend` — Obsidian Chat UI
- **Wat:** Pixel-exact React + Vite shell uit `Atlas_999_React.zip` + Research uit `Research_Update.zip`.
- **Waar:** `frontend/`
- **Contract:** `/v1/models`, `/v1/chat/completions`, `/v1/atlas/*` (incl. research).
- **State:** `localStorage` cache; bron van waarheid = Brain SQLite + Memory Cortex.
- **Research UI:** Data → Research submenu; thin client (start/pause/clarify + event stream).

### `atlas-brain` — Brain Core (Rust)
- **Wat:** Request-driven HTTP-daemon. Geen background loops.
- **Waar:** `backend/atlas-brain/`
- **Bind:** default `127.0.0.1:8080` (`brain-config.json`).
- **Data:** `ATLAS_DATA_DIR` of `./data` → `brain-config.json` + `atlas.db`.

### `local-lm-client` — Lokale LM-adapter
- **Providers:** `local_http` | `stub`
- **Wanneer:** alleen bij `/v1/models`, `/v1/chat/completions`, of on-demand consolidate.

### `config-plane` — Runtime settings
- **API:** `GET/PUT /v1/atlas/settings`
- **Velden o.a.:** SYSTEM_PROMPT, model/lm/embed, knowledge_top_k, memory_token_budget, memory_hops, working_ttl_secs, memory_auto_consolidate, consolidate_prompt
- **Persist:** `data/brain-config.json`

### `conversation-memory`
- **API:** `/v1/atlas/conversations*`
- **Store:** SQLite `conversations` + genormaliseerde `messages` (+ `messages_fts`)
- **Bron van waarheid:** `messages` rijen; `messages_json` is denormalized mirror

### `memory-cortex` — Memory v2 (M0–M5)
One-brain residual memory naast het LM.

**Request-pipeline:**
1. Write Governor (learn / chat episodic)  
2. MoE router (normalized expert mix + working continuity)  
3. Multi-head attention — semantic · episodic · self_profile · lexical · recency · **working**  
4. Fuse → gate (expert_top_k schaalt head capacity)  
5. Optional hop 2  
6. Residual ContextStream + token-budget  
7. Local LM completion  
8. Salience bump (used) / soften (near-miss)  
9. Working KV save (TTL refresh on read/write)  
10. Optional auto-consolidate (settings)

**Experts:** working · episodic · semantic · self_profile · **procedural**

**Conflict/truth (M1):** Write Governor detecteert divergente claims (sim ≥ conflict_low, < dedupe) → beide nodes + `conflicts_with` edge.

**Working KV (M3):** per `conversation_id` — pins, recent hits, last router/query, embedding blob; expiry **on-read** only.  
API: `GET /v1/atlas/memory/working/{conversation_id}`

**MoE / salience (M4):** `memory/moe.rs` normalize + continuity; write-back bump/soften.

**Consolidate (M5):** episode→semantic distill on-demand.  
API: `POST /v1/atlas/memory/consolidate`  
Stub-LM: deterministische distill uit brontekst.  
`consolidate_prompt` is settings (technisch), niet SYSTEM_PROMPT-ethiek.

**Overige API:**
- `GET/POST /v1/atlas/memory` · `POST …/recall` (`trace`) · `POST …/migrate`
- `GET/POST …/profile` · `DELETE …/{id}` forget

**Store:** `memory_nodes` · `memory_embeddings` · `working_memory` · `memory_edges` · FTS · `traces` · `outbox`

### `store-plane` — Logische stores (plan → M0–M5 done)
Wereldklasse stores-architectuur: **28 logische stores**, engines gefaseerd (SQLite → Postgres/Redis/S3).

- **Plan:** `docs/plan-stores-architecture.md` · **Engines:** `docs/engines.md`
- **M0–M5 shipped** — SQLite default; dual-write Postgres + Redis + S3/MinIO
- **API surface:** traces · outbox · edges · documents · tools · tasks · prompts · models/registry · evals · feedback · training · cache · engines · projects · artifacts · costs · secrets · code
- **Hybrid recall:** α·vector + β·lex + γ·BM25; hop-2 volgt edge-neighbors
- **Outbox:** drain on chat/ingest (+ expliciete POST)

### `scale-adapters` — Physical engines (M5)
- **Postgres:** schema migrate + dual-write mirror van kritische stores
- **Redis:** working memory + semantic cache hot path
- **S3/MinIO:** content-addressed objects (+ local mirror)
- **Code index:** `code_files` / `code_symbols` + FTS search
- **Status:** `GET /v1/atlas/engines`

### `agent-os` — Aansturing (M3)
- **Tools:** versioned schemas + policy `allow|deny|ask` + capabilities; builtins seeded (`memory.*`, `documents.ingest`, `web.search`, `web.fetch`, `research.run`)
- **Tasks:** queue/running/blocked/completed + checkpoints (snapshot JSON); research parent + child subagents via `parent_id`
- **Prompts:** versioned `system_prompt`; snapshot on settings PUT
- **Models:** registry synced from settings model_id

### `research-plane` — Multi-agent research (one-brain)
Request-driven research orchestrator in Atlas Brain — geen aparte daemon, geen background loops.

**Flow:** intake (1–3 clarifiers) → lead plan → parallel subagents (ticks) → lead review / gaps → synthese → citation verify → rapport + Memory Cortex write + artifact.

**API:**
- `POST/GET /v1/atlas/research/jobs`
- `GET /v1/atlas/research/jobs/{id}`
- `POST …/tick` (1–32 steps, request-scoped)
- `POST …/pause` · `POST …/clarify`

**Stores:** `research_jobs` · `research_events` · `research_sources` · `research_scratchpad` · `research_claims`

**Tools:** `web.search` / `web.fetch` — provider `stub` (offline corpus) of `http` (`research_search_url`). SEO-tier bronnen worden genegeerd.

**Budgets (config):** `research_max_subagents` · `research_max_rounds` · `research_max_sources` · `research_max_ticks`

**Integratie:** Agent-OS tasks, objects (content hash), artifacts, Memory Cortex (semantic learn), outbox events `research.created` / `research.completed`.

### `improvement-loop` — Controle (M4)
- **Evals:** cases + request-driven runner (`POST /v1/atlas/evals/run`) → scores + traces
- **Feedback:** up/down/correction; corrections → training_examples
- **Semantic cache:** opt-in (`semantic_cache_enabled`); invalidate on prompt/model change
- **Datasets-koppeling (gepland):** training_examples = label/feedback-sink; trainingsmix-export komt uit Datasets mix compiler — zie `docs/plan-datasets-architecture.md`

### `document-rag` — Document plane (M2)
- Ingest: object hash (sha256) → chunks → semantic memory nodes + `has_chunk` edges
- Search: FTS5 op chunks; recall via Memory Cortex (semantic heads)
- **Niet verwarren met Datasets:** documents = RAG-ingest van tekst/files; datasets = Hub/Parquet acquisitie → recipe → trainingscorpus

### `datasets-plane` — Data → Datasets (D0–D7 shipped)
Cursor-klasse data-OS op één brain: Hub/local acquisitie → schema-agent/recipe → request-driven verwerkings-DAG → verrijking (kostenladder) → relatiegraaf → mix-export.

- **Plan:** `docs/plan-datasets-architecture.md` (D0–D9)
- **Status:** **D0–D7 DONE** in `atlas-brain`; D8 UI wacht op frontend-upload; D9 scale optioneel
- **API:** `/v1/atlas/datasets/catalog/*` · `POST/GET /datasets` · `…/acquire` · `…/sample` · recipes/verify/approve · jobs/tick/pause · records/facets/datamap · edges/neighbors/subgraph · mixes/compile · eval-guards
- **Invariants:** Research-achtige ticks (geen ETL-daemon); JSONL streaming in-process; SQLite + `data/datasets/`; geen Dagster/Ray/Neo4j defaults
- **Stores:** `dataset_catalog_cache` · `datasets` · `dataset_shards` · `dataset_recipes` · `dataset_jobs` · `dataset_events` · `dataset_records` (+ FTS) · `dataset_edges` · `enrichment_cache` · `eval_guards` · `dataset_mixes`
- **Tools (seeded):** `datasets.search` · `acquire` · `run` · `get_record` · `neighbors` · `subgraph` · `compile_mix`
- **Human gate:** verplicht na steekproef (`await_approval`), vóór full run
- **UI:** graphics later; contracts live
- **Productpositie:** onder Data naast Research (Research = web→rapport; Datasets = corpus→mix)

### `knowledge-store` — Compat
`/v1/atlas/knowledge*` → Write Governor / Cortex.

### `neural-pipeline`
`brain/pipeline.rs` + `memory/cortex.rs` + `residual.rs`  
Frames: SYSTEM_PROMPT → self → router? → semantic → procedural → episodic → working → conversation

---

## Gereserveerd (plugins / later)

| Naam | Intentie |
|------|----------|
| `trading-engine` / workspaces / crawlers | Plugins op dezelfde brain |
| `data-workspace` UI | Product shell: Research live; Datasets API live (D0–D7); **UI D8** volgt upload |
| `coding-workspace` UI | Product shell (code-index API al live) |
| `pdf.read` / `code.sandbox` research tools | Fase 2 research tools |
| `ethics-layer` / `security-layer` | Laatst |
| `graph-scale` (AGE/Neo4j) | Alleen bij meetbare edge-volume (ook datasets D9) |
| `datasets-scale` (Ray / Qdrant / lakeFS) | Alleen D9 achter metingen |
| `cnn-memory` / `rnn-episode` | Experimenteel |

---

## Start

```bash
cd backend/atlas-brain && ATLAS_DATA_DIR=./data cargo run --release
cd frontend && npm install && npm run dev
```

Offline: `lm_provider=stub`, `embed_provider=stub`.
