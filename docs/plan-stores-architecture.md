# Atlas Brain — Stores Architecture Plan (World-Class)

> Planmodus deliverable. Doel: één AI-framework op Cursor-niveau, gebouwd op **logische stores** en **weinige engines**, met Atlas’ one-brain / request-driven invariants intact.

---

## 1. Verdict op het voorstel

**Sterk.** De 20 logische stores zijn een correcte cognitieve + product-taxonomie. De scheiding “20 stores ≠ 20 systemen” is precies hoe productie-AI-stacks moeten denken.

**Niet wereldklasse genoeg als je dit 1:1 bouwt.** Drie fouten zouden het kapot maken:

1. **Te vroeg 4 engines** — voor Atlas (local-first, one-brain) is Postgres+Redis+Neo4j+S3 overkill en schendt de “één brain, geen daemons”-filosofie. Engines volgen schaal, niet taxonomy.
2. **Conversation als blob** — volledige `messages_json` blokkeert FTS, traces, citations, partial replay. Berichten moeten first-class rijen zijn.
3. **Knowledge graph als Neo4j-default** — premature. Edge-tabel + hop-queries in dezelfde DB als memory dekt 90% tot je miljoenen relaties hebt.

**Richting:** taxonomy behouden, **5 ontbrekende stores toevoegen**, engines **gefaseerd** kiezen, en alles onder één **Write Governor + Identity/Scope plane**.

---

## 2. Wat er al goed is (behouden)

| # | Store | Waarom dit klopt |
|---|--------|------------------|
| 1 | Conversation | Bron van waarheid voor dialoog |
| 2 | Working memory | Hot path; kortlevend |
| 3–5 | Episodic / Semantic / Procedural | Klassieke memory-splitsing; Atlas Memory Cortex heeft dit al deels |
| 6 | Profile/preferences | Self-model + UX |
| 7–9 | Documents + vector + BM25 | Hybrid RAG is non-negotiable |
| 10 | Knowledge graph | Relaties > platte feiten |
| 11–14 | Tools / tasks / prompts / config | Control plane van een agent-OS |
| 15–18 | Trace / eval / feedback / training | Verbeterlus = product-moat |
| 19–20 | Semantic cache / object storage | Latency + multimodale artifacts |

Atlas heeft vandaag al: conversation + memory_nodes (working/episodic/semantic/self_profile) + embeddings + config-plane + residual pipeline. Dat is Phase-0 kapitaal — niet weggooien voor een greenfield Postgres-migratie.

---

## 3. Wat mist (toevoegen)

Zonder deze stores bouw je een chat-met-geheugen, geen agent-OS.

| # | Nieuwe store | Waarom Cursor-klasse |
|---|--------------|----------------------|
| 21 | **Workspace / Project store** | Scope-hiërarchie: global → project → conversation. Zonder dit lekt memory tussen projecten. |
| 22 | **Artifact / Checkpoint store** | Diffs, file snapshots, tool outputs, “restore to here”. Los van ruwe blobs. |
| 23 | **Code / AST index** | Coding-agent vereist symbol/path index, niet alleen document-chunks. |
| 24 | **Embedding & model version registry** | Embeddings zonder model-version zijn giftig bij modelwissel. Pin + reindex policy. |
| 25 | **Cost / quota ledger** | Tokens, latency, $ per turn. Zonder dit geen product-ops. |
| 26 | **Outbox / durable events** | Request-driven, maar transactioneel: “memory written → later consolidate”. Geen sweeper-daemon; drain on next request of explicit API. |
| 27 | **Policy / capability store** | Toolrechten runtime (niet alleen schema’s). Allow/deny, scopes, human-approve. |
| 28 | **Truth / conflict store** | Contradiction handling naast semantic memory (Write Governor heeft dit al deels nodig). |

Optioneel later (niet in MVP): multi-device sync log, feature-flag experiments, approval queue UI.

**Totaal logisch: 28 stores, nog steeds 1–4 engines.**

---

## 4. Kritische verbeteringen op jouw 20

### 4.1 Conversation store → genormaliseerd
- Tabellen: `conversations`, `messages`, `message_parts` (text/tool/image), `message_citations`.
- FTS op message content.
- Elke message heeft `trace_id`, `model_id`, `token_in/out`.

### 4.2 Working memory ≠ Redis-verplicht
- **Local:** in-process + SQLite `memory_kind=working` met TTL/salience (Atlas nu).
- **Cloud multi-instance:** Redis of equivalent.
- Semantic cache (19) mag Redis; working memory mag co-loceren tot schaal pijn doet.

### 4.3 Procedural memory ≠ Tool registry
- **Procedural (5):** geleerde werkwijzen (“hoe fix ik Vite CORS in dit repo”).
- **Tool registry (11):** machine-leesbare schemas + permissions.
- Koppeling via IDs, niet samenvoegen.

### 4.4 Profile vs Semantic
- Profile = stabiele self/user prefs (lage churn, hoge prioriteit in residual).
- Semantic = feiten over wereld/project (hogere churn, conflict-gevoelig).
- Atlas’ `self_profile` expert behouden als first-class head.

### 4.5 Secrets apart van Config
- Config (14): niet-geheim JSON (model_id, budgets, top_k).
- Secrets: encrypted-at-rest, nooit in traces/evals, aparte access path.
- Local: OS keychain / encrypted file; Cloud: KMS.

### 4.6 Knowledge graph licht starten
- `memory_edges(src, rel, dst, weight, provenance)` in dezelfde DB.
- Hop-2 recall (Atlas heeft dit al in cortex) leest edges.
- Neo4j/AGE alleen als edge-query latency of volume dat eist.

### 4.7 Semantic cache met harde invalidatie
- Key = hash(model + prompt_template_version + tools_hash + retrieval_fingerprint + user_msg).
- Invalidate op: prompt/model change, memory write in betrokken scope, tool schema change.

### 4.8 Trace is de spine
- Elke completion produceert één immutable trace: router mix → hits → frames → tool I/O → tokens → latency.
- Eval/feedback/training refereren `trace_id`. Zonder dit geen verbeterlus.

---

## 5. Engines — gefaseerd (Atlas-first)

| Fase | Engines | Stores |
|------|---------|--------|
| **A — Local Brain (nu)** | **SQLite** (FTS5 + sqlite-vec of blob embeddings) + **filesystem** blobs | Alles behalve schaal-Redis/graph |
| **B — Local+** | + optionele embedded Redis-achtige cache alleen als metingen het eisen | 2, 19 |
| **C — Team / Cloud** | **Postgres** (pgvector + FTS + JSONB) + **S3/MinIO** + **Redis** | Migratie van A; zelfde logische schema |
| **D — Graph at scale** | Postgres AGE **of** Neo4j | Alleen store 10 als edges exploderen |

**Regel:** logisch schema is engine-agnostisch. Store-traits in Rust (`ConversationStore`, `MemoryStore`, …) met SQLite-impl eerst, Postgres-impl later.

Dit respecteert Atlas invariants: request-driven, geen health-ping daemons, één brain.

---

## 6. Identity & Scope plane (verplicht fundament)

Elke rij in bijna elke store draagt:

```
tenant_id? → user_id → project_id? → conversation_id? → trace_id?
```

Scopes (al in Atlas provenance): `global | project | conversation`.

**ACL:** policy store beslist wat recall mag zien. Working/episodic default conversation-scoped; semantic mag project; profile user-scoped.

Zonder deze plane faalt multi-project en later multi-user hard.

---

## 7. Write paths (één deur per domein)

| Domein | Enige schrijfpad | Guarantees |
|--------|------------------|------------|
| Memory | Write Governor | classify → chunk → dedupe/conflict → embed(version) → commit + outbox |
| Documents | Ingest pipeline | parse → chunk → FTS + vector + optional entities/edges |
| Tools | Registry API + signed schemas | versioned; old calls pinned |
| Agent state | Task runtime | checkpoint vóór elke side-effect |
| Feedback/Eval | Explicit APIs | immutable scores; link to trace |

**Verboden:** ad-hoc `INSERT` vanuit chat handlers buiten governor/ingest.

---

## 8. Read path (Memory Cortex = product-differentiator)

Behoud en versterk de bestaande pipeline:

1. Query → Router (expert mix)
2. Heads: semantic · episodic · self_profile · lexical/BM25 · recency · (+ procedural, + graph hop)
3. Fuse → gate → token-budget residual
4. LM completion
5. Salience write-back + optional outbox “distill later”

Hybrid score (wereldklasse default):

```
score = α·vector + β·bm25 + γ·recency + δ·salience + ε·graph_prior
```

α…ε zijn config-plane knobs (al deels aanwezig), niet hardcoded.

---

## 9. Roadmap (technisch, geen kalender)

### Milestone 0 — Schema hardening (op huidige SQLite)
- Normaliseer messages
- Message + memory FTS5
- `memory_edges` tabel
- `embedding_model_id` kolom
- `traces` tabel (JSON append-only)
- Outbox tabel (pending events)

### Milestone 1 — Memory completeness
- Procedural kind end-to-end
- Conflict/truth path in Write Governor
- Session-end distill (on-request, geen sweeper): episodic → semantic candidates
- Profile API aangescherpt (prefs vs facts)

### Milestone 2 — Knowledge plane
- Document store + chunker
- Hybrid recall (vector + FTS) in cortex lexical/semantic heads
- Object storage voor attachments (local `data/objects/`)

### Milestone 3 — Agent OS
- Tool registry + policy
- Task/agent state + checkpoints + subagent tree
- Prompt/model registry (versioned SYSTEM_PROMPT — al deels config-plane)
- Artifact/checkpoint store

### Milestone 4 — Improvement loop
- Eval store + runner (request-triggered suites)
- Feedback → training dataset export
- Cost ledger in traces
- Semantic cache (opt-in)

### Milestone 5 — Scale adapters
- Postgres backend achter dezelfde traits
- Redis voor working/cache wanneer multi-instance
- AGE/Neo4j alleen met meetbare noodzaak
- Code/AST index plugin voor coding-workspace

---

## 10. Non-goals (bewust)

- Geen 20 microservices.
- Geen background embed/consolidate sweepers (Atlas verbod) — wel outbox drained by next request / explicit `/v1/atlas/memory/consolidate`.
- Geen ethiek-/security-theater vroeg; policy store wél vroeg voor tool-capabilities.
- Geen cloud-OpenAI-eis; local LM blijft first-class.

---

## 11. Succescriteria (wereldklasse)

1. **Recall quality:** hybrid > vector-only op vaste eval-set (eval store).
2. **Provenance:** elk residual frame traceerbaar naar node/doc/tool.
3. **Isolation:** project A memory nooit in project B zonder expliciete scope.
4. **Replay:** gegeven `trace_id`, reconstructeer context + tool I/O.
5. **Upgrade safety:** embedding model bump → oude vectors gemarkeerd stale, reindex on-demand.
6. **Local dignity:** volledige stack draait op één binary + SQLite + disk tot je bewust cloud kiest.
7. **Engine swap:** SQLite→Postgres zonder API-breuk (`/v1/atlas/*` stabiel).

---

## 12. Aanbevolen beslissingen (nu vastleggen)

| Beslissing | Keuze |
|------------|--------|
| Logische taxonomy | Jouw 20 + stores 21–28 |
| Primary engine nu | SQLite (+ FTS5, embeddings in-DB) |
| Graph | Edge-tabel eerst |
| Working memory | SQLite/in-process tot multi-instance |
| Objects | `data/objects/` content-addressed |
| Secrets | Apart encrypted; nooit in traces |
| Verbeterlus | Trace → feedback → eval → dataset |
| Migratiepad | Store traits; Postgres later |

---

## 13. Implementatiestatus

### Milestone 0 — DONE
- Genormaliseerde `messages` + FTS5 (`messages_fts`, `memory_fts`)
- `memory_edges` + hop-2 neighbor expand
- `embedding_model_id` + `stale` mark on model change
- Append-only `traces` (chat + recall)
- `outbox` + request-driven drain
- APIs: `/v1/atlas/traces*`, `/v1/atlas/outbox`, `/v1/atlas/edges*`

### Milestone 1 — DONE
- Procedural memory schrijfbaar + MoE head + residual frames
- Conflict/truth: divergente claims → beide bewaren + `conflicts_with` edge
- Config: `memory_conflict_sim_low`

### Milestone 2 — DONE
- `documents` + `document_chunks` + `documents_fts`
- Content-addressed object storage (`data/objects/{hh}/{sha256}`)
- Ingest pipeline → semantic memory nodes + `has_chunk` edges
- APIs: `/v1/atlas/documents*` + search

### Volgende: Milestone 3 (Agent OS)
1. Tool registry + policy/capabilities
2. Task/agent state + checkpoints
3. Prompt/model registry versioning
4. Daarna M4 eval-loop, M5 scale adapters
