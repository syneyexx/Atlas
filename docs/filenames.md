# Atlas 999 — filenames (bestand ↔ architectuur)

> Na **elke** werkbeurt bijwerken. Snel “waar hoort dit bestand?”.

## Root

| Bestand / map | Architectuur | Rol |
|---------------|--------------|-----|
| `Atlas_999_React.zip` | frontend (bronarchief) | Originele goedgekeurde UI-zip |
| `README.md` | meta | Korte projectintro |
| `docs/datanames.md` | docs | Architectuurkaart |
| `docs/filenames.md` | docs | Deze bestandsindex |
| `frontend/` | frontend | Actieve React UI |
| `backend/atlas-brain/` | atlas-brain | Rust Brain Core + Memory Cortex |

## Frontend

| Bestand | Architectuur | Rol |
|---------|--------------|-----|
| `frontend/src/main.tsx` | frontend | React entry |
| `frontend/src/components/AtlasWorkspace.tsx` | frontend | Shell, settings, kennis-UI, composer |
| `frontend/src/components/ChatThread.tsx` | frontend | Chatbubbels / acties |
| `frontend/src/components/AtlasMark.tsx` | frontend | SVG-merk |
| `frontend/src/lib/runtime.ts` | frontend ↔ atlas-brain | `/v1/models` + `/v1/chat/completions` |
| `frontend/src/lib/brain.ts` | frontend ↔ control-plane | settings / conversations / knowledge client |
| `frontend/src/lib/conversations.ts` | frontend | Voorbeeldgesprek + typeguards |
| `frontend/public/assets/atlas.css` | frontend | Basislayout |
| `frontend/public/assets/chat.css` | frontend | Chat-geometry + settings/kennis |
| `frontend/package.json` | frontend | React 19 / Vite 8 |

## Backend — core

| Bestand | Architectuur | Rol |
|---------|--------------|-----|
| `backend/atlas-brain/Cargo.toml` | atlas-brain | Crate + deps |
| `backend/atlas-brain/src/main.rs` | atlas-brain | Boot, CORS, bind, shutdown |
| `backend/atlas-brain/src/state.rs` | atlas-brain | AppState (config, store, lm, embed) |
| `backend/atlas-brain/src/error.rs` | atlas-brain | Fouttype → HTTP JSON |
| `backend/atlas-brain/src/config.rs` | config-plane | Settings + memory/embed knobs |
| `backend/atlas-brain/src/api/mod.rs` | atlas-brain | Router (geen `/health`) |
| `backend/atlas-brain/src/api/models.rs` | local-lm-client | `GET /v1/models` |
| `backend/atlas-brain/src/api/chat.rs` | neural-pipeline + conversation-memory | `POST /v1/chat/completions` |
| `backend/atlas-brain/src/api/config_api.rs` | config-plane | `GET/PUT /v1/atlas/settings` |
| `backend/atlas-brain/src/api/conversations.rs` | conversation-memory | CRUD gesprekken |
| `backend/atlas-brain/src/api/knowledge.rs` | knowledge-store compat | learn/recall via cortex |
| `backend/atlas-brain/src/api/memory_api.rs` | memory-cortex | memory / profile / migrate / forget |
| `backend/atlas-brain/src/lm/client.rs` | local-lm-client | Local LM HTTP of stub |
| `backend/atlas-brain/src/brain/pipeline.rs` | neural-pipeline | Orchestrator → cortex → LM |
| `backend/atlas-brain/src/brain/recall.rs` | legacy lexical | Bewaard als referentie/fallback-module |

## Backend — Memory Cortex

| Bestand | Architectuur | Rol |
|---------|--------------|-----|
| `backend/atlas-brain/src/memory/mod.rs` | memory-cortex | Module root |
| `backend/atlas-brain/src/memory/types.rs` | memory-cortex | Node, provenance, hits, trace, requests |
| `backend/atlas-brain/src/memory/write.rs` | memory-cortex | Write Governor (dedupe/chunk/commit) |
| `backend/atlas-brain/src/memory/embed.rs` | memory-cortex | Embed client (`stub`/`local_hash`/`local_http`) |
| `backend/atlas-brain/src/memory/attention.rs` | memory-cortex | Multi-head recall + router + fuse |
| `backend/atlas-brain/src/memory/residual.rs` | memory-cortex | ContextStream + token-budget |
| `backend/atlas-brain/src/memory/cortex.rs` | memory-cortex | Hops + salience write-back |
| `backend/atlas-brain/src/memory/decay.rs` | memory-cortex | On-read salience/decay |
| `backend/atlas-brain/src/store/db.rs` | store | SQLite open + legacy tables |
| `backend/atlas-brain/src/store/memory.rs` | memory-cortex store | memory_nodes / embeddings / migrate |
| `backend/atlas-brain/src/store/conversations.rs` | conversation-memory | Message types |
| `backend/atlas-brain/src/store/knowledge.rs` | knowledge-store | Legacy knowledge types |

## Runtime data (niet in git)

| Pad | Architectuur | Rol |
|-----|--------------|-----|
| `backend/atlas-brain/data/brain-config.json` | config-plane | Persistente settings |
| `backend/atlas-brain/data/atlas.db` | store | conversations, knowledge, memory_nodes, embeddings |
