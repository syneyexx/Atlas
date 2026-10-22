/** Atlas Brain control-plane client (settings, memory, knowledge). */

export type BrainSettings = {
  system_prompt: string;
  model_id: string;
  lm_base_url: string;
  lm_provider: string;
  knowledge_top_k: number;
  knowledge_max_chars: number;
};

export type BrainMessage = {
  role: string;
  content: string;
  created_at?: string;
  callout?: { title: string; content: string };
};

export type BrainConversation = {
  id: string;
  title: string;
  messages: BrainMessage[];
  updated_at: string;
};

export type KnowledgeNode = {
  id: string;
  title: string;
  content: string;
  tags: string[];
  created_at: string;
};

function atlasRoot(baseUrl: string) {
  return baseUrl.replace(/\/$/, '').replace(/\/v1$/, '') + '/v1/atlas';
}

export async function fetchSettings(baseUrl: string, signal?: AbortSignal): Promise<BrainSettings> {
  const response = await fetch(`${atlasRoot(baseUrl)}/settings`, { signal });
  if (!response.ok) throw new Error(`Settings laden mislukt (HTTP ${response.status}).`);
  return response.json() as Promise<BrainSettings>;
}

export async function saveSettings(
  baseUrl: string,
  patch: Partial<BrainSettings>,
  signal?: AbortSignal,
): Promise<BrainSettings> {
  const response = await fetch(`${atlasRoot(baseUrl)}/settings`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(patch),
    signal,
  });
  if (!response.ok) throw new Error(`Settings opslaan mislukt (HTTP ${response.status}).`);
  return response.json() as Promise<BrainSettings>;
}

export async function fetchConversations(baseUrl: string, signal?: AbortSignal): Promise<BrainConversation[]> {
  const response = await fetch(`${atlasRoot(baseUrl)}/conversations`, { signal });
  if (!response.ok) throw new Error(`Gesprekken laden mislukt (HTTP ${response.status}).`);
  return response.json() as Promise<BrainConversation[]>;
}

export async function upsertConversation(
  baseUrl: string,
  conversation: BrainConversation,
  signal?: AbortSignal,
): Promise<BrainConversation> {
  const response = await fetch(`${atlasRoot(baseUrl)}/conversations`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(conversation),
    signal,
  });
  if (!response.ok) throw new Error(`Gesprek opslaan mislukt (HTTP ${response.status}).`);
  return response.json() as Promise<BrainConversation>;
}

export async function learnKnowledge(
  baseUrl: string,
  body: { title: string; content: string; tags?: string[] },
  signal?: AbortSignal,
): Promise<KnowledgeNode> {
  const response = await fetch(`${atlasRoot(baseUrl)}/knowledge`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    signal,
  });
  if (!response.ok) throw new Error(`Kennis opslaan mislukt (HTTP ${response.status}).`);
  return response.json() as Promise<KnowledgeNode>;
}
