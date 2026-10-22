# Atlas engines (M5)

Logische stores blijven ongewijzigd. Physical engines zijn configureerbaar en **shipped**.

## Backends

| Domein | Default | Cloud / multi-instance |
|--------|---------|------------------------|
| Primary DB | SQLite (`atlas.db`) | Postgres dual-write (`ATLAS_DATABASE_URL`, `db_backend=dual\|postgres`) |
| Working + semantic cache | SQLite | Redis (`ATLAS_REDIS_URL`) — hot path + SQLite durable copy |
| Objects | `data/objects/` | S3/MinIO (`ATLAS_OBJECT_BACKEND=s3`) + local mirror |
| Graph | `memory_edges` SQL | Zelfde schema in Postgres (AGE alleen bij meetbare edge-scale) |

## Start full stack

```bash
docker compose -f docker-compose.yml up -d
```

Brain met dual engines:

```bash
export ATLAS_DB_BACKEND=dual
export ATLAS_DATABASE_URL=postgres://atlas:atlas@127.0.0.1:5432/atlas
export ATLAS_REDIS_URL=redis://127.0.0.1:6379
export ATLAS_OBJECT_BACKEND=s3
export ATLAS_S3_ENDPOINT=http://127.0.0.1:9000
export ATLAS_S3_BUCKET=atlas-objects
export ATLAS_S3_ACCESS_KEY=atlasminio
export ATLAS_S3_SECRET_KEY=atlasminio123
# ATLAS_SECRETS_KEY optioneel — brain genereert + persist een key bij eerste start
cd backend/atlas-brain && cargo run --release
```

Status: `GET /v1/atlas/engines`

## dual vs postgres vs sqlite

| Mode | Gedrag |
|------|--------|
| `sqlite` | Local one-brain (default). Alle reads/writes via SQLite + filesystem objects. |
| `dual` | SQLite blijft de actieve query-engine (FTS5/workbench). Elke kritische write dual-writet naar Postgres (memory, edges, traces, costs, documents, working, projects, artifacts, secrets, outbox). Redis/S3 hot paths actief wanneer geconfigureerd. |
| `postgres` | Postgres schema + connect verplicht; dual-write mirror zoals `dual`. SQLite blijft lokale FTS/workbench voor de request path. |

## Wat dual-write dekt

`memory_nodes` · `memory_edges` · `working_memory` · `traces` · `outbox` · `documents` / `document_chunks` · `cost_ledger` · `projects` · `artifacts` · `secrets`

Redis: working KV + semantic cache (TTL = `semantic_cache_ttl_secs`).  
S3: content-addressed objects met filesystem mirror voor read-after-write.

## Datasets plane (D0–D7 shipped)

Zie `docs/plan-datasets-architecture.md`. **Geen nieuwe default-engines in D0–D7.**

| Domein | Default (nu) | Later (D-C/D-D / D9) |
|--------|--------------|----------------------|
| Dataset metadata / jobs / recipes | SQLite (zelfde brain DB); Postgres DDL aanwezig voor datasets/jobs/recipes/edges | Dual-write mirrors (nog niet) |
| Bulk records / shards | JSONL onder `data/datasets/` (+ object hashes) | Parquet crate · S3/MinIO |
| Record search | FTS5 `dataset_records_fts` | Dedicated vector ANN |
| Graph | `dataset_edges` SQL | AGE/Neo4j alleen D9 |
| Compute | In-process streaming via job ticks | Ray adapter alleen D9 |

Config: `datasets_hub_provider` (`stub`\|`http`) · `datasets_max_ticks` · `datasets_disk_budget_bytes` · `datasets_max_cost_usd`.

Orchestratie: request-driven (`POST …/datasets/jobs/{id}/tick`) — geen Dagster/Prefect/worker-daemon.
