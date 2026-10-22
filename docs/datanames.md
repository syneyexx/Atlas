# Atlas 999 — datanames (architectuurkaart)

> Na **elke** werkbeurt bijwerken. Dit is de navigatiekaart zodat AI/engineers niet het hele project hoeven te lezen.

## One-brain principe

Alles denkt via **Atlas Brain** (`backend/atlas-brain`). De React-UI is alleen het gezicht. Trading/Data/Coding/crawlers zijn later plugins op dezelfde brain — niet aparte daemons die elkaar pollen.

**Verboden:** periodieke health checks, periodieke API/model-pings, hardcoded SYSTEM_PROMPT, ethiek-/security-lagen (komen later).

---

## Architecturen

### `frontend` — Obsidian Chat UI
- **Wat:** Pixel-exact React + Vite shell uit `Atlas_999_React.zip` (Chat, navigatie, composer).
- **Waar:** `frontend/`
- **Contract:** praat met Brain via OpenAI-vormige `/v1/models` + `/v1/chat/completions`, plus Atlas control-plane `/v1/atlas/*`.
- **State:** lokale cache in `localStorage` (`atlas999.conversations`, `atlas999.runtime`); bron van waarheid voor geheugen is Brain SQLite.

### `atlas-brain` — Brain Core (Rust)
- **Wat:** Request-driven HTTP-daemon. Geen background loops.
- **Waar:** `backend/atlas-brain/`
- **Bind:** default `127.0.0.1:8080` (uit `brain-config.json`).
- **Data:** `ATLAS_DATA_DIR` of `./data` → `brain-config.json` + `atlas.db`.

### `local-lm-client` — Lokale LM-adapter
- **Wat:** HTTP-client naar een **lokale** OpenAI-compatibele LM (Ollama / llama.cpp server / LM Studio). Geen cloud-OpenAI.
- **Providers (settings):**
  - `local_http` — echte lokale LM op `lm_base_url`
  - `stub` — offline pipeline-test (geen model nodig)
- **Wanneer actief:** alleen bij `/v1/models` of `/v1/chat/completions` (nooit op een timer).

### `config-plane` — Runtime settings
- **Wat:** SYSTEM_PROMPT, model_id, lm_base_url, lm_provider, knowledge_top_k/max_chars.
- **API:** `GET/PUT /v1/atlas/settings`
- **Persist:** `data/brain-config.json`
- **Regel:** niets hiervan hardcoden in codepaden als beleidstekst.

### `conversation-memory`
- **Wat:** Persistente gesprekken + turns.
- **API:** `GET/POST /v1/atlas/conversations`, `GET/DELETE /v1/atlas/conversations/{id}`
- **Store:** SQLite tabel `conversations`
- **Chat-pad:** completions kunnen `conversation_id` / `conversation_title` / `persist` meekrijgen.

### `knowledge-store` — Leren & oproepen
- **Wat:** Kennisnodes (title/content/tags) + on-demand lexical recall (cosine over log-TF tokens).
- **API:**
  - `POST /v1/atlas/knowledge` — leren
  - `POST /v1/atlas/knowledge/recall` — oproepen
  - `GET /v1/atlas/knowledge` — lijst
  - `DELETE /v1/atlas/knowledge/{id}`
- **Store:** SQLite tabel `knowledge`
- **Injectie:** Brain pipeline voegt top-k hits toe als system-block `[ATLAS_KNOWLEDGE]…[/ATLAS_KNOWLEDGE]` vóór de LM-call.

### `neural-pipeline` — Context assembly
- **Volgorde (one-brain):**
  1. `SYSTEM_PROMPT` (uit settings; mag leeg)
  2. Recalled knowledge block (als `knowledge_top_k > 0`)
  3. Conversation turns (user/assistant)
  4. Local LM completion
- **Code:** `backend/atlas-brain/src/brain/pipeline.rs` + `recall.rs`

---

## Gereserveerd (nog niet gebouwd)

| Naam | Intentie |
|------|----------|
| `trading-engine` | Later — plugin op brain, geen eigen health loops |
| `data-workspace` | Later — ingest/pipelines |
| `coding-workspace` | Later |
| `crawler-orchestrator` | Later — aangestuurd door brain, on-demand |
| `embedding-neural` | Later — vervangt/ versterkt lexical recall |
| `ethics-layer` / `security-layer` | Laatst |

---

## Omgeving / start

```bash
# Brain
cd backend/atlas-brain
ATLAS_DATA_DIR=./data cargo run --release

# UI
cd frontend
npm install
npm run dev
```

UI Settings → Brain API `http://127.0.0.1:8080/v1`, model-ID, lokale LM-URL, SYSTEM_PROMPT → **Verbinden**.
