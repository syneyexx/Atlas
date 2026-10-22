'use client';

import { createId } from '../lib/id';
import { useEffect, useRef, useState } from 'react';
import {
  ArrowRight,
  FileText,
  Folder,
  Grid2X2,
  Menu,
  MessageSquare,
  Paperclip,
  Plus,
  Send,
  Settings,
  Square,
  X,
} from 'lucide-react';
import { completeChat, verifyRuntime, type ChatMessage, type RuntimeConfig } from '../lib/runtime';
import {
  fetchConversations,
  fetchSettings,
  learnKnowledge,
  saveSettings,
  upsertConversation,
  type BrainSettings,
} from '../lib/brain';
import { ResearchWorkspace } from './ResearchWorkspace';
import { exampleResearch, type ResearchTask } from '../lib/research';
import { ChatThread } from './ChatThread';
import { defaultConversations, isConversation, type Conversation } from '../lib/conversations';

const sections = ['Chat', 'Data', 'Coding', 'Trading', 'Models', 'Settings'] as const;
type Section = (typeof sections)[number];

const emptySettings: BrainSettings = {
  system_prompt: '',
  model_id: '',
  lm_base_url: 'http://127.0.0.1:11434/v1',
  lm_provider: 'local_http',
  knowledge_top_k: 6,
  knowledge_max_chars: 6000,
};

function ModelLoader({
  filename,
  onChoose,
  onLoad,
}: {
  filename: string;
  onChoose: () => void;
  onLoad: () => void;
}) {
  return (
    <section className="model-card" aria-label="Lokaal model">
      <h2>Lokaal model</h2>
      <div className="model-controls">
        <button className="file-picker" onClick={onChoose}>
          <Folder aria-hidden="true" />
          <span>{filename || 'Selecteer een modelbestand'}</span>
          <span className="extension">.gguf</span>
        </button>
        <button className="load-button" onClick={onLoad}>
          Model laden <ArrowRight aria-hidden="true" />
        </button>
      </div>
    </section>
  );
}

export function AtlasWorkspace() {
  const [researchTasks, setResearchTasks] = useState<ResearchTask[]>([exampleResearch]);
  const [researchId, setResearchId] = useState(exampleResearch.id);
  const [researchRequest, setResearchRequest] = useState(0);
  const [section, setSection] = useState<Section>('Chat');
  const [subpage, setSubpage] = useState('Chat');
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [filename, setFilename] = useState('');
  const [conversations, setConversations] = useState<Conversation[]>(defaultConversations);
  const [activeId, setActiveId] = useState(defaultConversations[0].id);
  const [draft, setDraft] = useState('');
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [config, setConfig] = useState<RuntimeConfig>({
    baseUrl: 'http://127.0.0.1:8080/v1',
    model: '',
  });
  const [brainSettings, setBrainSettings] = useState<BrainSettings>(emptySettings);
  const [knowledgeDraft, setKnowledgeDraft] = useState({ title: '', content: '' });
  const [connected, setConnected] = useState(false);
  const [busy, setBusy] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState('');
  const fileInput = useRef<HTMLInputElement>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const attachmentInput = useRef<HTMLInputElement>(null);
  const [attachment, setAttachment] = useState<{ name: string; text: string } | null>(null);
  const abort = useRef<AbortController | null>(null);
  const active = conversations.find((c) => c.id === activeId) || conversations[0];

  useEffect(() => {
    if (window.location.hash === '#research') {
      setSection('Data');
      setSubpage('Research');
    }
  }, []);

  useEffect(() => {
    try {
      const saved = localStorage.getItem('atlas999.conversations');
      if (saved) {
        const parsed: unknown = JSON.parse(saved);
        if (Array.isArray(parsed) && parsed.every(isConversation)) {
          const owned = parsed.filter((c) => !c.example);
          const defaults = defaultConversations.filter((c) => !owned.some((o) => o.id === c.id));
          setConversations([...defaults, ...owned]);
        }
      }
      const savedRuntime = localStorage.getItem('atlas999.runtime');
      if (savedRuntime) {
        const parsed = JSON.parse(savedRuntime) as RuntimeConfig;
        if (parsed?.baseUrl && typeof parsed.model === 'string') setConfig(parsed);
      }
    } catch {
      /* storage is optional */
    }
  }, []);

  useEffect(() => {
    if (settingsOpen) dialog.current?.showModal();
    else dialog.current?.close();
  }, [settingsOpen]);

  useEffect(() => () => abort.current?.abort(), []);

  function updateConversations(next: Conversation[]) {
    setConversations(next);
    try {
      localStorage.setItem(
        'atlas999.conversations',
        JSON.stringify(next.filter((c) => !c.example)),
      );
    } catch {
      /* private browsing remains usable */
    }
  }

  function persistRuntime(next: RuntimeConfig) {
    setConfig(next);
    try {
      localStorage.setItem('atlas999.runtime', JSON.stringify(next));
    } catch {
      /* optional */
    }
  }

  async function syncFromBrain(baseUrl: string) {
    try {
      const remote = await fetchConversations(baseUrl);
      if (!remote.length) return;
      const mapped: Conversation[] = remote.map((c) => ({
        id: c.id,
        title: c.title,
        messages: c.messages.map((m) => ({
          role: m.role === 'assistant' ? 'assistant' : 'user',
          content: m.content,
          createdAt: m.created_at,
          callout: m.callout,
        })),
      }));
      const examples = defaultConversations.filter((c) => c.example);
      const locals = conversations.filter(
        (c) => !c.example && !mapped.some((m) => m.id === c.id),
      );
      updateConversations([...examples, ...mapped, ...locals]);
    } catch {
      /* brain memory sync is best-effort until connected */
    }
  }

  function newChat() {
    if (busy) return;
    const chat = { id: createId(), title: 'Nieuw gesprek', messages: [] };
    updateConversations([chat, ...conversations]);
    setActiveId(chat.id);
    setSection('Chat');
    setSubpage('Chat');
    setSidebarOpen(false);
    setDraft('');
    setAttachment(null);
    setError('');
  }

  function newResearch() {
    setSection('Data');
    setSubpage('Research');
    setResearchRequest((value) => value + 1);
    setSidebarOpen(false);
  }

  function selectSection(next: Section) {
    if (next === 'Settings') {
      setSettingsOpen(true);
      void loadBrainSettings();
      return;
    }
    setSection(next);
    setSubpage(next === 'Data' ? 'Research' : 'Chat');
  }

  async function loadBrainSettings() {
    try {
      const settings = await fetchSettings(config.baseUrl);
      setBrainSettings(settings);
      if (settings.model_id && !config.model) {
        persistRuntime({ ...config, model: settings.model_id });
      }
    } catch {
      /* settings panel still usable offline for editing brain URL */
    }
  }

  async function connect() {
    setError('');
    setConnecting(true);
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 12000);
    try {
      const url = new URL(config.baseUrl);
      if (!['http:', 'https:'].includes(url.protocol)) {
        throw new Error('Gebruik een HTTP- of HTTPS-adres.');
      }
      if (!config.model.trim()) throw new Error('Vul de model-ID van je runtime in.');

      await saveSettings(
        config.baseUrl,
        {
          system_prompt: brainSettings.system_prompt,
          model_id: config.model.trim(),
          lm_base_url: brainSettings.lm_base_url,
          lm_provider: brainSettings.lm_provider,
          knowledge_top_k: brainSettings.knowledge_top_k,
          knowledge_max_chars: brainSettings.knowledge_max_chars,
        },
        controller.signal,
      );

      await verifyRuntime(config, controller.signal);
      setConnected(true);
      persistRuntime(config);
      await syncFromBrain(config.baseUrl);
      setSettingsOpen(false);
      setSection('Chat');
    } catch (e) {
      setConnected(false);
      setError(
        e instanceof Error && e.name !== 'TypeError'
          ? e.message
          : 'Runtime niet bereikbaar. Controleer Atlas Brain, CORS en je lokale LM.',
      );
    } finally {
      clearTimeout(timeout);
      setConnecting(false);
    }
  }

  function now() {
    return new Date().toISOString();
  }

  async function runCompletion(
    messages: ChatMessage[],
    id: string,
    title: string,
    next: Conversation[],
  ) {
    updateConversations(next);
    setBusy(true);
    setError('');
    abort.current = new AbortController();
    try {
      const answer = await completeChat(config, messages, abort.current.signal, {
        conversationId: id,
        conversationTitle: title,
        persist: true,
      });
      const withAnswer = next.map((c) =>
        c.id === id
          ? {
              ...c,
              messages: [...messages, { role: 'assistant' as const, content: answer, createdAt: now() }],
            }
          : c,
      );
      updateConversations(withAnswer);
      const saved = withAnswer.find((c) => c.id === id);
      if (saved) {
        void upsertConversation(config.baseUrl, {
          id: saved.id,
          title: saved.title,
          messages: saved.messages.map((m) => ({
            role: m.role,
            content: m.content,
            created_at: m.createdAt,
            callout: m.callout,
          })),
          updated_at: now(),
        }).catch(() => undefined);
      }
    } catch (e) {
      if (!(e instanceof Error && e.name === 'AbortError')) {
        setError(e instanceof Error ? e.message : 'Het antwoord kon niet worden opgehaald.');
      }
    } finally {
      setBusy(false);
      abort.current = null;
    }
  }

  async function send() {
    if (busy) return;
    if (!connected) {
      setError('');
      setSettingsOpen(true);
      void loadBrainSettings();
      return;
    }
    if (!draft.trim() && !attachment) return;
    const content = [
      draft.trim(),
      attachment ? `[Bijlage: ${attachment.name}]\n${attachment.text}` : '',
    ]
      .filter(Boolean)
      .join('\n\n');
    const id = active.example ? createId() : active.id;
    const messages: ChatMessage[] = [
      ...(active.example ? [] : active.messages),
      { role: 'user', content, createdAt: now() },
    ];
    const title =
      !active.example && active.messages.length
        ? active.title
        : (draft.trim() || attachment?.name || 'Nieuw gesprek').slice(0, 32);
    const conversation: Conversation = { id, title, messages };
    const next = active.example
      ? [conversation, ...conversations]
      : conversations.map((c) => (c.id === id ? conversation : c));
    setActiveId(id);
    setDraft('');
    setAttachment(null);
    await runCompletion(messages, id, title, next);
  }

  async function retry(index: number) {
    if (busy) return;
    if (!connected) {
      setError('');
      setSettingsOpen(true);
      void loadBrainSettings();
      return;
    }
    if (active.example) {
      setError(
        'Start een nieuw gesprek om echte antwoorden te genereren. Het voorbeeldgesprek blijft behouden.',
      );
      return;
    }
    const messages = active.messages.slice(0, index);
    await runCompletion(
      messages,
      active.id,
      active.title,
      conversations.map((c) => (c.id === active.id ? { ...c, messages } : c)),
    );
  }

  async function attachFile(file?: File) {
    if (!file) return;
    if (file.size > 2 * 1024 * 1024) {
      setError('Kies een tekstbestand van maximaal 2 MB.');
      return;
    }
    if (!/\.(txt|md|json|csv|log|py|java|js|ts|tsx|css|html|rs)$/i.test(file.name)) {
      setError('Dit bestandstype wordt nog niet ondersteund. Kies een tekst- of codebestand.');
      return;
    }
    try {
      setAttachment({ name: file.name, text: await file.text() });
      setError('');
    } catch {
      setError('De bijlage kon niet worden gelezen.');
    }
  }

  async function saveKnowledge() {
    if (!connected) {
      setError('Verbind eerst Atlas Brain.');
      return;
    }
    if (!knowledgeDraft.content.trim()) {
      setError('Kennisinhoud mag niet leeg zijn.');
      return;
    }
    try {
      await learnKnowledge(config.baseUrl, {
        title: knowledgeDraft.title.trim() || 'Zonder titel',
        content: knowledgeDraft.content.trim(),
      });
      setKnowledgeDraft({ title: '', content: '' });
      setError('');
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Kennis opslaan mislukt.');
    }
  }

  const homeVisible = section === 'Chat' && subpage === 'Chat' && !active.messages.length;
  const researchMode = section === 'Data' && subpage === 'Research';

  return (
    <main
      className={`atlas-shell chat-layout ${researchMode ? 'research-layout' : ''} ${sidebarOpen ? 'sidebar-open' : ''}`}
    >
      <div className="atlas-art" aria-hidden="true" />
      <aside className="sidebar" aria-label={researchMode ? 'Onderzoeken' : 'Gesprekken'}>
        <a href="/" className="brand" aria-label="Atlas 999">
          ATLAS <sup>999</sup>
        </a>
        <button
          className="new-chat"
          onClick={researchMode ? newResearch : newChat}
          disabled={busy}
        >
          <Plus />
          <span>{researchMode ? 'Nieuw onderzoek' : 'Nieuw gesprek'}</span>
        </button>
        <div className="conversation-heading">{researchMode ? 'ONDERZOEKEN' : 'GESPREKKEN'}</div>
        <nav className="conversation-list">
          {researchMode ? (
            <>
              {researchTasks.map((task) => (
                <button
                  key={task.id}
                  className={`conversation ${task.id === researchId ? 'selected' : ''}`}
                  onClick={() => {
                    setResearchId(task.id);
                    setSubpage('Research');
                    setSidebarOpen(false);
                  }}
                >
                  <FileText />
                  <span>{task.example ? 'Lokale AI-frameworks' : task.title}</span>
                </button>
              ))}
              <button className="conversation" onClick={newResearch}>
                <MessageSquare />
                <span>Nieuwe onderzoeksvraag</span>
              </button>
            </>
          ) : (
            conversations.map((c) => (
              <button
                key={c.id}
                disabled={busy}
                className={`conversation ${c.id === active.id ? 'selected' : ''}`}
                onClick={() => {
                  setActiveId(c.id);
                  setSection('Chat');
                  setSubpage('Chat');
                  setSidebarOpen(false);
                }}
              >
                <MessageSquare />
                <span>{c.title}</span>
              </button>
            ))
          )}
        </nav>
        <button
          className="sidebar-settings"
          onClick={() => {
            setError('');
            setSettingsOpen(true);
            void loadBrainSettings();
          }}
        >
          <Settings />
          <span>Instellingen</span>
        </button>
      </aside>
      {sidebarOpen && (
        <button className="sidebar-scrim" aria-label="Menu sluiten" onClick={() => setSidebarOpen(false)} />
      )}
      <div className="content-shell">
        <header className="topbar">
          <button
            className="mobile-menu"
            aria-label="Menu openen"
            onClick={() => setSidebarOpen(!sidebarOpen)}
          >
            <Menu />
          </button>
          <nav className="section-tabs" aria-label="Hoofdnavigatie">
            {sections.map((tab) => (
              <button
                key={tab}
                className={section === tab ? 'active' : ''}
                aria-current={section === tab ? 'page' : undefined}
                onClick={() => selectSection(tab)}
              >
                {tab}
              </button>
            ))}
          </nav>
          <div className="topbar-status">
            <span className={`status-dot ${connected ? 'online' : ''}`} />
            <span>Lokaal</span>
            <span className="topbar-divider" />
            <button
              aria-label="Instellingen openen"
              onClick={() => {
                setError('');
                setSettingsOpen(true);
                void loadBrainSettings();
              }}
            >
              <Settings />
            </button>
          </div>
        </header>
        <section className="workspace" aria-label={section}>
          <div className="research-mount" hidden={!researchMode}>
            <ResearchWorkspace
              selectedId={researchId}
              onSelectTask={setResearchId}
              onTasksChange={setResearchTasks}
              newRequest={researchRequest}
              baseUrl={config.baseUrl}
              connected={connected}
            />
          </div>
          {homeVisible && (
            <div className="welcome">
              <h1>Begin met jouw model</h1>
              <p>Selecteer een lokaal model om te starten.</p>
              <ModelLoader
                filename={filename}
                onChoose={() => fileInput.current?.click()}
                onLoad={() => {
                  setError('');
                  setSettingsOpen(true);
                  void loadBrainSettings();
                }}
              />
            </div>
          )}
          {section === 'Chat' && subpage === 'Chat' && !!active.messages.length && (
            <ChatThread conversation={active} busy={busy} onRetry={(index) => void retry(index)} />
          )}
          {((section !== 'Chat' && section !== 'Data') || subpage === 'Dashboard') && (
            <div className="section-placeholder">
              <span className="section-eyebrow">ATLAS 999</span>
              <h1>{subpage === 'Dashboard' ? 'Dashboard' : section}</h1>
              {section === 'Models' ? (
                <>
                  <ModelLoader
                    filename={filename}
                    onChoose={() => fileInput.current?.click()}
                    onLoad={() => {
                      setError('');
                      setSettingsOpen(true);
                      void loadBrainSettings();
                    }}
                  />
                  <div className="knowledge-panel" aria-label="Kennis leren">
                    <h2>Kennis opslaan</h2>
                    <p>Atlas gebruikt opgeslagen kennis bij het redeneren (on-demand recall).</p>
                    <label>
                      Titel
                      <input
                        value={knowledgeDraft.title}
                        onChange={(e) =>
                          setKnowledgeDraft({ ...knowledgeDraft, title: e.target.value })
                        }
                        placeholder="Kort label"
                      />
                    </label>
                    <label>
                      Inhoud
                      <textarea
                        rows={5}
                        value={knowledgeDraft.content}
                        onChange={(e) =>
                          setKnowledgeDraft({ ...knowledgeDraft, content: e.target.value })
                        }
                        placeholder="Feiten, procedures, notities…"
                      />
                    </label>
                    <button className="load-button" type="button" onClick={() => void saveKnowledge()}>
                      Opslaan in brein <ArrowRight />
                    </button>
                  </div>
                </>
              ) : (
                <p>Deze werkruimte staat klaar voor de volgende stap.</p>
              )}
              <button
                className="back-link"
                onClick={() => {
                  setSection('Chat');
                  setSubpage('Chat');
                }}
              >
                Terug naar Chat <ArrowRight />
              </button>
            </div>
          )}
          {section === 'Chat' && subpage === 'Chat' && (
            <div className="composer-area">
              <div className="model-status" role="status">
                <span className={`status-dot ${connected ? 'online' : ''}`} />
                {connected
                  ? `Atlas Brain · ${config.model}`
                  : active.example
                    ? 'Voorbeeldgesprek · geen model verbonden'
                    : filename
                      ? `${filename} geselecteerd · Brain nog niet verbonden`
                      : 'Nog geen model geladen'}
              </div>
              {error && !settingsOpen && (
                <p className="inline-error" role="alert">
                  {error}
                </p>
              )}
              {attachment && (
                <div className="attachment-chip">
                  <Paperclip />
                  <span>{attachment.name}</span>
                  <button onClick={() => setAttachment(null)} aria-label="Bijlage verwijderen">
                    <X />
                  </button>
                </div>
              )}
              <form
                className="composer"
                onSubmit={(e) => {
                  e.preventDefault();
                  void send();
                }}
              >
                <button
                  type="button"
                  className="attach-button"
                  aria-label="Bestand toevoegen"
                  title="Tekstbestand toevoegen"
                  onClick={() => attachmentInput.current?.click()}
                >
                  <Paperclip />
                </button>
                <span className="composer-divider" aria-hidden="true" />
                <textarea
                  rows={1}
                  aria-label="Bericht aan Atlas"
                  placeholder="Stel Atlas een vraag..."
                  value={draft}
                  onChange={(e) => setDraft(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
                      e.preventDefault();
                      void send();
                    }
                  }}
                />
                {busy ? (
                  <button
                    type="button"
                    className="send-button"
                    onClick={() => abort.current?.abort()}
                    aria-label="Antwoord stoppen"
                  >
                    <Square />
                  </button>
                ) : (
                  <button
                    className={`send-button ${!connected ? 'connect-runtime' : ''}`}
                    disabled={connected && !draft.trim() && !attachment}
                    aria-label={connected ? 'Bericht verzenden' : 'Lokale runtime verbinden'}
                    title={connected ? 'Verzenden' : 'Verbind eerst Atlas Brain'}
                  >
                    <Send />
                  </button>
                )}
              </form>
            </div>
          )}
        </section>
        <nav className="bottom-nav" aria-label="Submenu">
          <button
            className={subpage === 'Dashboard' ? 'active dashboard-tab' : 'dashboard-tab'}
            aria-current={subpage === 'Dashboard' ? 'page' : undefined}
            onClick={() => setSubpage('Dashboard')}
          >
            <Grid2X2 />
            <span>Dashboard</span>
          </button>
          {section === 'Data' ? (
            <button
              className={subpage === 'Research' ? 'active chat-tab' : 'chat-tab'}
              aria-current={subpage === 'Research' ? 'page' : undefined}
              onClick={() => setSubpage('Research')}
            >
              <FileText />
              <span>Research</span>
            </button>
          ) : (
            <button
              className={subpage === 'Chat' ? 'active chat-tab' : 'chat-tab'}
              aria-current={subpage === 'Chat' ? 'page' : undefined}
              onClick={() => {
                setSubpage('Chat');
                setSection('Chat');
              }}
            >
              <MessageSquare />
              <span>Chat</span>
            </button>
          )}
        </nav>
      </div>
      <input
        ref={fileInput}
        className="sr-only"
        type="file"
        accept=".gguf"
        aria-label="Selecteer een GGUF-modelbestand"
        onChange={(e) => {
          const file = e.target.files?.[0];
          if (file) {
            if (!file.name.toLowerCase().endsWith('.gguf')) {
              setError('Selecteer een .gguf-modelbestand.');
              return;
            }
            setFilename(file.name);
          }
        }}
      />
      <input
        ref={attachmentInput}
        className="sr-only"
        type="file"
        accept=".txt,.md,.json,.csv,.log,.py,.java,.js,.ts,.tsx,.css,.html,.rs"
        aria-label="Tekstbestand toevoegen"
        onChange={(e) => {
          void attachFile(e.target.files?.[0]);
          e.currentTarget.value = '';
        }}
      />
      <dialog
        className="runtime-dialog"
        ref={dialog}
        onCancel={() => setSettingsOpen(false)}
        onClick={(e) => {
          if (e.target === e.currentTarget) setSettingsOpen(false);
        }}
      >
        <div className="dialog-header">
          <div>
            <span className="section-eyebrow">ATLAS 999</span>
            <h2>Atlas Brain</h2>
          </div>
          <button aria-label="Instellingen sluiten" onClick={() => setSettingsOpen(false)}>
            <X />
          </button>
        </div>
        <p>
          Koppel de UI aan Atlas Brain. Brain praat uitsluitend met een lokale LM-client — geen
          cloud-API. Research, chat en memory delen dezelfde brain.
        </p>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void connect();
          }}
        >
          <label>
            Brain API-adres
            <input
              type="url"
              required
              value={config.baseUrl}
              onChange={(e) => {
                persistRuntime({ ...config, baseUrl: e.target.value });
                setConnected(false);
              }}
              placeholder="http://127.0.0.1:8080/v1"
            />
          </label>
          <label>
            Model-ID
            <input
              required
              value={config.model}
              onChange={(e) => {
                persistRuntime({ ...config, model: e.target.value });
                setBrainSettings({ ...brainSettings, model_id: e.target.value });
                setConnected(false);
              }}
              placeholder="Lokale model-ID"
            />
          </label>
          <label>
            Lokale LM-basis-URL
            <input
              type="url"
              required
              value={brainSettings.lm_base_url}
              onChange={(e) => {
                setBrainSettings({ ...brainSettings, lm_base_url: e.target.value });
                setConnected(false);
              }}
              placeholder="http://127.0.0.1:11434/v1"
            />
          </label>
          <label>
            LM-provider
            <select
              value={brainSettings.lm_provider}
              onChange={(e) => {
                setBrainSettings({ ...brainSettings, lm_provider: e.target.value });
                setConnected(false);
              }}
            >
              <option value="local_http">local_http</option>
              <option value="stub">stub (pipeline-test)</option>
            </select>
          </label>
          <label>
            SYSTEM_PROMPT
            <textarea
              rows={6}
              value={brainSettings.system_prompt}
              onChange={(e) => setBrainSettings({ ...brainSettings, system_prompt: e.target.value })}
              placeholder="Volledig aanpasbaar — leeg = geen system prompt"
            />
          </label>
          {filename && <p className="selected-file">Geselecteerd bestand: {filename}</p>}
          {error && (
            <p className="inline-error" role="alert">
              {error}
            </p>
          )}
          <button className="load-button" disabled={connecting}>
            {connecting ? 'Verbinding controleren…' : 'Verbinden'}
            <ArrowRight />
          </button>
        </form>
      </dialog>
    </main>
  );
}
