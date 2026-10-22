/** Research types + Brain client. UI never invents SEARCH/FETCH/EXEC events. */

export type ResearchEventType =
  | 'QUEUE'
  | 'SEARCH'
  | 'FETCH'
  | 'PARSE'
  | 'EXEC'
  | 'WRITE'
  | 'WAIT'
  | 'PAUSE';

export type ResearchEvent = {
  id: string;
  timestamp: string;
  type: ResearchEventType;
  lines: string[];
  details?: string;
};

export type ResearchAttachment = { name: string; type: string; size: number; text?: string };

export type ResearchTask = {
  id: string;
  title: string;
  mode: string;
  example?: boolean;
  paused?: boolean;
  status?: string;
  phase?: string;
  question?: string;
  events: ResearchEvent[];
  attachment?: ResearchAttachment;
  sources?: ResearchSource[];
  findings?: ResearchFinding[];
  openQuestions?: string[];
  claims?: ResearchClaim[];
  reportMd?: string;
  clarifications?: Clarifier[];
};

export type ResearchSource = {
  id: string;
  url: string;
  title: string;
  fetched_at: string;
  quality_tier: string;
  fragment: string;
};

export type ResearchFinding = {
  subquestion?: string;
  source_id?: string;
  summary?: string;
  quality_tier?: string;
  url?: string;
  title?: string;
};

export type ResearchClaim = {
  id: string;
  claim: string;
  source_ids: string[];
  verified: boolean;
  note: string;
};

export type Clarifier = { id: string; question: string; answer?: string };

export const exampleResearch: ResearchTask = {
  id: 'research-example',
  title: 'Hoe bouw je een betrouwbaar lokaal AI-framework?',
  mode: 'Verdiepend',
  example: true,
  status: 'running',
  phase: 'reviewing',
  events: [
    { id: 'e1', timestamp: '14:28:03', type: 'QUEUE', lines: ['research.local_ai · taak gestart'] },
    { id: 'e2', timestamp: '14:28:04', type: 'SEARCH', lines: ['query: local LLM tool calling', 'limiet: 5 bronnen'] },
    { id: 'e3', timestamp: '14:28:06', type: 'FETCH', lines: ['bron: docs.djl.ai', 'status: 200 · 18.4 kB'] },
    { id: 'e4', timestamp: '14:28:08', type: 'PARSE', lines: ['3 relevante passages gevonden'] },
    {
      id: 'e5',
      timestamp: '14:28:10',
      type: 'EXEC',
      lines: ['python verify_claims.py', 'exit: 0 · 2 checks geslaagd'],
      details:
        'VOORBEELD — niet uitgevoerd.\n\nCommando: python verify_claims.py\nWerkmap: research/local-ai\nExitcode: 0\nStandaarduitvoer: 2 checks geslaagd\nStandaardfouten: geen\n\nEen toekomstige onderzoeksbackend levert hier echte argumenten, stdout, stderr en exitcodes.',
    },
    { id: 'e6', timestamp: '14:28:12', type: 'WRITE', lines: ['research/local-ai/notities.md'] },
    { id: 'e7', timestamp: '14:28:13', type: 'WAIT', lines: ['Volgende bron in wachtrij'] },
  ],
  sources: [],
  findings: [],
  openQuestions: ['Geheugen versus modeltraining', 'Betrouwbaarheid van toolcalling', 'Grenzen van lokale hardware'],
};

const EVENT_TYPES: ResearchEventType[] = ['QUEUE', 'SEARCH', 'FETCH', 'PARSE', 'EXEC', 'WRITE', 'WAIT', 'PAUSE'];

export function researchTime() {
  return new Intl.DateTimeFormat('nl-NL', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    timeZone: 'Europe/Amsterdam',
  }).format(new Date());
}

export function isResearchTask(value: unknown): value is ResearchTask {
  if (!value || typeof value !== 'object') return false;
  const item = value as ResearchTask;
  return (
    typeof item.id === 'string' &&
    typeof item.title === 'string' &&
    typeof item.mode === 'string' &&
    Array.isArray(item.events) &&
    item.events.every(
      (event) =>
        event &&
        typeof event.id === 'string' &&
        typeof event.timestamp === 'string' &&
        EVENT_TYPES.includes(event.type) &&
        Array.isArray(event.lines) &&
        event.lines.every((line) => typeof line === 'string'),
    )
  );
}

function atlasRoot(baseUrl: string) {
  return baseUrl.replace(/\/$/, '').replace(/\/v1$/, '') + '/v1/atlas';
}

type BrainEvent = {
  id: string;
  timestamp: string;
  type?: string;
  event_type?: string;
  lines: string[];
  details?: string | null;
};

type BrainJobPayload = {
  job: {
    id: string;
    title: string;
    mode: string;
    status: string;
    phase: string;
    question: string;
    paused: boolean;
    report_md?: string;
    clarifications?: Clarifier[] | unknown;
    attachment?: ResearchAttachment | null;
  };
  events: BrainEvent[];
  sources: ResearchSource[];
  findings: ResearchFinding[];
  open_questions: string[] | unknown;
  claims: ResearchClaim[];
};

function mapEvent(event: BrainEvent): ResearchEvent {
  const raw = (event.type || event.event_type || 'WAIT') as string;
  const type = (EVENT_TYPES.includes(raw as ResearchEventType) ? raw : 'EXEC') as ResearchEventType;
  return {
    id: event.id,
    timestamp: event.timestamp,
    type,
    lines: event.lines || [],
    details: event.details || undefined,
  };
}

export function mapBrainJob(payload: BrainJobPayload): ResearchTask {
  const clarifications = Array.isArray(payload.job.clarifications)
    ? (payload.job.clarifications as Clarifier[])
    : [];
  const openQuestions = Array.isArray(payload.open_questions)
    ? (payload.open_questions as string[])
    : [];
  return {
    id: payload.job.id,
    title: payload.job.title,
    mode: payload.job.mode,
    status: payload.job.status,
    phase: payload.job.phase,
    question: payload.job.question,
    paused: payload.job.paused,
    events: (payload.events || []).map(mapEvent),
    attachment: payload.job.attachment || undefined,
    sources: payload.sources || [],
    findings: payload.findings || [],
    openQuestions,
    claims: payload.claims || [],
    reportMd: payload.job.report_md || '',
    clarifications,
  };
}

export async function createResearchJob(
  baseUrl: string,
  body: {
    question: string;
    mode: string;
    attachment?: ResearchAttachment;
    auto_run?: boolean;
  },
  signal?: AbortSignal,
): Promise<ResearchTask> {
  const response = await fetch(`${atlasRoot(baseUrl)}/research/jobs`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      question: body.question,
      mode: body.mode,
      attachment: body.attachment,
      auto_run: body.auto_run ?? true,
    }),
    signal,
  });
  if (!response.ok) {
    const text = await response.text().catch(() => '');
    throw new Error(`Research starten mislukt (HTTP ${response.status}). ${text}`);
  }
  return mapBrainJob((await response.json()) as BrainJobPayload);
}

export async function fetchResearchJob(
  baseUrl: string,
  id: string,
  signal?: AbortSignal,
): Promise<ResearchTask> {
  const response = await fetch(`${atlasRoot(baseUrl)}/research/jobs/${id}`, { signal });
  if (!response.ok) throw new Error(`Research laden mislukt (HTTP ${response.status}).`);
  return mapBrainJob((await response.json()) as BrainJobPayload);
}

export async function listResearchJobs(baseUrl: string, signal?: AbortSignal): Promise<ResearchTask[]> {
  const response = await fetch(`${atlasRoot(baseUrl)}/research/jobs?limit=50`, { signal });
  if (!response.ok) throw new Error(`Research-lijst mislukt (HTTP ${response.status}).`);
  const data = (await response.json()) as { jobs: BrainJobPayload['job'][] };
  return (data.jobs || []).map((job) => ({
    id: job.id,
    title: job.title,
    mode: job.mode,
    status: job.status,
    phase: job.phase,
    question: job.question,
    paused: job.paused,
    events: [],
    reportMd: job.report_md || '',
  }));
}

export async function tickResearchJob(
  baseUrl: string,
  id: string,
  steps = 4,
  signal?: AbortSignal,
): Promise<ResearchTask> {
  const response = await fetch(`${atlasRoot(baseUrl)}/research/jobs/${id}/tick`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ steps }),
    signal,
  });
  if (!response.ok) throw new Error(`Research tick mislukt (HTTP ${response.status}).`);
  return mapBrainJob((await response.json()) as BrainJobPayload);
}

export async function pauseResearchJob(
  baseUrl: string,
  id: string,
  paused: boolean,
  signal?: AbortSignal,
): Promise<ResearchTask> {
  const response = await fetch(`${atlasRoot(baseUrl)}/research/jobs/${id}/pause`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ paused }),
    signal,
  });
  if (!response.ok) throw new Error(`Research pauze mislukt (HTTP ${response.status}).`);
  return mapBrainJob((await response.json()) as BrainJobPayload);
}

export async function clarifyResearchJob(
  baseUrl: string,
  id: string,
  answers: Clarifier[],
  signal?: AbortSignal,
): Promise<ResearchTask> {
  const response = await fetch(`${atlasRoot(baseUrl)}/research/jobs/${id}/clarify`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ answers }),
    signal,
  });
  if (!response.ok) throw new Error(`Research clarify mislukt (HTTP ${response.status}).`);
  return mapBrainJob((await response.json()) as BrainJobPayload);
}

/** Map backend phase → UI phase rail index (0..3). */
export function phaseRail(phase?: string): { complete: number; current: number } {
  switch (phase) {
    case 'intake':
    case 'planning':
      return { complete: 0, current: 0 };
    case 'researching':
      return { complete: 1, current: 1 };
    case 'reviewing':
      return { complete: 2, current: 2 };
    case 'synthesizing':
    case 'verifying':
      return { complete: 3, current: 3 };
    case 'done':
      return { complete: 4, current: -1 };
    default:
      return { complete: 0, current: 0 };
  }
}
