/** OpenAI-compatible adapter aimed at Atlas Brain (local LM behind the brain). */
export type RuntimeConfig = { baseUrl: string; model: string };
export type ChatMessage = {
  role: 'user' | 'assistant';
  content: string;
  createdAt?: string;
  callout?: { title: string; content: string };
};

export type CompleteOptions = {
  conversationId?: string;
  conversationTitle?: string;
  persist?: boolean;
};

export async function verifyRuntime(config: RuntimeConfig, signal?: AbortSignal) {
  const response = await fetch(`${config.baseUrl.replace(/\/$/, '')}/models`, { signal });
  if (!response.ok) throw new Error(`Runtime antwoordt met HTTP ${response.status}.`);
  const result = await response.json() as { data?: { id: string }[] };
  const models = Array.isArray(result.data) ? result.data : [];
  if (!models.some((m: { id: string }) => m.id === config.model)) {
    throw new Error('Dit model is niet gevonden in de runtime. Controleer de model-ID.');
  }
}

export async function completeChat(
  config: RuntimeConfig,
  messages: ChatMessage[],
  signal?: AbortSignal,
  options: CompleteOptions = {},
): Promise<string> {
  const response = await fetch(`${config.baseUrl.replace(/\/$/, '')}/chat/completions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      model: config.model,
      messages: messages.map(({ role, content, callout }) => ({
        role,
        content: callout ? `${content}\n\n${callout.title}\n${callout.content}` : content,
      })),
      stream: false,
      conversation_id: options.conversationId,
      conversation_title: options.conversationTitle,
      persist: options.persist ?? true,
    }),
    signal,
  });
  if (!response.ok) throw new Error(`Runtime antwoordt met HTTP ${response.status}.`);
  const result = await response.json() as { choices?: { message?: { content?: string } }[] };
  const content = result.choices?.[0]?.message?.content;
  if (typeof content !== 'string') throw new Error('De runtime gaf geen geldig antwoord.');
  return content;
}
