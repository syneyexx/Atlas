# Atlas Brain — Strategy Lab Plan (World-Class)

> Planmodus deliverable. Doel: onder **Trading** een **Strategy Lab** dat autonoom hypothesen formuleert, formaliseert, backtest, valideert en in een bibliotheek zet — op **één brain**, request-driven, met een **niet-omzeilbare poortwachter**.
>
> **UI-graphics:** bewust out-of-scope. De Trading-pagina maak je later; dit plan is contracts, stores, DSL, validatiepijplijn, agentrollen en bouwvolgorde. De pagina is een view op die contracts.

---

## 0. Context (source is leidend)

| Kapitaal vandaag | Hergebruik voor Strategy Lab |
|------------------|------------------------------|
| Research plane (tick/budget/orchestrator) | **Patroon** voor onderzoekslus zonder daemon |
| Datasets plane (recipes, jobs, Parquet/JSONL, edges) | Marktdata-acquisitie, recipe-achtige specs, result shards |
| Agent-OS: tools · tasks · checkpoints · policy | Hypothese-agent, coder, verifier; tool allow/deny |
| Memory Cortex + procedural | “Wanneer pas ik wat toe”, lessen uit mislukkingen |
| Evals · feedback · cost ledger · traces | Poortwachter-scores, attempt registry, kosten |
| Artifacts · objects · projects · secrets | Spec hashes, backtest reports, broker keys later |
| Engines (SQLite → dual Postgres · Redis · S3) | Metadata vs bulk resultaten |

**Gap:** geen Strategy DSL, geen backtester, geen attempt/holdout registry, geen gatekeeper die het LM niet mag herschrijven, geen live/paper trading plane.

**Productpositie:** top-tab **Trading** → Strategy Lab (onderzoek) + later Live Desk (executie). Zelfde brain als Chat/Data/Coding. Geen aparte trading-microservice.

---

## 1. Verdict op Claudes voorstel

**Sterk als productvisie.** De vier datafases (ontdekken → stresstest → forward/paper → klein live), de onderzoekslus, de poortwachter (OOS, Deflated Sharpe, purged CV, holdout, aparte verifier), en de bibliotheek-metadata (hypothese, bewijs, status, regimes, correlaties) zijn precies het verschil tussen “mooie backtests” en een serieus lab.

**Niet wereldklasse genoeg als je het 1:1 bouwt.** Acht fouten zouden Atlas (of je kapitaal) kapot maken:

| # | Claude-default / implicatie | Waarom fout voor Atlas | Expert-correctie |
|---|----------------------------|------------------------|------------------|
| 1 | Autonoom systeem “zoekt strategieën” met vrije code | Reward hacking: LM schrijft look-ahead of past validatie aan | **DSL-first**: uitvoerbare strategies alleen via gevalideerde YAML/JSON-spec + allowlisted primitives; poortwachter = **immutable code + sealed holdout**, niet een prompt |
| 2 | GAN’s / diffusie / agent-based markten vroeg | Simulator bevat jouw aannames; bewijst geen edge; dure stack | Fase 1–2: **block-bootstrap + regime replay**; generatieve sim alleen S7+ achter meting |
| 3 | RL-omgeving over de hele lus als default | Background trainers, non-determinisme, split brain | Beloning = OOS-score in **eval store**; fine-tune/RL pas S8 als export uit bewezen trajectories |
| 4 | Postgres + Parquet + graph als “bestaande opzet” letterlijk | Atlas is SQLite-first; graph = edge-tabel | Zelfde engines als Stores/Datasets: SQLite metadata → Parquet/JSONL results → Postgres dual |
| 5 | Poortwachter als “tweede LLM die breekt” alleen | LLMs zijn omkoopbaar; reward hacking | Verifier-agent **mag alleen rapporteren**; gates zijn deterministische checks in Rust; LM heeft **geen write** op `gate_definitions` |
| 6 | Live broker + risk in dezelfde iteratie als discovery | Slippage/kosten/kill-switch te laat = kapitaalrisico | Live/paper = **S6+**; risk engine hard-coded, niet LM-configureerbaar boven plafond |
| 7 | Papers (SSRN/arXiv) als idee-bron zonder ingest-plane | Geen pin, geen decontaminatie tegen eigen holdouts | Papers via Documents/Research → claim store; strategie citeert `source_ids` |
| 8 | Pagina + lab tegelijk | UI zonder contracts = theater | Contracts + fixture backtest **vóór** Trading UI |

**Richting:** Claudes taxonomy behouden, **Atlas invariants hard afdwingen**, ontbrekende stukken toevoegen (§3), engines gefaseerd, poortwachter als **constitution** (code), discovery als **capability** (agents).

---

## 2. Wat er al goed is (behouden uit Claude)

1. **Vier datafases** — historie → gesimuleerde stress → forward/paper → klein live.
2. **Echte data ≠ bewijs van causaliteit; simulatie ≠ bewijs van winst** — beide nodig, verschillende rollen.
3. **Onderzoekslus:** idee → formaliseer → implementeer → backtest → valideer → bibliotheek → reflectie.
4. **Economische reden verplicht** — hypothese zonder “waarom zou dit werken?” → reject vóór compute.
5. **Poortwachter-ideeën:** OOS/walk-forward, Deflated Sharpe + multiple testing, purged/embargoed CV, parameter-stabiliteit, regimes, capaciteit, look-ahead audit, verborgen holdout, aparte breaker.
6. **Bibliotheekvelden:** spec+code hash, hypothese, bewijs, status lifecycle, voorwaarden, live-vs-expectatie, familie/correlatie.
7. **Daytrade-toepassing later:** regime → allocator → signalen → risk (los) → executie → decay detection → dagelijkse review.
8. **Leren uit mislukkingen** via procedural memory.
9. **Eerlijke kanttekeningen** — daytrade-kosten, arbitrage, paper vóór echt geld.

---

## 3. Wat mist (toevoegen — Cursor-moat)

Zonder dit bouw je een LLM die backtests hallucineert.

| # | Toevoeging | Waarom |
|---|------------|--------|
| A | **Strategy identity plane** | `strategy_id` + `spec_hash` + `code_hash` + `dataset_pin`. Zonder pin = giftige lineage. |
| B | **Strategy Spec DSL (JSON Schema)** | Universum, signalen, entry/exit, sizing, kosten — gevalideerd vóór elke run. Geen vrije Python in S0–S4. |
| C | **Primitive registry** | Allowlisted indicatoren/executie-modellen (SMA, z-score, VWAP band, limit/market, …). Nieuwe primitive = code review + eval, niet chat. |
| D | **Attempt registry** | Elke geteste variant telt mee voor Deflated Sharpe / White’s Reality Check. Verzwijgen = fraud. |
| E | **Sealed holdout vault** | Cryptografisch gesealde periode/instrument-set; één unlock per strategie-familie; audit log. |
| F | **Gate constitution (immutable)** | Gates in code + versioned `gate_pack_semver`; agents mogen packs **niet** muteren (policy deny). |
| G | **Point-in-time / look-ahead auditor** | Automatische checks: geen future bars, geen restated fundamentals zonder as-of, geen same-bar leak. |
| H | **Cost & fill model as first-class** | Spread, fee, slippage, partial fills — pin per run; “zero-cost” runs mogen nooit promoveren. |
| I | **Promotion state machine** | `idea → specified → backtested → gated → paper → small_live → active → retired` met harde transitions. |
| J | **Portfolio correlation graph** | `strategy_edges` (family_of, correlates_with, conflicts_with, regime_fit). |
| K | **Offline fixture market** | Tiny OHLCV + known edges/non-edges voor CI; stub broker. |
| L | **Risk ceiling store** | Absolute caps (max DD, daily loss, notional) — buiten LM-bereik; kill switch API. |

---

## 4. Atlas invariants (hard — niet onderhandelbaar)

1. **One brain** — Strategy Lab is een plane in `atlas-brain`, geen aparte trading-service.
2. **Request-driven** — geen background optimizer/RL-trainer/market sweeper; wel `tick`, outbox drain on request, expliciete `promote` / `paper.tick`.
3. **Één schrijfdeur per domein** — specs via Strategy API; memory via Write Governor; risk events via Risk Engine; costs via ledger.
4. **Trace spine** — elke hypothese, backtest, gate-pass, paper fill → `trace_id`.
5. **Scope** — strategies default **project-scoped**; library cards mogen global-read binnen tenant.
6. **Local dignity** — SQLite + `data/strategy/` + fixture bars + stub broker → full discovery path offline.
7. **Engines follow scale** — taxonomy ≠ physical systems.
8. **Secrets** — broker keys in secrets store; nooit in specs/traces/UI.
9. **UI honesty** — frontend verzint geen PnL; alleen Brain events/metrics.
10. **Gate sovereignty** — model mag gate_pack, holdout, of attempt counts niet wijzigen of omzeilen (zelfde klasse als reward hacking).
11. **Money is late** — geen echte orders vóór S6 + expliciete human arming.
12. **Source leidend** — docs volgen code zodra S0+ landt; tot die tijd is dit plan de source of intent.

---

## 5. Datafases (Claude behouden, Atlas-operationeel)

| Fase | Data | Doel | Lab-status die het mag voeden | Bewijs-waarde |
|------|------|------|-------------------------------|---------------|
| **1 Ontdekken** | Historische echte bars (pinned) | Ideeën vinden/filteren | → `backtested` | Noodzakelijk, niet voldoende |
| **2 Stresstest** | Block-bootstrap / regime shuffle van echte reeksen | Fragiliteit afwijzen | → `gated` (deel) | Negatief bewijs sterk; positief bewijs zwak |
| **3 Forward / paper** | Live of delayed echte data, **nepgeld** | Bewijs op ongezien pad | → `paper` | Sterkste pre-money bewijs |
| **4 Klein live** | Echt geld, harde notional-cap | Kosten/slippage meten | → `small_live` → `active` | Enige absolute waarheid over executie |

**Regel:** een strategie die **alleen** fase 2 overleeft → automatisch `retired` of `rejected`. Promotie vereist fase 1 **én** 2; live vereist fase 3 stabiel onder decay-monitor.

---

## 6. Logische stores (uitbreiding)

Nieuwe logische stores — **zelfde 1–4 engines**.

| # | Store | Inhoud |
|---|--------|--------|
| 40 | **Market catalog** | Instrumenten, venues, timeframes, sessions, corporate-action calendars |
| 41 | **Market revisions** | Pin: symbol set + bar source + as-of + root hash (point-in-time) |
| 42 | **Market shards** | OHLCV/trades/quotes paths, checksums, gaps |
| 43 | **Strategy specs** | Versioned DSL docs + schema_version + status |
| 44 | **Strategy code artifacts** | Generated/compiled runner binding; content hash |
| 45 | **Strategy library** | Card: hypothese, economic rationale, family, tags, lifecycle status |
| 46 | **Research attempts** | Elke variant die ooit is getest (params, parent_id, outcome) — voor multiple testing |
| 47 | **Backtest runs** | Config pin, cost model, metrics summary, result shard refs |
| 48 | **Gate results** | Per gate_pack_semver: pass/fail, scores, evidence refs |
| 49 | **Holdout vault** | Sealed ranges; unlock events (one-shot per family) |
| 50 | **Simulation runs** | Bootstrap/regime stress configs + metrics |
| 51 | **Paper / live books** | Virtual or real positions, fills, slippage vs model |
| 52 | **Strategy edges** | family_of · correlates_with · conflicts_with · derived_from · regime_fit |
| 53 | **Risk ceiling & incidents** | Caps, breaches, kill-switch state |
| 54 | **Decay monitors** | Live vs backtest expectation bands; pause triggers |

**Mapping op bestaande stores:** objects (bar shards, reports), artifacts (spec snapshots, gate reports), tasks/checkpoints (lab job ticks), traces/costs, secrets (brokers), projects (scope), procedural memory (lessons), evals (gate golden cases), documents/research (papers → claims).

---

## 7. Engines — gefaseerd (Strategy-specifiek)

| Fase | Engines | Wanneer |
|------|---------|---------|
| **T-A Local** | SQLite metadata + JSONL/Parquet under `data/strategy/` + in-process backtester | Default |
| **T-B Local+** | DuckDB/Polars voor snelle walk-forward aggregaties; sqlite-vec voor paper/claim search | Meetbare query-pijn |
| **T-C Team** | Postgres dual-write metadata · S3 result shards · Redis optional job lease | Multi-instance |
| **T-D Sim scale** | Optionele sim workers **als adapter** (niet default daemon) | Alleen bij zware stress grids |
| **T-E Live** | Broker adapters achter trait; risk engine in-process; never LM in order path | S6+ |

**Default compute:** deterministische bar-replay engine in Rust (of Wasm-sandboxed formula IR). **Geen** unrestricted `eval(python)` in discovery path.

---

## 8. Domeinmodel (contracts)

### 8.1 Identity

```
MarketRef = {
  venue: "crypto" | "equity" | "fx" | "fixture",
  symbols: ["BTC-USD", ...],
  timeframe: "1m" | "5m" | "1h" | ...,
  revision: "<content-hash of PIT snapshot>",
}
StrategyKey = sha256(canonical_json(spec) + primitive_registry_semver)
AttemptKey  = sha256(StrategyKey + param_set + market_revision + cost_model_id)
RunKey      = sha256(AttemptKey + gate_pack_semver + engine_semver + seed?)
```

### 8.2 Strategy Spec DSL (v0 — concreet)

JSON/YAML, gevalideerd tegen JSON Schema `strategy_spec/1.0.0`.

```yaml
spec_version: "1.0.0"
id: "mean_reversion.vwap_band.v3"
title: "Intraday VWAP band mean reversion"
hypothesis: >
  Temporary liquidity shocks push price outside a volume-weighted fair value;
  reversion occurs within the session when inventory mean-reverts.
economic_rationale: >
  Market makers widen and then normalize; without a regime filter this dies in trends.
family: "mean_reversion.intraday"
universe:
  market_ref: { venue: fixture, symbols: [BTC-USD], timeframe: 1m, revision: "…" }
  session: "24x7"          # or exchange calendar id
  max_names: 1
signals:
  - id: z_vwap
    primitive: zscore_to_vwap
    params: { lookback_bars: 60 }
entry:
  long_when:  { all: [ { signal: z_vwap, op: "<=", value: -2.0 } ] }
  short_when: { all: [ { signal: z_vwap, op: ">=", value:  2.0 } ] }
  confirm_bars: 1
exit:
  take_profit_z: 0.0
  stop_z: 3.5
  max_hold_bars: 30
  flatten_on_session_end: true
sizing:
  model: fixed_notional      # fixed_notional | vol_target | kelly_capped
  notional: 1000
  max_leverage: 1.0
costs:
  model: "fixture_taker_v1"  # pin — required
  fee_bps: 2
  spread_bps: 1
  slippage_model: vol_impact_v0
constraints:
  allow_look_ahead: false    # must be false to run
  require_pit: true
meta:
  tags: [crypto, mean_reversion]
  regime_fit: [range, low_vol]
  sources: []                # research claim / document ids
```

**DSL-regels:**
- Alleen geregistreerde `primitive` namen.
- Geen inline code, geen imports, geen filesystem.
- `costs.model` verplicht; unknown model → reject.
- Spec wijziging → nieuwe `spec_hash` → nieuwe attempt (telt voor multiple testing).

### 8.3 Primitive registry (v0 seed)

| Primitive | Kind | Notes |
|-----------|------|-------|
| `sma` / `ema` / `zscore` / `zscore_to_vwap` | signal | Deterministic bar formulas |
| `rsi` / `realized_vol` / `atr` | signal | |
| `session_time_filter` / `vol_regime` | filter | |
| `cross_above` / `threshold` | entry helper | Compiles into entry AST |
| `fixed_notional` / `vol_target` | sizing | Caps enforced by risk ceiling |
| `fixture_taker_v1` / `maker_rebate_v0` | cost | |
| `market` / `limit_touch` | execution (paper/live) | Live only via broker trait |

Nieuwe primitive = PR + golden eval bars + gate that forbids silent look-ahead.

### 8.4 Lab job ticks (Research/Datasets-patroon)

```
POST /v1/atlas/strategy/jobs/{id}/tick  { max_steps?: 1..32 }
```

State machine:

```
draft → hypothesize → specify → implement → backtest
  → validate → (await_human?) → library_write → reflect
  → completed | failed | rejected
```

Elke tick: 1..N step-units binnen budget; events append-only; stop op gate fail, pause, budget, of completion. **Geen** background loop.

### 8.5 Promotion machine (apart van lab job)

```
idea → specified → backtested → gated → paper → small_live → active → retired
         ↘ rejected at any hard gate
```

Transitions vereisen bewijs-artifact IDs. `active` vereist armed risk ceiling + human ack.

---

## 9. De poortwachter (constitution)

### 9.1 Principe

De poortwachter is **geen chatpersona**. Het is:

1. **Deterministische gate pack** in Rust (`gate_pack_semver`)
2. **Attempt registry** (altijd schrijven vóór score publicatie)
3. **Sealed holdout** (crypto seal; one unlock)
4. **Verifier agent** (optioneel, read-only): probeert de strategie te breken en schrijft bevindingen → mag **alleen** fail-evidence toevoegen, nooit gates verlagen

Policy: tools `strategy.gates.update`, `strategy.holdout.unseal` (behalve controlled API), `strategy.attempts.delete` → **deny** voor LM roles.

### 9.2 Gate pack v0 (concrete checks)

| Gate ID | Check | Fail behavior |
|---------|-------|---------------|
| `G_econ` | Non-empty hypothesis + economic_rationale (≥ N chars, not duplicate of title) | reject pre-compute |
| `G_cost` | Cost model pin present; zero-cost flag forbidden for promotion | reject |
| `G_pit` | Look-ahead audit clean on AST + bar alignment | reject |
| `G_oos` | Walk-forward OOS sharpe/exp shortfall above floor **after costs** | reject |
| `G_dsr` | Deflated Sharpe vs attempt_count family-wide | reject |
| `G_purged` | Purged/embargoed CV mean ≥ floor; worst fold not catastrophic | reject |
| `G_stable` | Local param perturbation (±ε) keeps sign of edge / limited metric drop | reject |
| `G_regime` | Not single-regime only (unless declared niche + capacity ok) | warn or reject by mode |
| `G_capacity` | Adv. volume / participation ≤ cap | reject for live track |
| `G_stress` | Block-bootstrap / regime stress: median OOS not negative; tail DD bounded | reject |
| `G_holdout` | Sealed holdout unlock once → pass band | required for `gated→paper` |
| `G_verifier` | Breaker agent findings severity < threshold | reject if critical |

**Deflated Sharpe (Bailey & López de Prado):** gebruik family attempt count, niet “alleen de winnaars”. Attempt rows zijn append-only.

### 9.3 Holdout vault

```
HoldoutSeal {
  id, market_ref_subset, start, end,
  seal_hash,           # HMAC/content hash
  status: sealed|unlocked|spent,
  spent_by_strategy_family?,
  unlocked_at?, unlock_trace_id?
}
```

Unlock API is human-gated (`await_approval` + project owner). Tweede unlock voor dezelfde family → hard deny.

### 9.4 Look-ahead audit

- Spec AST: verbied features die future windows nodig hebben zonder shift.
- Engine: signals computed with `closed_bar_only`; entries on next bar open (configurable but default strict).
- Dataset: PIT revision; restated fields must carry `as_of`.
- Static tests on fixture where cheating strategies must fail golden gates.

---

## 10. Onderzoekslus (pipeline)

```
[Idea agent] leest papers/journal/failures (Memory + Documents + Research claims)
    → hypothese + economic rationale
[Formalizer] → Strategy Spec draft (DSL) + RecipeVerifier-achtige SpecVerifier
[Implementer] → bind spec → runner artifact (hash)  [geen vrije code in S0–S4]
[Backtester] → historical run + costs (fase 1)
[Gate pack] → OOS / DSR / purged / stability / PIT
[Stress] → bootstrap/regime (fase 2)
[Human gate?] → optional for first promotions / always for holdout unlock
[Library] → versioned card + evidence bundle
[Reflector] → procedural memory lesson (success or failure)
```

Mislukkingen zijn first-class: `status=rejected` + lesson node blijft doorzoekbaar.

---

## 11. Agentrollen (mapped op Agent-OS)

| Rol | Verantwoordelijkheid | Write rights |
|-----|----------------------|--------------|
| **Lab planner** | Job plan, budgets, phase order | lab job |
| **Idea agent** | Hypotheses from papers/memory | draft specs only |
| **Formalizer** | DSL emit | specs draft |
| **Spec verifier** | Schema + econ + cost pin | pass/fail only |
| **Implementer** | Spec → runner binding | code artifacts |
| **Backtest runner** | Deterministic engine | runs + shards |
| **Gate runner** | Gate pack execution | gate_results (append) |
| **Breaker / verifier agent** | Adversarial critique | evidence notes only |
| **Reflector** | Lessons → procedural memory | memory via governor |
| **Risk engine** | Caps, kill switch | risk store only — **no LM** |
| **Human** | Holdout unlock, arm live, budget overrides | approvals |

Subagents = child tasks (`parent_id`), zelfde patroon als Research.

---

## 12. API-oppervlak (contract-first; UI later)

```
# Market data
GET  /v1/atlas/strategy/markets/catalog/search
POST /v1/atlas/strategy/markets/revisions          # pin PIT snapshot
POST /v1/atlas/strategy/markets/{id}/acquire
GET  /v1/atlas/strategy/markets/{id}

# Specs & library
POST /v1/atlas/strategy/specs/propose
POST /v1/atlas/strategy/specs/verify
GET/PUT /v1/atlas/strategy/specs/{id}
GET  /v1/atlas/strategy/library
GET  /v1/atlas/strategy/library/{id}
GET  /v1/atlas/strategy/library/{id}/evidence

# Lab jobs (discovery loop)
POST /v1/atlas/strategy/jobs
GET  /v1/atlas/strategy/jobs/{id}
POST /v1/atlas/strategy/jobs/{id}/tick
POST /v1/atlas/strategy/jobs/{id}/pause
POST /v1/atlas/strategy/jobs/{id}/approve

# Attempts, runs, gates
GET  /v1/atlas/strategy/attempts?family=
POST /v1/atlas/strategy/runs/backtest
POST /v1/atlas/strategy/runs/stress
POST /v1/atlas/strategy/gates/run
GET  /v1/atlas/strategy/gates/packs

# Holdout
GET  /v1/atlas/strategy/holdouts
POST /v1/atlas/strategy/holdouts/{id}/request_unlock   # → await_approval
POST /v1/atlas/strategy/holdouts/{id}/unlock           # human

# Promotion & paper/live (later milestones)
POST /v1/atlas/strategy/library/{id}/promote
POST /v1/atlas/strategy/paper/sessions
POST /v1/atlas/strategy/paper/{id}/tick
POST /v1/atlas/strategy/risk/arm
POST /v1/atlas/strategy/risk/kill

# Graph
GET  /v1/atlas/strategy/edges…
GET  /v1/atlas/strategy/library/{id}/neighbors

# MCP mirrors of the above tools
```

Mutaties: traces + outbox (`strategy.job.created`, `strategy.gate.failed`, `strategy.promoted`, `strategy.risk.killed`, …).

---

## 13. Database-tabellen (SQLite-first; Postgres DDL spiegel)

### 13.1 Core

```sql
-- Market plane
strategy_market_catalog (
  id TEXT PK, venue TEXT, symbol TEXT, title TEXT, tags_json TEXT,
  meta_json TEXT, created_at TEXT
);
strategy_market_revisions (
  id TEXT PK, catalog_keys_json TEXT, timeframe TEXT,
  revision_hash TEXT NOT NULL, pit_as_of TEXT, root_manifest_json TEXT,
  created_at TEXT
);
strategy_market_shards (
  id TEXT PK, revision_id TEXT, path TEXT, bytes INTEGER,
  checksum TEXT, bar_count INTEGER, gap_json TEXT
);

-- Specs & library
strategy_specs (
  id TEXT PK, strategy_key TEXT NOT NULL, family TEXT,
  title TEXT, spec_json TEXT NOT NULL, spec_hash TEXT NOT NULL,
  schema_version TEXT, status TEXT, -- draft|verified|deprecated
  hypothesis TEXT, economic_rationale TEXT,
  source_ids_json TEXT, project_id TEXT,
  created_at TEXT, updated_at TEXT
);
strategy_library (
  id TEXT PK, spec_id TEXT NOT NULL, code_hash TEXT,
  status TEXT NOT NULL, -- idea|specified|backtested|gated|paper|small_live|active|retired|rejected
  regime_fit_json TEXT, conditions_json TEXT,
  parent_id TEXT, -- family lineage
  created_at TEXT, updated_at TEXT
);

-- Attempts (multiple testing spine)
strategy_attempts (
  id TEXT PK, attempt_key TEXT UNIQUE,
  family TEXT NOT NULL, spec_hash TEXT NOT NULL,
  params_json TEXT, parent_attempt_id TEXT,
  market_revision TEXT, cost_model_id TEXT,
  outcome TEXT, -- pending|pass|fail|error
  metrics_json TEXT, created_at TEXT
);

-- Runs & results
strategy_runs (
  id TEXT PK, run_key TEXT UNIQUE, attempt_id TEXT NOT NULL,
  kind TEXT, -- backtest|stress|paper|live
  engine_semver TEXT, gate_pack_semver TEXT,
  config_json TEXT, seed INTEGER,
  status TEXT, metrics_json TEXT,
  result_shard_path TEXT, trace_id TEXT,
  created_at TEXT, completed_at TEXT
);

strategy_gate_results (
  id TEXT PK, run_id TEXT, pack_semver TEXT,
  gate_id TEXT, passed INTEGER, score REAL,
  detail_json TEXT, created_at TEXT
);

strategy_holdouts (
  id TEXT PK, seal_hash TEXT NOT NULL,
  market_subset_json TEXT, start TEXT, end TEXT,
  status TEXT, -- sealed|unlocked|spent
  spent_family TEXT, unlock_trace_id TEXT,
  created_at TEXT, unlocked_at TEXT
);

strategy_edges (
  id TEXT PK, src TEXT, dst TEXT, rel TEXT,
  score REAL, provenance_json TEXT, created_at TEXT
);

strategy_jobs (
  id TEXT PK, title TEXT, status TEXT, phase TEXT,
  plan_json TEXT, budget_json TEXT, paused INTEGER,
  library_id TEXT, conversation_id TEXT, task_id TEXT,
  created_at TEXT, updated_at TEXT
);
strategy_events (
  id TEXT PK, job_id TEXT, seq INTEGER,
  timestamp TEXT, event_type TEXT, lines_json TEXT, details TEXT
);

strategy_risk_ceilings (
  id TEXT PK, project_id TEXT,
  max_daily_loss REAL, max_drawdown REAL, max_notional REAL,
  armed INTEGER, killed INTEGER, updated_at TEXT
);
strategy_risk_incidents (
  id TEXT PK, ceiling_id TEXT, kind TEXT, detail_json TEXT, created_at TEXT
);

strategy_decay_monitors (
  id TEXT PK, library_id TEXT,
  expect_json TEXT, band_json TEXT,
  status TEXT, -- ok|warn|paused
  last_check_at TEXT
);
```

Bulk equity curves / trade lists: Parquet of JSONL under `data/strategy/runs/{run_id}/` (object hash in artifacts).

### 13.2 Indexes (minimaal)

- `strategy_attempts(family, created_at)`
- `strategy_library(status, updated_at)`
- `strategy_runs(attempt_id, kind)`
- `strategy_gate_results(run_id, gate_id)`
- `strategy_edges(src, rel)` / `(dst, rel)`
- FTS op `strategy_specs(hypothesis, economic_rationale, title)` voor lab search

---

## 14. Daytrading-toepassing (later — niet S0)

Pas ná library `gated` + paper stability:

| Component | Rol | Constraint |
|-----------|-----|------------|
| Regime detector | Kiest passende library subset | Read-only op live features |
| Portfolio allocator | Lage correlatie via `strategy_edges` | Risk ceiling hard cap |
| Signal generator | Streaming bars → DSL runner | Same engine as backtest |
| Risk engine | Size, daily loss, max DD, kill | **Geen LM in path** |
| Broker adapter | Orders + fill/slippage log | Secrets; stub first |
| Decay monitor | Live vs expectatie → auto-pause | Writes monitor status |
| Daily review | Journal → new hypotheses | Feeds idea agent |

---

## 15. Hoe het “leert” (zonder reward hacking)

1. **Procedural memory:** reflector schrijft lessen (“VWAP MR sterft in high-trend regime zonder filter”).
2. **Attempt registry:** negatieve resultaten blijven tellen — preferential attachment naar “lucky” families wordt bestraft via DSR.
3. **Eval export (S8):** trajectories met OOS reward → `training_examples` / Datasets mix; **nooit** trainen op holdout.
4. **Geen online gewicht-update** van gate thresholds door het model.

---

## 16. Pagina (spec only — graphics later)

Wanneer Trading UI landt, zones met **één job per sectie**:

1. **Lab console** — job events, phase, budgets, tick/pause (Research-achtig).
2. **Library** — status filters; card = hypothese + rationale + evidence chips (geen fake PnL).
3. **Evidence drawer** — runs, gates, stress, holdout state.
4. **Live desk (later)** — paper/live, risk armed state, decay — bewust níet in eerste viewport van Lab.

UI leest uitsluitend API; mag sealed holdout ranges niet tonen vóór unlock.

---

## 17. Roadmap (technisch, geen kalender)

Afhankelijkheden dwingen volgorde. **Niets weggelaten** — wel pas materialiseren wanneer dependency groen is.

### S0 — Identity + stores + DSL schema
- Tabellen §13; JSON Schema `strategy_spec/1.0.0`
- Primitive registry seed (signals/sizing/costs stubs)
- Stub market fixtures + catalog
- APIs: CRUD skeleton, spec verify (schema-only)
- **Done when:** offline pin + list + invalid spec rejected

### S1 — Deterministic backtester v0
- Bar replay; next-bar execution; cost model pin
- Equity/trade JSONL shards; metrics (Sharpe, DD, turnover)
- Look-ahead golden tests (cheater fails)
- **Done when:** fixture mean-reversion known-edge passeert; cheater look-ahead faalt `G_pit`

### S2 — Lab orchestrator (Research-patroon)
- Jobs/events/ticks; idea→specify→backtest path met stub LM
- Attempt registry write-on-run
- SpecVerifier (econ + cost + schema)
- **Done when:** één tick-budgette job eindigt in `backtested` of `rejected` met traces

### S3 — Gate pack v0 (zonder holdout unlock UX polish)
- OOS walk-forward, purged CV, stability, DSR using attempts, cost gate
- Gate results append-only; LM cannot update pack
- **Done when:** overfit fixture faalt DSR/stability; honest fixture kan passen

### S4 — Stress fase 2
- Block-bootstrap + regime shuffle runners
- `G_stress` wired; sim results in shards
- **Done when:** fragile strategy fails stress; robust fixture survives

### S5 — Holdout vault + library promotion
- Seal/unlock human gate; one-shot per family
- Library statuses through `gated`
- Strategy edges + evidence bundle API
- **Done when:** second unlock denied; promotion requires full gate pack + holdout

### S6 — Paper trading (fase 3)
- Stub broker + streaming fixture/live delayed feed
- Paper book, slippage monitor, decay monitor v0
- **Done when:** paper session ticks request-driven; decay can pause

### S7 — Risk ceilings + small live adapter (fase 4)
- Hard caps; kill switch; broker trait + one paper-equal adapter
- Human arm required; notional micro-cap
- **Done when:** breach → kill; LM tools cannot raise ceilings

### S8 — Learning export
- Rejected/successful trajectories → procedural + optional Datasets mix
- Reflector quality evals
- **Done when:** lesson recall improves idea-agent reject rate on known-bad patterns (eval set)

### S9 — UI page
- Consume S0–S8 contracts; graphics later upload
- **Done when:** Lab → evidence → promote → paper zonder UI-leugens

### S10 — Scale / generative sim (optioneel)
- GAN/diffusion/ABM **alleen** als stress-adapter achter flag; never as sole promotion evidence
- Postgres/S3 dual already via engines
- **Done when:** adapters behind traits; local path remains default

---

## 18. Non-goals (bewust)

- Geen aparte trading-microservice, optimizer-daemon, of RL trainer in S0–S7.
- Geen unrestricted strategy code execution in discovery.
- Geen Neo4j/Ray/Kafka defaults.
- Geen echte orders vóór S7 + human arm.
- Geen pixel-UI in deze planfase.
- Geen “Sharpe zonder kosten” als promotie-metric.
- Geen ethiek-theater; wél risk ceilings + audit als technische gates.
- Geen silent market redownload — altijd revision pin.

---

## 19. Succescriteria (wereldklasse)

1. **Pin integrity:** zelfde market revision → identieke root hash.
2. **No look-ahead:** cheater fixture faalt altijd `G_pit`.
3. **Attempt honesty:** elke run schrijft attempt; DSR gebruikt family count.
4. **Gate sovereignty:** LM role cannot mutate gate_pack or delete attempts (policy test).
5. **Holdout one-shot:** tweede unlock zelfde family → deny.
6. **Cost realism:** zero-cost runs cannot reach `gated`.
7. **Stress humility:** sim-only winners cannot promote.
8. **Local dignity:** fixtures → S0–S5 path zonder netwerk/broker.
9. **Replay:** `run_id` → spec_hash + market_revision + engine_semver + metrics reproduceerbaar (seeded stress).
10. **UI honesty (S9):** elk lab-event uit Brain; geen client-side fake equity.

---

## 20. Beslissingen (nu vastleggen)

| Beslissing | Keuze |
|------------|--------|
| Orchestratie | Atlas jobs + ticks (Research/Datasets-patroon) |
| Strategy representation | DSL YAML/JSON + JSON Schema; primitives allowlist |
| Execution in discovery | Deterministic in-process engine; no free Python |
| Bulk results | JSONL/Parquet under `data/strategy/` |
| Metadata engine | SQLite → Postgres dual (bestaande engines) |
| Graph | `strategy_edges` SQL eerst |
| Poortwachter | Immutable Rust gate pack + sealed holdout; breaker LM read-only |
| Multiple testing | Append-only `strategy_attempts` + Deflated Sharpe |
| Simulatie | Block-bootstrap/regime eerst; generative later optional |
| Money | Paper S6 → small live S7; risk ceilings non-negotiable |
| UI | Contract-first; Trading page later |
| Claudes scope | Capability map behouden; runtime Atlas-first |

---

## 21. Kritiek op Claude — samenvatting in één alinea

Claude tekende het juiste **lab** (fases, lus, poortwachter-ideeën, bibliotheek) maar liet de **constitution** te zacht: een LLM die zelf zoekt zal zonder DSL-allowlist, attempt registry, sealed holdout en immutable gates vooral overfit-theater produceren — en “verifier agent” alleen is omkoopbaar. Dit plan houdt zijn onderzoekslus en datafases intact, dwingt Research-achtige ticks af, hergebruikt Store/Datasets-kapitaal, en maakt de poortwachter tot code + crypto-seal die het model niet mag herschrijven — zodat Strategy Lab autonoom mag **zoeken**, maar niet mag **liegen**.

---

## 22. Implementatiestatus

| MS | Inhoud | Status |
|----|--------|--------|
| S0 | Identity + stores + DSL schema + fixtures | PLAN |
| S1 | Deterministic backtester v0 + PIT golden | PLAN |
| S2 | Lab orchestrator + attempts | PLAN |
| S3 | Gate pack v0 | PLAN |
| S4 | Stress (bootstrap/regime) | PLAN |
| S5 | Holdout + library promotion | PLAN |
| S6 | Paper trading + decay | PLAN |
| S7 | Risk ceilings + small live adapter | PLAN |
| S8 | Learning export / reflector | PLAN |
| S9 | Trading UI (Strategy Lab) | PLAN |
| S10 | Scale / generative sim adapters | PLAN (optioneel) |

> Na elke implementatie-werkbeurt: deze tabel + `docs/datanames.md` + `docs/filenames.md` bijwerken. **Source (code) is leidend.**
