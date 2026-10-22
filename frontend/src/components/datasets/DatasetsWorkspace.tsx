'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import {
  ArrowDownToLine,
  ArrowUpRight,
  Check,
  Code,
  Copy,
  Database,
  FileText,
  Image as ImageIcon,
  Info,
  LayoutGrid,
  List,
  Maximize2,
  Minimize2,
  MoreVertical,
  Pause,
  Play,
  Search,
  Terminal,
  X,
} from 'lucide-react';
import {
  approveRecipe,
  count,
  createDatasetJob,
  datasetPreview,
  fetchDatasetJob,
  formatBytes,
  getCatalogCard,
  listDatasets,
  listRecords,
  logEvent,
  mapBrainEvents,
  mapCatalogToPreview,
  parseRepository,
  pauseDatasetJob,
  pinAndAcquire,
  searchCatalog,
  tickDatasetJob,
  type Dataset,
  type DatasetFilter,
  type ImportEvent,
} from '../../lib/datasets';

export type { DatasetFilter };

const statuses = {
  ready: 'Gereed',
  downloading: 'Downloaden',
  paused: 'Gepauzeerd',
  cancelled: 'Geannuleerd',
  verifying: 'Controleren',
  registering: 'Registreren',
};

export function DatasetsWorkspace({
  filter,
  onFilterChange,
  importRequest,
  onDownloadCount,
  baseUrl,
  connected,
}: {
  filter: DatasetFilter;
  onFilterChange: (filter: DatasetFilter) => void;
  importRequest: number;
  onDownloadCount: (count: number) => void;
  baseUrl: string;
  connected: boolean;
}) {
  const [datasets, setDatasets] = useState<Dataset[]>([]);
  const [events, setEvents] = useState<ImportEvent[]>([]);
  const [activeId, setActiveId] = useState('');
  const [repo, setRepo] = useState('');
  const [config, setConfig] = useState('default');
  const [split, setSplit] = useState('train');
  const [format, setFormat] = useState('JSONL');
  const [query, setQuery] = useState('');
  const [kind, setKind] = useState('Alle typen');
  const [grid, setGrid] = useState(false);
  const [error, setError] = useState('');
  const [metadata, setMetadata] = useState<Dataset | null>(null);
  const [detail, setDetail] = useState<Dataset | null>(null);
  const [previewRows, setPreviewRows] = useState<Record<string, unknown>[]>([]);
  const [expanded, setExpanded] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const [copied, setCopied] = useState(false);
  const [menu, setMenu] = useState('');
  const [busy, setBusy] = useState(false);
  const input = useRef<HTMLInputElement>(null);
  const log = useRef<HTMLDivElement>(null);
  const consoleDialog = useRef<HTMLDialogElement>(null);
  const metadataDialog = useRef<HTMLDialogElement>(null);
  const detailDialog = useRef<HTMLDialogElement>(null);
  const lastRequest = useRef(importRequest);
  const pollRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const active = datasets.find((d) => d.id === activeId);
  const running = datasets.filter((d) =>
    ['downloading', 'paused', 'verifying', 'registering'].includes(d.status),
  );

  const refreshLibrary = useCallback(async () => {
    if (!connected) return;
    try {
      const list = await listDatasets(baseUrl);
      setDatasets(list);
      if (!activeId && list.length) setActiveId(list[0].id);
      if (activeId && !list.some((d) => d.id === activeId) && list.length) {
        setActiveId(list[0].id);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Datasets laden mislukt.');
    }
  }, [baseUrl, connected, activeId]);

  useEffect(() => {
    void refreshLibrary();
  }, [refreshLibrary]);

  useEffect(() => {
    onDownloadCount(running.length);
  }, [running.length, onDownloadCount]);

  useEffect(() => {
    if (importRequest !== lastRequest.current) {
      lastRequest.current = importRequest;
      input.current?.focus();
    }
  }, [importRequest]);

  useEffect(() => {
    if (expanded) consoleDialog.current?.showModal();
    else consoleDialog.current?.close();
  }, [expanded]);

  useEffect(() => {
    if (metadata) metadataDialog.current?.showModal();
    else metadataDialog.current?.close();
  }, [metadata]);

  useEffect(() => {
    if (detail) detailDialog.current?.showModal();
    else detailDialog.current?.close();
  }, [detail]);

  useEffect(() => {
    if (autoScroll && log.current) log.current.scrollTop = log.current.scrollHeight;
  }, [events, active?.progress, autoScroll, expanded]);

  // Poll active job — only Brain events, no invented progress.
  useEffect(() => {
    if (pollRef.current) clearTimeout(pollRef.current);
    if (!connected || !active?.jobId) return;
    if (!['downloading', 'registering', 'verifying'].includes(active.status)) return;
    if (active.status === 'paused') return;

    pollRef.current = setTimeout(() => {
      void (async () => {
        try {
          const payload = await tickDatasetJob(baseUrl, active.jobId!, 4);
          setEvents(mapBrainEvents(payload.events));
          await refreshLibrary();
        } catch (e) {
          setError(e instanceof Error ? e.message : 'Job-tick mislukt.');
        }
      })();
    }, 1200);
    return () => {
      if (pollRef.current) clearTimeout(pollRef.current);
    };
  }, [active, baseUrl, connected, refreshLibrary]);

  const shown = datasets.filter(
    (d) =>
      (filter !== 'Downloads' || d.status !== 'ready') &&
      (!['Research', 'Code & instructies', 'Eigen datasets'].includes(filter) ||
        d.collection === filter) &&
      `${d.name} ${d.repo}`.toLowerCase().includes(query.toLowerCase()) &&
      (kind === 'Alle typen' || d.kind === kind),
  );

  async function inspect() {
    setError('');
    if (!connected) {
      setError('Verbind eerst Atlas Brain (Instellingen).');
      return;
    }
    try {
      const id = parseRepository(repo);
      let card;
      try {
        card = await getCatalogCard(baseUrl, 'hf', id);
      } catch {
        const results = await searchCatalog(baseUrl, id, 5);
        card = results.find((r) => r.id === id) || results[0];
      }
      if (!card) throw new Error(`Geen cataloguskaart voor ${id}. Probeer atlas/tiny-instruct.`);
      setMetadata(mapCatalogToPreview(card, config, split, format));
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Ongeldige repository');
    }
  }

  async function start() {
    if (!metadata || !connected) return;
    if (running.length) {
      setError('Pauzeer of rond eerst de huidige import af.');
      setMetadata(null);
      return;
    }
    setBusy(true);
    setError('');
    try {
      setEvents([
        logEvent('INFO', ['Import gestart via Atlas Brain', `repo: ${metadata.repo}`]),
        logEvent('HTTP', [`POST /v1/atlas/datasets · acquire`, `hub: ${metadata.repo}`]),
      ]);
      const acquired = await pinAndAcquire(baseUrl, metadata.repo, metadata.revision);
      const shards = acquired.shards || [];
      setEvents((es) => [
        ...es,
        logEvent('PLAN', [
          `${shards.length} shard(s) · revision ${acquired.dataset.revision}`,
          `root_hash: ${acquired.dataset.root_hash.slice(0, 16)}…`,
        ]),
        logEvent('DISK', [
          `./data/datasets/${acquired.dataset.id}/`,
          ...shards.map((s) => `${s.name} · ${formatBytes(s.size_bytes)} · ${s.checksum.slice(0, 12)}…`),
        ]),
        logEvent('OK', ['Acquire voltooid · shards content-addressed']),
      ]);

      const sample = await createDatasetJob(baseUrl, {
        dataset_id: acquired.dataset.id,
        kind: 'sample',
        auto_run: true,
      });
      setEvents(mapBrainEvents(sample.events));
      setActiveId(acquired.dataset.id);
      setMetadata(null);
      setRepo('');
      onFilterChange('Alle datasets');
      await refreshLibrary();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Import mislukt.');
      setEvents((es) => [
        ...es,
        logEvent('CANCEL', [e instanceof Error ? e.message : 'Import mislukt']),
      ]);
    } finally {
      setBusy(false);
    }
  }

  async function pause() {
    if (!active?.jobId || !connected) return;
    const resume = active.status === 'paused';
    try {
      const payload = await pauseDatasetJob(baseUrl, active.jobId, !resume);
      setEvents(mapBrainEvents(payload.events));
      await refreshLibrary();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Pauze mislukt.');
    }
  }

  async function approveActive(approve: boolean) {
    if (!active?.recipeId || !connected) return;
    setBusy(true);
    try {
      await approveRecipe(baseUrl, active.recipeId, approve);
      setEvents((es) => [
        ...es,
        logEvent(approve ? 'APPROVE' : 'REJECT', [
          approve ? 'Human gate: recept goedgekeurd' : 'Human gate: recept afgewezen',
        ]),
      ]);
      if (approve) {
        const full = await createDatasetJob(baseUrl, {
          dataset_id: active.id,
          kind: 'full',
          recipe_id: active.recipeId,
          auto_run: true,
        });
        setEvents(mapBrainEvents(full.events));
      }
      await refreshLibrary();
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Approve/full mislukt.');
    } finally {
      setBusy(false);
    }
  }

  async function openDetail(d: Dataset) {
    setDetail(d);
    setPreviewRows([]);
    if (connected && d.status === 'ready') {
      try {
        const rows = await listRecords(baseUrl, d.id, 5);
        setPreviewRows(rows);
      } catch {
        setPreviewRows([]);
      }
    }
  }

  async function copyLog() {
    try {
      await navigator.clipboard.writeText(
        'ATLAS 999 · DATASETS LOG\n' +
          events.map((e) => `${e.time} ${e.type} ${e.lines.join('\n          ')}`).join('\n'),
      );
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    } catch {
      setError('Kopiëren is hier niet beschikbaar. Gebruik Log downloaden.');
    }
  }

  function downloadLog() {
    const a = document.createElement('a');
    const url = URL.createObjectURL(
      new Blob(
        [
          'ATLAS 999 · DATASETS LOG\n' +
            events.map((e) => `${e.time} ${e.type} ${e.lines.join('\n          ')}`).join('\n'),
        ],
        { type: 'text/plain;charset=utf-8' },
      ),
    );
    a.href = url;
    a.download = 'atlas-dataset-log.txt';
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  }

  function datasetIcon(d: Dataset) {
    return d.kind === 'Code' ? <Code /> : d.kind === 'Afbeelding' ? <ImageIcon /> : <FileText />;
  }

  function terminal() {
    const awaitingGate = active?.status === 'verifying' && !!active.recipeId;
    return (
      <>
        <header className="ds-console-header">
          <Terminal />
          <h2>Importconsole</h2>
          <b>
            {active
              ? {
                  downloading: 'BEZIG',
                  ready: 'KLAAR',
                  paused: 'PAUZE',
                  cancelled: 'STOP',
                  verifying: 'GATE',
                  registering: 'SAMPLE',
                }[active.status]
              : 'LOG'}
          </b>
          <button title="Log kopiëren" aria-label="Importlog kopiëren" onClick={() => void copyLog()}>
            {copied ? <Check /> : <Copy />}
          </button>
          <button title="Log downloaden" aria-label="Importlog downloaden" onClick={downloadLog}>
            <ArrowDownToLine />
          </button>
          <button
            aria-label={expanded ? 'Console verkleinen' : 'Console vergroten'}
            onClick={() => setExpanded(!expanded)}
          >
            {expanded ? <Minimize2 /> : <Maximize2 />}
          </button>
        </header>
        <div className="ds-console-sub">
          <span>
            {active?.name || 'Laatste import'} · {active?.split || 'train'} · {active?.format || 'JSONL'}
          </span>
          <small>{connected ? 'BRAIN' : 'OFFLINE'}</small>
        </div>
        <div className="ds-console-log" ref={log} role="log" aria-label="Dataset importlog" aria-live="off">
          {!events.length && (
            <div className="ds-event ds-event-info">
              <time>—</time>
              <strong>INFO</strong>
              <div>
                <span>Nog geen job-events. Importeer een dataset of selecteer een actieve job.</span>
              </div>
            </div>
          )}
          {events.map((e) => (
            <div key={e.id} className={`ds-event ds-event-${e.type.toLowerCase()}`}>
              <time>{e.time}</time>
              <strong>{e.type}</strong>
              <div>
                {e.lines.map((line, i) => (
                  <span key={i}>{line}</span>
                ))}
              </div>
            </div>
          ))}
          {active && (
            <div className="ds-log-progress">
              <span>
                {formatBytes((active.bytes * active.progress) / 100)} / {formatBytes(active.bytes)} ·{' '}
                {active.progress}%
                {active.rootHash ? ` · ${active.rootHash.slice(0, 12)}…` : ''}
              </span>
              <progress max={100} value={active.progress} />
            </div>
          )}
        </div>
        <div className="ds-console-footer">
          <p>
            {awaitingGate
              ? 'Human gate: keur het recept goed vóór de full run.'
              : active?.status === 'downloading'
                ? 'Brain verwerkt shards / sample-job via request-driven ticks.'
                : active?.status === 'paused'
                  ? 'Gepauzeerd. Hervat om verder te tick’en.'
                  : active?.status === 'ready'
                    ? 'Dataset gereed in de bibliotheek.'
                    : active?.status === 'cancelled'
                      ? 'Gestopt of mislukt. Start een nieuwe import.'
                      : connected
                        ? 'Events komen uitsluitend van Atlas Brain.'
                        : 'Verbind Brain om te importeren.'}
          </p>
          <div>
            {awaitingGate ? (
              <>
                <button disabled={busy} onClick={() => void approveActive(true)}>
                  <Check /> Goedkeuren
                </button>
                <button disabled={busy} onClick={() => void approveActive(false)}>
                  <X /> Afwijzen
                </button>
              </>
            ) : (
              <>
                <button
                  disabled={!active?.jobId || !['downloading', 'paused', 'registering'].includes(active.status)}
                  onClick={() => void pause()}
                >
                  {active?.status === 'paused' ? <Play /> : <Pause />}
                  {active?.status === 'paused' ? 'Hervatten' : 'Pauzeren'}
                </button>
                <button
                  disabled={!active?.jobId || active.status === 'ready'}
                  onClick={() => {
                    if (active?.jobId) {
                      void fetchDatasetJob(baseUrl, active.jobId).then((p) =>
                        setEvents(mapBrainEvents(p.events)),
                      );
                    }
                  }}
                >
                  <Terminal /> Vernieuw log
                </button>
              </>
            )}
          </div>
          <label>
            <input
              type="checkbox"
              checked={autoScroll}
              onChange={(e) => setAutoScroll(e.target.checked)}
            />
            Automatisch scrollen
          </label>
          {copied && <small role="status">Log gekopieerd</small>}
        </div>
      </>
    );
  }

  return (
    <div className="datasets-page">
      <header className="ds-heading">
        <div>
          <span>ATLAS DATA LIBRARY</span>
          <h1>
            <Database />
            Datasets
          </h1>
          <p>Van bron naar bruikbare kennis. Alles op één plek.</p>
        </div>
        <b>
          <i />
          {connected ? 'BRAIN LIVE' : 'OFFLINE'}
        </b>
      </header>
      <div className="ds-body">
        <div className="ds-main">
          <div className="ds-stats">
            <button onClick={() => onFilterChange('Alle datasets')}>
              <Database />
              <span>
                <strong>{datasets.length}</strong>Datasets
              </span>
            </button>
            <button onClick={() => onFilterChange('Alle datasets')}>
              <FileText />
              <span>
                <strong>{count(datasets.reduce((s, d) => s + d.records, 0))}</strong>
                Records
              </span>
            </button>
            <button onClick={() => onFilterChange('Opslag')}>
              <Database />
              <span>
                <strong>
                  {formatBytes(datasets.reduce((s, d) => s + (d.bytes * d.progress) / 100, 0))}
                </strong>
                Lokale opslag
              </span>
            </button>
          </div>
          <section className="ds-import">
            <div className="ds-import-heading">
              <span className="ds-hug" aria-hidden="true">
                🤗
              </span>
              <div>
                <h2>Importeer van Hugging Face</h2>
                <p>Plak een dataset-URL of repository-ID (stub: atlas/tiny-instruct).</p>
              </div>
            </div>
            <form
              onSubmit={(e) => {
                e.preventDefault();
                void inspect();
              }}
            >
              <div className="ds-source-input">
                <span aria-hidden="true">↗</span>
                <input
                  ref={input}
                  aria-label="Dataset-URL of repository-ID"
                  placeholder="organisatie/dataset-naam"
                  value={repo}
                  onChange={(e) => setRepo(e.target.value)}
                  required
                />
                <button type="submit" disabled={busy || !connected}>
                  Dataset ophalen <ArrowDownToLine />
                </button>
              </div>
              <div className="ds-import-options">
                <label>
                  Configuratie:
                  <select
                    aria-label="Datasetconfiguratie"
                    value={config}
                    onChange={(e) => setConfig(e.target.value)}
                  >
                    <option>default</option>
                    <option>nl</option>
                    <option>en</option>
                  </select>
                </label>
                <label>
                  Split:
                  <select aria-label="Datasetsplit" value={split} onChange={(e) => setSplit(e.target.value)}>
                    <option>train</option>
                    <option>validation</option>
                    <option>test</option>
                  </select>
                </label>
                <label>
                  Formaat:
                  <select
                    aria-label="Datasetformaat"
                    value={format}
                    onChange={(e) => setFormat(e.target.value)}
                  >
                    <option>JSONL</option>
                    <option>Parquet</option>
                  </select>
                </label>
              </div>
              <p className="ds-import-note">
                <Info />
                Import loopt via Atlas Brain (pin · acquire · sample · human gate · full).
              </p>
              {error && (
                <p className="ds-error" role="alert">
                  {error}
                </p>
              )}
            </form>
          </section>
          <section className="ds-library">
            <div className="ds-library-title">
              <h2>{filter === 'Alle datasets' ? 'Mijn datasets' : filter}</h2>
              {filter !== 'Alle datasets' && (
                <button onClick={() => onFilterChange('Alle datasets')}>
                  Alle datasets <ArrowUpRight />
                </button>
              )}
            </div>
            {filter === 'Opslag' && (
              <p className="ds-info">Shard-bytes uit Brain (`data/datasets/` + object store).</p>
            )}
            {filter === 'Collecties' && (
              <div className="ds-collections">
                {['Research', 'Code & instructies', 'Eigen datasets'].map((c) => (
                  <button key={c} onClick={() => onFilterChange(c as DatasetFilter)}>
                    {c}
                    <b>{datasets.filter((d) => d.collection === c).length}</b>
                  </button>
                ))}
              </div>
            )}
            <div className="ds-library-tools">
              <label>
                <Search />
                <input
                  aria-label="Zoek datasets"
                  placeholder="Zoek datasets…"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                />
              </label>
              <select
                aria-label="Datasettype filter"
                value={kind}
                onChange={(e) => setKind(e.target.value)}
              >
                <option>Alle typen</option>
                <option>Tekst</option>
                <option>Code</option>
                <option>Afbeelding</option>
              </select>
              <button
                className={!grid ? 'selected' : ''}
                aria-label="Lijstweergave"
                onClick={() => setGrid(false)}
              >
                <List />
              </button>
              <button
                className={grid ? 'selected' : ''}
                aria-label="Rasterweergave"
                onClick={() => setGrid(true)}
              >
                <LayoutGrid />
              </button>
            </div>
            <div className={grid ? 'ds-records ds-grid' : 'ds-records'}>
              {!grid && (
                <div className="ds-table-head">
                  <span>DATASET</span>
                  <span>TYPE</span>
                  <span>RECORDS</span>
                  <span>GROOTTE</span>
                  <span>STATUS</span>
                  <span />
                </div>
              )}
              {shown.map((d) => (
                <div className="ds-row" key={d.id}>
                  <button className="ds-name" onClick={() => void openDetail(d)}>
                    <i>{datasetIcon(d)}</i>
                    <span>
                      <strong>{d.name}</strong>
                      <small>
                        Hugging Face · {d.repo}
                      </small>
                    </span>
                  </button>
                  <span className={`ds-kind ds-kind-${d.kind.toLowerCase()}`}>{d.kind}</span>
                  <span>{count(d.records)}</span>
                  <span>{formatBytes(d.bytes)}</span>
                  <button
                    className={`ds-status ds-status-${d.status}`}
                    onClick={() => {
                      setActiveId(d.id);
                      if (d.jobId) {
                        void fetchDatasetJob(baseUrl, d.jobId).then((p) =>
                          setEvents(mapBrainEvents(p.events)),
                        );
                      }
                      setExpanded(true);
                    }}
                  >
                    <span>
                      <i />
                      {statuses[d.status]}
                      {d.status !== 'ready' ? ` · ${d.progress}%` : ''}
                    </span>
                    {d.status !== 'ready' && <progress max={100} value={d.progress} />}
                  </button>
                  <div className="ds-row-actions">
                    <button
                      aria-label={`Acties voor ${d.name}`}
                      onClick={() => setMenu(menu === d.id ? '' : d.id)}
                    >
                      <MoreVertical />
                    </button>
                    {menu === d.id && (
                      <div className="ds-row-menu">
                        <button
                          onClick={() => {
                            void openDetail(d);
                            setMenu('');
                          }}
                        >
                          Details bekijken
                        </button>
                        <button
                          onClick={() => {
                            setActiveId(d.id);
                            setExpanded(true);
                            setMenu('');
                          }}
                        >
                          Toon in console
                        </button>
                      </div>
                    )}
                  </div>
                </div>
              ))}
              {!shown.length && (
                <p className="ds-empty">
                  {connected
                    ? 'Geen datasets. Probeer atlas/tiny-instruct te importeren.'
                    : 'Verbind Atlas Brain om datasets te laden.'}
                </p>
              )}
            </div>
          </section>
        </div>
        <section className="ds-console" aria-label="Importconsole">
          {!expanded && terminal()}
        </section>
      </div>
      <dialog className="ds-expanded" ref={consoleDialog} onCancel={() => setExpanded(false)}>
        {expanded && terminal()}
      </dialog>
      <dialog className="ds-dialog" ref={metadataDialog} onCancel={() => setMetadata(null)}>
        <header>
          <h2>Import voorbereiden</h2>
          <button aria-label="Importvenster sluiten" onClick={() => setMetadata(null)}>
            <X />
          </button>
        </header>
        {metadata && (
          <>
            <b className="ds-demo-label">CATALOGUS · ATLAS BRAIN</b>
            <h3>{metadata.repo}</h3>
            <dl>
              <dt>Configuratie / split</dt>
              <dd>
                {metadata.config} / {metadata.split}
              </dd>
              <dt>Formaat</dt>
              <dd>{metadata.format}</dd>
              <dt>Revision</dt>
              <dd>{metadata.revision}</dd>
            </dl>
            <p>
              Pin + acquire via Brain, daarna sample-job met human gate vóór full run. Gated sets
              vereisen secret <code>hf_token</code>.
            </p>
            <button className="ds-primary" disabled={running.length > 0 || busy} onClick={() => void start()}>
              Import starten <ArrowDownToLine />
            </button>
            {running.length > 0 && <p>Rond de actieve import af via de console.</p>}
          </>
        )}
      </dialog>
      <dialog
        className="ds-dialog ds-detail-dialog"
        ref={detailDialog}
        onCancel={() => setDetail(null)}
      >
        <header>
          <h2>{detail?.name}</h2>
          <button aria-label="Datasetdetails sluiten" onClick={() => setDetail(null)}>
            <X />
          </button>
        </header>
        {detail && (
          <>
            <b className="ds-demo-label">ATLAS BRAIN</b>
            <dl>
              <dt>Repository</dt>
              <dd>{detail.repo}</dd>
              <dt>Configuratie / split</dt>
              <dd>
                {detail.config} / {detail.split}
              </dd>
              <dt>Status</dt>
              <dd>
                {statuses[detail.status]} · {detail.progress}%
              </dd>
              <dt>Collectie</dt>
              <dd>{detail.collection}</dd>
              <dt>Records / grootte</dt>
              <dd>
                {count(detail.records)} / {formatBytes(detail.bytes)}
              </dd>
              {detail.rootHash && (
                <>
                  <dt>Root hash</dt>
                  <dd>{detail.rootHash}</dd>
                </>
              )}
            </dl>
            <h3>Records</h3>
            <pre>
              {JSON.stringify(previewRows.length ? previewRows : datasetPreview(detail), null, 2)}
            </pre>
          </>
        )}
      </dialog>
    </div>
  );
}
