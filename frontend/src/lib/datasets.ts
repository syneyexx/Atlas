/** Datasets types + Atlas Brain client. UI shows only backend-confirmed events. */

import { createId } from './id';

export type DatasetStatus =
  | 'ready'
  | 'downloading'
  | 'paused'
  | 'cancelled'
  | 'verifying'
  | 'registering';

export type Dataset = {
  id: string;
  name: string;
  repo: string;
  kind: 'Tekst' | 'Code' | 'Afbeelding';
  records: number;
  bytes: number;
  status: DatasetStatus;
  progress: number;
  collection: string;
  config: string;
  split: string;
  format: string;
  revision: string;
  /** Brain dataset status */
  brainStatus?: string;
  rootHash?: string;
  recipeId?: string;
  jobId?: string;
  example?: boolean;
};

export type ImportEvent = {
  id: string;
  time: string;
  type:
    | 'INFO'
    | 'HTTP'
    | 'PLAN'
    | 'DISK'
    | 'GET'
    | 'OK'
    | 'WAIT'
    | 'PAUSE'
    | 'CANCEL'
    | 'VERIFY'
    | 'WRITE'
    | 'QUEUE'
    | 'EXEC'
    | 'DONE'
    | 'APPROVE'
    | 'REJECT';
  lines: string[];
};

export type CatalogCard = {
  source?: string;
  id: string;
  title?: string;
  description?: string;
  license?: string;
  modality?: string;
  gated?: boolean;
  tags?: string[];
  revision?: string;
  downloads?: number;
};

export type DatasetFilter =
  | 'Alle datasets'
  | 'Downloads'
  | 'Collecties'
  | 'Opslag'
  | 'Research'
  | 'Code & instructies'
  | 'Eigen datasets';

type BrainDataset = {
  id: string;
  source: string;
  hub_id: string;
  revision: string;
  title: string;
  description: string;
  license: string;
  modality: string;
  gated: boolean;
  root_hash: string;
  status: string;
  project_id?: string | null;
  meta?: Record<string, unknown>;
};

type BrainShard = {
  id: string;
  name: string;
  split: string;
  size_bytes: number;
  checksum: string;
  download_state: string;
};

type BrainEvent = {
  id: string;
  timestamp: string;
  type?: string;
  event_type?: string;
  lines: string[];
  details?: string | null;
};

type BrainJob = {
  id: string;
  dataset_id: string;
  recipe_id?: string | null;
  kind: string;
  status: string;
  phase: string;
  paused: boolean;
  metrics?: Record<string, unknown>;
};

function atlasRoot(baseUrl: string) {
  return baseUrl.replace(/\/$/, '').replace(/\/v1$/, '') + '/v1/atlas';
}

export function formatBytes(bytes: number) {
  if (!bytes) return '0 MB';
  return (
    new Intl.NumberFormat('nl-NL', { maximumFractionDigits: 1 }).format(
      bytes >= 1e9 ? bytes / 1e9 : bytes / 1e6,
    ) + (bytes >= 1e9 ? ' GB' : ' MB')
  );
}

export function count(value: number) {
  return new Intl.NumberFormat('nl-NL', {
    notation: value >= 1e6 ? 'compact' : 'standard',
    maximumFractionDigits: 1,
  }).format(value);
}

export function logEvent(
  type: ImportEvent['type'],
  lines: string[],
  offset = 0,
): ImportEvent {
  return {
    id: createId(),
    type,
    lines,
    time: new Intl.DateTimeFormat('nl-NL', {
      hour: '2-digit',
      minute: '2-digit',
      second: '2-digit',
      timeZone: 'Europe/Amsterdam',
    }).format(new Date(Date.now() + offset)),
  };
}

export function parseRepository(input: string): string {
  const text = input.trim();
  let repo = text;
  if (/^https?:/i.test(text)) {
    const url = new URL(text);
    if (url.protocol !== 'https:' || url.hostname !== 'huggingface.co') {
      throw Error('Gebruik een https://huggingface.co/datasets/… URL.');
    }
    const parts = url.pathname.split('/').filter(Boolean);
    if (parts[0] !== 'datasets' || parts.length < 3) {
      throw Error('Gebruik een dataset-URL met organisatie en naam.');
    }
    repo = parts.slice(1, 3).join('/');
  }
  if (!/^[a-zA-Z0-9][a-zA-Z0-9_.-]*\/[a-zA-Z0-9][a-zA-Z0-9_.-]*$/.test(repo)) {
    throw Error('Vul een repository-ID in als organisatie/dataset-naam.');
  }
  return repo;
}

function modalityKind(modality: string): Dataset['kind'] {
  const m = modality.toLowerCase();
  if (m.includes('code')) return 'Code';
  if (m.includes('image') || m.includes('vision') || m.includes('beeld')) return 'Afbeelding';
  return 'Tekst';
}

function collectionFor(kind: Dataset['kind'], hubId: string): string {
  if (kind === 'Code') return 'Code & instructies';
  if (/atlas\//i.test(hubId) || /research/i.test(hubId)) return 'Research';
  return 'Eigen datasets';
}

function statusFromBrain(ds: BrainDataset, job?: BrainJob | null): {
  status: DatasetStatus;
  progress: number;
} {
  if (job) {
    if (job.paused || job.status === 'paused') return { status: 'paused', progress: 55 };
    if (job.phase === 'await_approval' || job.status === 'blocked') {
      return { status: 'verifying', progress: 90 };
    }
    if (job.status === 'completed' || job.phase === 'done') {
      return { status: 'ready', progress: 100 };
    }
    if (job.status === 'failed') return { status: 'cancelled', progress: 0 };
    if (job.kind === 'sample') return { status: 'registering', progress: 70 };
    return { status: 'downloading', progress: 45 };
  }
  if (ds.status === 'acquired' || ds.root_hash) return { status: 'ready', progress: 100 };
  if (ds.status === 'pinned') return { status: 'registering', progress: 20 };
  return { status: 'downloading', progress: 10 };
}

export function mapBrainDataset(
  ds: BrainDataset,
  shards: BrainShard[] = [],
  facetsTotal = 0,
  job?: BrainJob | null,
): Dataset {
  const kind = modalityKind(ds.modality || 'text');
  const bytes = shards.reduce((s, sh) => s + (sh.size_bytes || 0), 0);
  const { status, progress } = statusFromBrain(ds, job);
  return {
    id: ds.id,
    name: ds.title || ds.hub_id,
    repo: ds.hub_id,
    kind,
    records: facetsTotal,
    bytes: bytes || 1e6,
    status,
    progress,
    collection: collectionFor(kind, ds.hub_id),
    config: 'default',
    split: shards[0]?.split || 'train',
    format: 'JSONL',
    revision: ds.revision,
    brainStatus: ds.status,
    rootHash: ds.root_hash,
    recipeId: job?.recipe_id || undefined,
    jobId: job?.id,
  };
}

export function mapCatalogToPreview(card: CatalogCard, config: string, split: string, format: string): Dataset {
  const kind = modalityKind(card.modality || 'text');
  return {
    id: createId(),
    name: card.title || card.id,
    repo: card.id,
    kind,
    records: 0,
    bytes: 2e6,
    status: 'downloading',
    progress: 0,
    collection: collectionFor(kind, card.id),
    config,
    split,
    format,
    revision: card.revision || 'latest',
  };
}

export function mapBrainEvents(events: BrainEvent[]): ImportEvent[] {
  return (events || []).map((e) => {
    const raw = (e.type || e.event_type || 'INFO').toUpperCase();
    const allowed: ImportEvent['type'][] = [
      'INFO',
      'HTTP',
      'PLAN',
      'DISK',
      'GET',
      'OK',
      'WAIT',
      'PAUSE',
      'CANCEL',
      'VERIFY',
      'WRITE',
      'QUEUE',
      'EXEC',
      'DONE',
      'APPROVE',
      'REJECT',
    ];
    const type = (allowed.includes(raw as ImportEvent['type']) ? raw : 'EXEC') as ImportEvent['type'];
    const ts = e.timestamp || '';
    const time = ts.includes('T')
      ? new Intl.DateTimeFormat('nl-NL', {
          hour: '2-digit',
          minute: '2-digit',
          second: '2-digit',
          timeZone: 'Europe/Amsterdam',
        }).format(new Date(ts))
      : ts.slice(11, 19) || logEvent('INFO', []).time;
    return {
      id: e.id || createId(),
      time,
      type,
      lines: e.lines || [],
    };
  });
}

export function datasetPreview(d: Dataset) {
  return d.kind === 'Code'
    ? [
        {
          instruction: 'Schrijf een functie die bronverwijzingen verzamelt.',
          response: 'def sources(records): return [r["source"] for r in records]',
        },
      ]
    : d.kind === 'Afbeelding'
      ? [
          {
            image: 'demo/images/0001.jpg',
            caption: 'Voorbeeld van een gelabelde afbeelding',
            label: 'landschap',
          },
        ]
      : [
          {
            question: 'Hoe bewaart Atlas bronherkomst?',
            answer: 'Via commit-SHA pin, shard checksums en content-addressed records.',
          },
          {
            question: 'Waar draait de pipeline?',
            answer: 'In Atlas Brain — request-driven ticks, geen aparte ETL-daemon.',
          },
        ];
}

async function readError(response: Response) {
  const text = await response.text().catch(() => '');
  try {
    const j = JSON.parse(text) as { error?: { message?: string } };
    return j.error?.message || text;
  } catch {
    return text;
  }
}

export async function searchCatalog(
  baseUrl: string,
  q: string,
  limit = 20,
  signal?: AbortSignal,
): Promise<CatalogCard[]> {
  const url = `${atlasRoot(baseUrl)}/datasets/catalog/search?q=${encodeURIComponent(q)}&limit=${limit}`;
  const response = await fetch(url, { signal });
  if (!response.ok) throw new Error(`Catalogus zoeken mislukt (HTTP ${response.status}).`);
  const data = (await response.json()) as { results: CatalogCard[] };
  return data.results || [];
}

export async function getCatalogCard(
  baseUrl: string,
  source: string,
  id: string,
  signal?: AbortSignal,
): Promise<CatalogCard> {
  const response = await fetch(
    `${atlasRoot(baseUrl)}/datasets/catalog/${encodeURIComponent(source)}/${encodeURIComponent(id)}`,
    { signal },
  );
  if (!response.ok) throw new Error(`Cataloguskaart niet gevonden (HTTP ${response.status}).`);
  return (await response.json()) as CatalogCard;
}

export async function listDatasets(baseUrl: string, signal?: AbortSignal): Promise<Dataset[]> {
  const response = await fetch(`${atlasRoot(baseUrl)}/datasets?limit=100`, { signal });
  if (!response.ok) throw new Error(`Datasets laden mislukt (HTTP ${response.status}).`);
  const data = (await response.json()) as { datasets: BrainDataset[] };
  const jobsRes = await fetch(`${atlasRoot(baseUrl)}/datasets/jobs?limit=50`, { signal });
  const jobs = jobsRes.ok
    ? (((await jobsRes.json()) as { jobs: BrainJob[] }).jobs || [])
    : [];
  const out: Dataset[] = [];
  for (const ds of data.datasets || []) {
    const detail = await fetch(`${atlasRoot(baseUrl)}/datasets/${ds.id}`, { signal });
    const payload = detail.ok
      ? ((await detail.json()) as { dataset: BrainDataset; shards: BrainShard[] })
      : { dataset: ds, shards: [] as BrainShard[] };
    let facetsTotal = 0;
    try {
      const facets = await fetch(`${atlasRoot(baseUrl)}/datasets/${ds.id}/facets`, { signal });
      if (facets.ok) {
        const f = (await facets.json()) as { total?: number };
        facetsTotal = f.total || 0;
      }
    } catch {
      /* optional */
    }
    const job = jobs.find((j) => j.dataset_id === ds.id && j.status !== 'completed')
      || jobs.find((j) => j.dataset_id === ds.id);
    out.push(mapBrainDataset(payload.dataset, payload.shards || [], facetsTotal, job));
  }
  return out;
}

export async function pinAndAcquire(
  baseUrl: string,
  hubId: string,
  revision?: string,
  signal?: AbortSignal,
): Promise<{ dataset: BrainDataset; shards: BrainShard[] }> {
  const response = await fetch(`${atlasRoot(baseUrl)}/datasets`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      source: 'hf',
      id: hubId,
      revision: revision || undefined,
      acquire: true,
    }),
    signal,
  });
  if (!response.ok) {
    throw new Error((await readError(response)) || `Acquire mislukt (HTTP ${response.status}).`);
  }
  return (await response.json()) as { dataset: BrainDataset; shards: BrainShard[] };
}

export async function createDatasetJob(
  baseUrl: string,
  body: { dataset_id: string; kind: 'sample' | 'full'; recipe_id?: string; auto_run?: boolean },
  signal?: AbortSignal,
): Promise<{ job: BrainJob; events: BrainEvent[] }> {
  const response = await fetch(`${atlasRoot(baseUrl)}/datasets/jobs`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    signal,
  });
  if (!response.ok) {
    throw new Error((await readError(response)) || `Job starten mislukt (HTTP ${response.status}).`);
  }
  return (await response.json()) as { job: BrainJob; events: BrainEvent[] };
}

export async function fetchDatasetJob(
  baseUrl: string,
  id: string,
  signal?: AbortSignal,
): Promise<{ job: BrainJob; events: BrainEvent[] }> {
  const response = await fetch(`${atlasRoot(baseUrl)}/datasets/jobs/${id}`, { signal });
  if (!response.ok) throw new Error(`Job laden mislukt (HTTP ${response.status}).`);
  return (await response.json()) as { job: BrainJob; events: BrainEvent[] };
}

export async function tickDatasetJob(
  baseUrl: string,
  id: string,
  steps = 4,
  signal?: AbortSignal,
): Promise<{ job: BrainJob; events: BrainEvent[]; tick_message?: string; done?: boolean }> {
  const response = await fetch(`${atlasRoot(baseUrl)}/datasets/jobs/${id}/tick`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ steps }),
    signal,
  });
  if (!response.ok) throw new Error((await readError(response)) || `Tick mislukt.`);
  return (await response.json()) as {
    job: BrainJob;
    events: BrainEvent[];
    tick_message?: string;
    done?: boolean;
  };
}

export async function pauseDatasetJob(
  baseUrl: string,
  id: string,
  paused: boolean,
  signal?: AbortSignal,
): Promise<{ job: BrainJob; events: BrainEvent[] }> {
  const response = await fetch(`${atlasRoot(baseUrl)}/datasets/jobs/${id}/pause`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ paused }),
    signal,
  });
  if (!response.ok) throw new Error((await readError(response)) || `Pauze mislukt.`);
  return (await response.json()) as { job: BrainJob; events: BrainEvent[] };
}

export async function approveRecipe(
  baseUrl: string,
  recipeId: string,
  approve: boolean,
  signal?: AbortSignal,
): Promise<void> {
  const response = await fetch(
    `${atlasRoot(baseUrl)}/datasets/recipes/${recipeId}/approve`,
    {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ approve }),
      signal,
    },
  );
  if (!response.ok) throw new Error((await readError(response)) || `Approve mislukt.`);
}

export async function listRecords(
  baseUrl: string,
  datasetId: string,
  limit = 5,
  signal?: AbortSignal,
): Promise<Record<string, unknown>[]> {
  const response = await fetch(
    `${atlasRoot(baseUrl)}/datasets/${datasetId}/records?limit=${limit}`,
    { signal },
  );
  if (!response.ok) return [];
  const data = (await response.json()) as { records: Array<{ title: string; meta?: Record<string, unknown>; contaminated?: boolean }> };
  return (data.records || []).map((r) => ({
    title: r.title,
    text: r.meta?.text,
    contaminated: r.contaminated,
  }));
}
