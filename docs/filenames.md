# Atlas 999 — filenames (bestand ↔ architectuur)

> Na **elke** werkbeurt bijwerken. Snel “waar hoort dit bestand?”.

## Root

| Bestand / map | Architectuur | Rol |
|---------------|--------------|-----|
| `Atlas_999_React.zip` | frontend (bronarchief) | Originele chat UI-zip |
| `Research_Update.zip` | frontend (bronarchief) | Research UI-zip (Data → Research) |
| `README.md` | meta | Intro |
| `docs/datanames.md` | docs | Architectuurkaart |
| `docs/filenames.md` | docs | Deze index |
| `docs/plan-stores-architecture.md` | store-plane | Wereldklasse stores-plan (28 logical / gefaseerde engines) |
| `docs/plan-datasets-architecture.md` | datasets-plane | Datasets plane plan (D0–D9; Hub→recipe→mix) |
| `docs/engines.md` | scale-adapters | M5 engine config (Postgres/Redis/S3); datasets engine-notities |
| `docker-compose.yml` | scale-adapters | Postgres + Redis + MinIO |
| `frontend/` | frontend | React UI |
| `backend/atlas-brain/` | atlas-brain | Brain + Memory Cortex |

## Frontend

| Bestand | Architectuur | Rol |
|---------|--------------|-----|
| `frontend/src/main.tsx` | frontend | Entry |
| `frontend/src/components/AtlasWorkspace.tsx` | frontend | Shell / settings / composer / Research mount |
| `frontend/src/components/ResearchWorkspace.tsx` | frontend / research-plane | Research UI (command, phases, terminal) |
| `frontend/src/components/ChatThread.tsx` | frontend | Bubbels |
| `frontend/src/components/AtlasMark.tsx` | frontend | Merk |
| `frontend/src/lib/runtime.ts` | frontend ↔ brain | Completions adapter |
| `frontend/src/lib/brain.ts` | frontend ↔ control-plane | Settings / knowledge client |
| `frontend/src/lib/research.ts` | frontend ↔ research-plane | Types + research API client |
| `frontend/src/lib/id.ts` | frontend | UUID helper (HTTP-preview safe) |
| `frontend/src/lib/conversations.ts` | frontend | Voorbeeld + guards |
| `frontend/public/assets/atlas.css` | frontend | Basis |
| `frontend/public/assets/chat.css` | frontend | Chat + settings |
| `frontend/public/assets/research.css` | frontend | Research layout / terminal |

## Backend — core / API

| Bestand | Architectuur | Rol |
|---------|--------------|-----|
| `backend/atlas-brain/src/main.rs` | atlas-brain | Boot |
| `backend/atlas-brain/src/state.rs` | atlas-brain | AppState |
| `backend/atlas-brain/src/config.rs` | config-plane | Settings incl. working/consolidate |
| `backend/atlas-brain/src/api/mod.rs` | atlas-brain | Router |
| `backend/atlas-brain/src/api/chat.rs` | neural-pipeline | Completions + episodic write + auto-consolidate |
| `backend/atlas-brain/src/api/memory_api.rs` | memory-cortex | memory/profile/working/consolidate/migrate/forget |
| `backend/atlas-brain/src/api/knowledge.rs` | knowledge-store compat | Via governor/cortex |
| `backend/atlas-brain/src/api/config_api.rs` | config-plane | Settings HTTP |
| `backend/atlas-brain/src/api/conversations.rs` | conversation-memory | CRUD |
| `backend/atlas-brain/src/api/models.rs` | local-lm-client | Models list |
| `backend/atlas-brain/src/api/store_api.rs` | store-plane | traces / outbox / edges HTTP |
| `backend/atlas-brain/src/api/documents.rs` | document-rag | document ingest / search / CRUD |
| `backend/atlas-brain/src/api/agent_api.rs` | agent-os | tools / tasks / prompts / model registry |
| `backend/atlas-brain/src/api/eval_api.rs` | improvement-loop | evals / feedback / training / cache |
| `backend/atlas-brain/src/api/platform_api.rs` | scale-adapters | engines / projects / artifacts / costs / secrets / code |
| `backend/atlas-brain/src/api/research_api.rs` | research-plane | jobs / tick / pause / clarify |
| `backend/atlas-brain/src/lm/client.rs` | local-lm-client | LM HTTP/stub |
| `backend/atlas-brain/src/brain/pipeline.rs` | neural-pipeline | Cortex → LM + durable trace |
| `backend/atlas-brain/src/brain/research/mod.rs` | research-plane | Research module root |
| `backend/atlas-brain/src/brain/research/orchestrator.rs` | research-plane | Intake→plan→subagents→synth→verify |
| `backend/atlas-brain/src/brain/research/tools.rs` | research-plane | web.search / web.fetch (stub|http) |
| `backend/atlas-brain/src/store/research.rs` | research-plane | jobs / events / sources / scratchpad / claims |
| `backend/atlas-brain/rust-toolchain.toml` | atlas-brain | Pin stable rustc |

## Backend — Memory Cortex

| Bestand | Architectuur | Rol |
|---------|--------------|-----|
| `backend/atlas-brain/src/memory/mod.rs` | memory-cortex | Root |
| `backend/atlas-brain/src/memory/types.rs` | memory-cortex | Nodes, provenance, trace |
| `backend/atlas-brain/src/memory/write.rs` | memory-cortex | Write Governor (+ conflict/truth, procedural) |
| `backend/atlas-brain/src/memory/ingest.rs` | document-rag | Document → objects + chunks + memory |
| `backend/atlas-brain/src/memory/embed.rs` | memory-cortex | Embedders |
| `backend/atlas-brain/src/memory/attention.rs` | memory-cortex | Multi-head recall |
| `backend/atlas-brain/src/memory/moe.rs` | memory-cortex | MoE router + expert_top_k (M4) |
| `backend/atlas-brain/src/memory/working.rs` | memory-cortex | Working KV analog (M3) |
| `backend/atlas-brain/src/memory/consolidate.rs` | memory-cortex | Episode→semantic distill (M5) |
| `backend/atlas-brain/src/memory/residual.rs` | memory-cortex | ContextStream + budget |
| `backend/atlas-brain/src/memory/cortex.rs` | memory-cortex | Orchestrator hops/salience/KV |
| `backend/atlas-brain/src/memory/decay.rs` | memory-cortex | On-read decay + bump/soften |
| `backend/atlas-brain/src/store/db.rs` | store / store-plane | SQLite open + migrations |
| `backend/atlas-brain/src/store/plane.rs` | store-plane | M0 schema: messages/FTS/edges/traces/outbox |
| `backend/atlas-brain/src/store/memory.rs` | memory-cortex store | nodes, embeddings+model_id, FTS, working_memory |
| `backend/atlas-brain/src/store/conversations.rs` | conversation-memory | Types + normalized messages + FTS |
| `backend/atlas-brain/src/store/documents.rs` | document-rag | documents + chunks + FTS |
| `backend/atlas-brain/src/store/objects.rs` | document-rag | Content-addressed blob put/get |
| `backend/atlas-brain/src/store/tools.rs` | agent-os | Tool registry + policy |
| `backend/atlas-brain/src/store/tasks.rs` | agent-os | Tasks + checkpoints |
| `backend/atlas-brain/src/store/prompts.rs` | agent-os | Prompt versions + model registry |
| `backend/atlas-brain/src/store/eval.rs` | improvement-loop | eval/feedback/training/cache tables |
| `backend/atlas-brain/src/memory/eval_runner.rs` | improvement-loop | Request-driven eval suite runner |
| `backend/atlas-brain/src/store/edges.rs` | store-plane | memory_edges CRUD |
| `backend/atlas-brain/src/store/traces.rs` | store-plane | Append-only audit traces |
| `backend/atlas-brain/src/store/outbox.rs` | store-plane | Durable events (request-drain) |
| `backend/atlas-brain/src/store/platform.rs` | scale-adapters | projects / artifacts / costs / secrets / code index |
| `backend/atlas-brain/src/store/engines/` | scale-adapters | EngineBundle · Postgres · Redis · S3 · pg_mirror |
| `backend/atlas-brain/src/store/knowledge.rs` | knowledge-store | Legacy types |

## Runtime data (niet in git)

| Pad | Rol |
|-----|-----|
| `…/data/brain-config.json` | Settings (incl. research_* budgets) |
| `…/data/atlas.db` | conversations, messages, knowledge, memory_*, documents, traces, outbox, research_* |
| `…/data/objects/` | Content-addressed blobs (sha256) — ook research source fragments / reports |
| `…/data/datasets/` | JSONL shards + manifests per pinned dataset (D1+) |

## Backend — datasets-plane (D0–D7)

| Bestand | Architectuur | Rol |
|---------|--------------|-----|
| `backend/atlas-brain/src/api/datasets_api.rs` | datasets-plane | HTTP `/v1/atlas/datasets*` |
| `backend/atlas-brain/src/brain/datasets/mod.rs` | datasets-plane | Plane root |
| `backend/atlas-brain/src/brain/datasets/hub.rs` | datasets-plane | Stub catalog + acquire + pin |
| `backend/atlas-brain/src/brain/datasets/engine.rs` | datasets-plane | Recipe propose/verify, pipeline steps, mix/datamap |
| `backend/atlas-brain/src/brain/datasets/orchestrator.rs` | datasets-plane | Job ticks, human gate, costs/artifacts |
| `backend/atlas-brain/src/store/datasets.rs` | datasets-plane | Schema + CRUD (catalog→mixes) |
