# Atlas 999 — datanames (architectuurkaart)

> Na **elke** werkbeurt bijwerken. Dit is de navigatiekaart zodat AI/engineers niet het hele project hoeven te lezen.

## One-brain principe

Alles denkt via **Atlas Brain** (`backend/atlas-brain`). De React-UI is alleen het gezicht. Trading/Data/Coding/crawlers zijn later plugins op dezelfde brain — niet aparte daemons die elkaar pollen.

**Verboden:** periodieke health checks, periodieke API/model-pings, hardcoded SYSTEM_PROMPT, ethiek-/security-lagen (komen later), background embed/consolidate sweepers.

---

## Architecturen

### `frontend` — Obsidian Chat UI
- **Wat:** Pixel-exact React + Vite shell uit `Atlas_999_React.zip` (Chat, navigatie, composer).
- **Waar:** `frontend/`
- **Contract:** Brain via `/v1/models` + `/v1/chat/completions`, plus `/v1/atlas/*`.
- **State:** `localStorage` cache; bron van waarheid = Brain SQLite + Memory Cortex.

### `atlas-brain` — Brain Core (Rust)
- **Wat:** Request-driven HTTP-daemon. Geen background loops.
- **Waar:** `backend/atlas-brain/`
- **Bind:** default `127.0.0.1:8080` (`brain-config.json`).
- **Data:** `ATLAS_DATA_DIR` of `./data` → `brain-config.json` + `atlas.db`.

### `local-lm-client` — Lokale LM-adapter
- **Wat:** HTTP-client naar lokale OpenAI-compat LM (Ollama / llama.cpp / LM Studio). Geen cloud-OpenAI.
- **Providers:** `local_http` | `stub`
- **Wanneer:** alleen bij `/v1/models` of `/v1/chat/completions`.

### `config-plane` — Runtime settings
- **API:** `GET/PUT /v1/atlas/settings`
- **Velden o.a.:** SYSTEM_PROMPT, model_id, lm_*, embed_*, knowledge_top_k, memory_token_budget, memory_hops
- **Persist:** `data/brain-config.json`
- **Regel:** geen policy-tekst hardcoded in codepaden.

### `conversation-memory`
- **API:** `GET/POST /v1/atlas/conversations`, `GET/DELETE /v1/atlas/conversations/{id}`
- **Store:** SQLite `conversations`
- **Chat:** completions kunnen `conversation_id` / `title` / `persist` meekrijgen.

### `memory-cortex` — Memory v2 (expert stack)
One-brain residual memory naast het LM (LM blijft de transformer + LM-head).

**Request-pipeline (per completion / recall):**
1. Write Governor (bij learn) — classify → chunk → dedupe/conflict → commit  
2. Router (query → expert mix)  
3. Multi-head attention — `semantic` | `episodic` | `self_profile` | `lexical` | `recency`  
4. Fuse logits → gate  
5. Optional hop 2 (query expand)  
6. Residual ContextStream + **token-budget**  
7. Local LM completion  
8. Salience write-back (on-request)

**Experts / kinds:** `working` · `episodic` · `semantic` · `self_profile` · (`procedural` later)

**Embedding:** `embed_provider` = `stub`/`local_hash` (deterministisch) of `local_http` (`POST …/embeddings`)

**Decay/forget:** on-read salience decay + `DELETE /v1/atlas/memory/{id}` (archive/delete). Geen sweeper-loop.

**Provenance per node:** source, created_at, parent_ids, confidence, scope, conversation_id

**API:**
- `GET/POST /v1/atlas/memory` — list / learn (Write Governor)
- `POST /v1/atlas/memory/recall` — fused hits; `trace:true` voor forensics
- `POST /v1/atlas/memory/migrate` — legacy knowledge → memory_nodes (+ embed on-demand)
- `GET/POST /v1/atlas/memory/profile` — self/profile
- `DELETE /v1/atlas/memory/{id}` — forget

**Store:** SQLite `memory_nodes` + `memory_embeddings`

### `knowledge-store` — Compat-laag
- **API:** `/v1/atlas/knowledge*` blijft werken.
- **Learn/recall** routeren via Write Governor / Memory Cortex.
- Semantic writes spiegelen ook legacy `knowledge`-tabel.

### `neural-pipeline` — Context assembly
- **Code:** `brain/pipeline.rs` + `memory/cortex.rs` + `memory/residual.rs`
- **Volgorde residual frames:** SYSTEM_PROMPT → self/profile → (router) → semantic → episodic → working → conversation  
- Token-budget snijdt lage prioriteit (schatting chars/4).

---

## Gereserveerd (nog niet / later)

| Naam | Intentie |
|------|----------|
| `memory-moe-hard` | M4: zwaardere MoE + salience tuning |
| `memory-consolidate` | M5: episode→semantic distill via LM |
| `trading-engine` | Plugin op brain |
| `data-workspace` / `coding-workspace` | Later |
| `crawler-orchestrator` | On-demand via brain |
| `ethics-layer` / `security-layer` | Laatst |
| `cnn-memory` / `rnn-episode` | Later |

---

## Omgeving / start

```bash
cd backend/atlas-brain
ATLAS_DATA_DIR=./data cargo run --release

cd frontend
npm install && npm run dev
```

Settings → Brain `http://127.0.0.1:8080/v1`, model-ID, lokale LM-URL, SYSTEM_PROMPT → **Verbinden**.  
Voor offline pipeline-tests: `lm_provider=stub`, `embed_provider=stub`.
