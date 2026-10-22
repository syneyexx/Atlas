'use client';

import { createId } from '../lib/id';
import { useCallback, useEffect, useRef, useState } from 'react';
import {
  ArrowRight,
  Check,
  ChevronDown,
  ChevronRight,
  Copy,
  Expand,
  ExternalLink,
  FileText,
  Layers,
  List,
  Minimize,
  Paperclip,
  Pause,
  Play,
  Target,
  Terminal,
  X,
  ChartNoAxesCombined,
} from 'lucide-react';
import {
  clarifyResearchJob,
  createResearchJob,
  exampleResearch,
  fetchResearchJob,
  isResearchTask,
  listResearchJobs,
  pauseResearchJob,
  phaseRail,
  researchTime,
  tickResearchJob,
  type Clarifier,
  type ResearchEvent,
  type ResearchTask,
} from '../lib/research';

type Props = {
  selectedId: string;
  onTasksChange: (tasks: ResearchTask[]) => void;
  newRequest: number;
  onSelectTask: (id: string) => void;
  baseUrl: string;
  connected: boolean;
};

const PHASES = ['Verkennen', 'Vergelijken', 'Toetsen', 'Synthese'] as const;

export function ResearchWorkspace({
  selectedId,
  onTasksChange,
  newRequest,
  onSelectTask,
  baseUrl,
  connected,
}: Props) {
  const [tasks, setTasks] = useState<ResearchTask[]>([exampleResearch]);
  const [prompt, setPrompt] = useState('');
  const [mode, setMode] = useState('Verdiepend');
  const [file, setFile] = useState<File | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const [frozen, setFrozen] = useState(false);
  const [frozenEvents, setFrozenEvents] = useState<ResearchEvent[] | null>(null);
  const [expanded, setExpanded] = useState(false);
  const [modal, setModal] = useState<{ title: string; content: string } | null>(null);
  const [copyFeedback, setCopyFeedback] = useState('');
  const [ready, setReady] = useState(false);
  const [clarifierDraft, setClarifierDraft] = useState<Record<string, string>>({});
  const input = useRef<HTMLTextAreaElement>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const feed = useRef<HTMLDivElement>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const fullDialog = useRef<HTMLDialogElement>(null);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pollTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [currentId, setCurrentId] = useState(selectedId);
  const active = tasks.find((t) => t.id === currentId) || exampleResearch;

  const upsertTask = useCallback((task: ResearchTask) => {
    setTasks((previous) => {
      const without = previous.filter((t) => t.id !== task.id && !(!task.example && t.id === task.id));
      const examples = without.filter((t) => t.example);
      const rest = without.filter((t) => !t.example && t.id !== task.id);
      return task.example ? [task, ...rest] : [...examples, task, ...rest.filter((t) => t.id !== task.id)];
    });
  }, []);

  useEffect(() => {
    try {
      const saved = JSON.parse(localStorage.getItem('atlas999.research') || '[]') as unknown;
      if (Array.isArray(saved) && saved.every(isResearchTask)) {
        setTasks([exampleResearch, ...saved.filter((t) => !t.example)]);
      }
    } catch {
      /* optional */
    }
    setReady(true);
  }, []);

  useEffect(() => {
    onTasksChange(tasks);
    if (ready) {
      try {
        localStorage.setItem(
          'atlas999.research',
          JSON.stringify(tasks.filter((t) => !t.example)),
        );
      } catch {
        /* memory-only */
      }
    }
  }, [tasks, ready, onTasksChange]);

  useEffect(() => {
    setCurrentId(selectedId);
    setFrozen(false);
    setFrozenEvents(null);
  }, [selectedId]);

  // Hydrate full job (events/sources/report) when selecting a listed remote job.
  useEffect(() => {
    if (!connected || !baseUrl || selectedId === exampleResearch.id) return;
    let cancelled = false;
    void fetchResearchJob(baseUrl, selectedId)
      .then((task) => {
        if (!cancelled) upsertTask(task);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [selectedId, connected, baseUrl, upsertTask]);

  useEffect(() => {
    if (newRequest) {
      setPrompt('');
      setFile(null);
      setError('');
      input.current?.focus();
    }
  }, [newRequest]);

  useEffect(() => {
    if (modal) dialog.current?.showModal();
    else dialog.current?.close();
  }, [modal]);

  useEffect(() => {
    if (expanded) fullDialog.current?.showModal();
    else fullDialog.current?.close();
  }, [expanded]);

  useEffect(() => {
    if (autoScroll && !frozen && feed.current && !active.example) {
      feed.current.scrollTop = feed.current.scrollHeight;
    }
  }, [active.events, autoScroll, frozen, active.example]);

  useEffect(
    () => () => {
      if (copyTimer.current) clearTimeout(copyTimer.current);
      if (pollTimer.current) clearTimeout(pollTimer.current);
    },
    [],
  );

  // Sync remote jobs when Brain connects (request-driven list, no health loop).
  useEffect(() => {
    if (!connected || !baseUrl) return;
    let cancelled = false;
    void (async () => {
      try {
        const remote = await listResearchJobs(baseUrl);
        if (cancelled || !remote.length) return;
        setTasks((previous) => {
          const examples = previous.filter((t) => t.example);
          const locals = previous.filter(
            (t) => !t.example && !remote.some((r) => r.id === t.id),
          );
          return [...examples, ...remote, ...locals];
        });
      } catch {
        /* offline list is optional */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [connected, baseUrl]);

  // Continue running jobs via request-driven ticks while the page is open.
  useEffect(() => {
    if (!connected || !baseUrl || active.example || active.paused) return;
    const running = active.status === 'running' && active.phase !== 'done';
    if (!running) return;
    if (pollTimer.current) clearTimeout(pollTimer.current);
    pollTimer.current = setTimeout(() => {
      void tickResearchJob(baseUrl, active.id, 2)
        .then((task) => {
          upsertTask(task);
        })
        .catch(() => undefined);
    }, 1200);
    return () => {
      if (pollTimer.current) clearTimeout(pollTimer.current);
    };
  }, [
    connected,
    baseUrl,
    active.id,
    active.status,
    active.phase,
    active.paused,
    active.example,
    active.events.length,
    upsertTask,
  ]);

  async function start() {
    if (!prompt.trim() && !file) {
      setError('Geef een onderwerp op of voeg een bestand toe.');
      input.current?.focus();
      return;
    }
    const title = prompt.trim() || `Onderzoek: ${file!.name}`;
    setError('');
    setBusy(true);

    if (!connected) {
      const task: ResearchTask = {
        id: createId(),
        title,
        mode,
        status: 'queued',
        phase: 'intake',
        attachment: file
          ? { name: file.name, type: file.type, size: file.size }
          : undefined,
        events: [
          {
            id: createId(),
            timestamp: researchTime(),
            type: 'QUEUE',
            lines: ['Opdracht lokaal vastgelegd', `modus: ${mode}`],
            details: `Onderzoeksopdracht:\n${title}\n\n${
              file
                ? `Bijlage: ${file.name} (${file.size} bytes).`
                : 'Geen bijlage.'
            }`,
          },
          {
            id: createId(),
            timestamp: researchTime(),
            type: 'WAIT',
            lines: ['Geen Brain verbonden', 'Verbind Atlas Brain om onderzoek te starten'],
          },
        ],
      };
      setTasks((previous) => [task, ...previous]);
      setCurrentId(task.id);
      onSelectTask(task.id);
      setFile(null);
      setPrompt('');
      setBusy(false);
      return;
    }

    try {
      let attachment:
        | { name: string; type: string; size: number; text?: string }
        | undefined;
      if (file) {
        const text = file.size <= 2 * 1024 * 1024 ? await file.text() : undefined;
        attachment = { name: file.name, type: file.type, size: file.size, text };
      }
      const task = await createResearchJob(baseUrl, {
        question: prompt.trim() || title,
        mode,
        attachment,
        auto_run: true,
      });
      upsertTask(task);
      setCurrentId(task.id);
      onSelectTask(task.id);
      setFile(null);
      setPrompt('');
      if (task.clarifications?.length) {
        const draft: Record<string, string> = {};
        for (const c of task.clarifications) draft[c.id] = c.answer || '';
        setClarifierDraft(draft);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Research starten mislukt.');
    } finally {
      setBusy(false);
    }
  }

  async function pause() {
    if (active.example) {
      setTasks((previous) =>
        previous.map((task) =>
          task.id === active.id
            ? {
                ...task,
                paused: !task.paused,
                events: [
                  ...task.events,
                  {
                    id: createId(),
                    timestamp: researchTime(),
                    type: task.paused ? 'WAIT' : 'PAUSE',
                    lines: [task.paused ? 'Voorbeeld hervat' : 'Voorbeeld gepauzeerd'],
                  },
                ],
              }
            : task,
        ),
      );
      return;
    }
    if (!connected) {
      setError('Verbind Atlas Brain om te pauzeren/hervatten.');
      return;
    }
    try {
      const task = await pauseResearchJob(baseUrl, active.id, !active.paused);
      upsertTask(task);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Pauze mislukt.');
    }
  }

  async function submitClarifiers() {
    if (!connected || active.example) return;
    const answers: Clarifier[] = (active.clarifications || []).map((c) => ({
      ...c,
      answer: clarifierDraft[c.id] || '',
    }));
    setBusy(true);
    try {
      const task = await clarifyResearchJob(baseUrl, active.id, answers);
      upsertTask(task);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Verduidelijking mislukt.');
    } finally {
      setBusy(false);
    }
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(
        active.events.map((e) => `${e.timestamp} ${e.type}\n${e.lines.join('\n')}`).join('\n\n'),
      );
      setCopyFeedback('Gekopieerd');
    } catch {
      setCopyFeedback('Kopiëren niet beschikbaar');
    }
    if (copyTimer.current) clearTimeout(copyTimer.current);
    copyTimer.current = setTimeout(() => setCopyFeedback(''), 2500);
  }

  function pickFile(selected?: File) {
    if (!selected) return;
    if (selected.size > 20 * 1024 * 1024) {
      setError('Kies een bestand van maximaal 20 MB.');
      return;
    }
    setFile(selected);
    setError('');
  }

  function showSources() {
    if (active.example) {
      setModal({
        title: 'Bronnen',
        content:
          'VOORBEELDWEERGAVE\n\nDe getallen in dit ontwerp zijn voorbeeldgegevens. Er zijn in deze interface nog geen 12 bronnen opgehaald of gecontroleerd.\n\nDe onderzoeksbackend levert straks de echte bron-URL, titel, ophaaltijd en relevante passages.',
      });
      return;
    }
    const sources = active.sources || [];
    setModal({
      title: 'Bronnen',
      content: sources.length
        ? sources
            .map(
              (s, i) =>
                `${i + 1}. [${s.quality_tier}] ${s.title}\n${s.url}\n${s.fetched_at}\n${s.fragment.slice(0, 400)}\n`,
            )
            .join('\n')
        : 'Er zijn nog geen bronnen opgehaald.',
    });
  }

  function showFindings() {
    if (active.example) {
      setModal({
        title: 'Bevindingen',
        content:
          'VOORBEELDWEERGAVE\n\nArchitecturen vergeleken\nBronnen gecontroleerd\nTegenstrijdigheden toetsen\n\nDeze regels illustreren de goedgekeurde layout.',
      });
      return;
    }
    if (active.reportMd) {
      setModal({ title: 'Rapport', content: active.reportMd });
      return;
    }
    const findings = active.findings || [];
    setModal({
      title: 'Bevindingen',
      content: findings.length
        ? findings.map((f, i) => `${i + 1}. ${f.summary || f.title || ''}`).join('\n\n')
        : 'Er zijn nog geen bevindingen.',
    });
  }

  function toggleFreeze() {
    setFrozenEvents(frozen ? null : active.events);
    setFrozen(!frozen);
  }

  const rail = active.example
    ? { complete: 2, current: 2 }
    : phaseRail(active.phase);
  const sourceCount = active.example ? 12 : active.sources?.length || 0;
  const findingCount = active.example
    ? 6
    : active.findings?.length || active.claims?.length || 0;
  const openItems = active.example
    ? ['Geheugen versus modeltraining', 'Betrouwbaarheid van toolcalling', 'Grenzen van lokale hardware']
    : active.openQuestions?.length
      ? active.openQuestions
      : active.status === 'blocked'
        ? ['Verduidelijkende vragen beantwoorden']
        : active.status === 'completed' || active.phase === 'done'
          ? ['Geen open vragen — rapport en bronnen gereed']
          : ['Onderzoeksvragen formuleren', 'Bronnen verzamelen en toetsen', 'Conclusies en hiaten vastleggen'];

  const statusLabel = active.paused
    ? 'Gepauzeerd'
    : active.example
      ? 'Bezig'
      : active.status === 'blocked'
        ? 'Wacht op input'
        : active.status === 'completed' || active.phase === 'done'
          ? 'Afgerond'
          : connected
            ? active.phase || 'Bezig'
            : 'Wacht op Brain';

  function terminalContent(full = false) {
    return (
      <>
        <div className="terminal-header">
          <div className="terminal-symbol">
            <Terminal />
          </div>
          <div className="terminal-title">
            <h2>Uitvoeringsterminal</h2>
            <span className="terminal-badge">
              {active.example ? 'VOORBEELDLOG' : connected ? 'ATLAS BRAIN' : 'LOKALE WACHTRIJ'}
            </span>
          </div>
          <span className="terminal-live">
            <i />
            {frozen ? 'VAST' : active.example ? 'LIVE' : active.status === 'running' ? 'LIVE' : 'WACHT'}
          </span>
          <div className="terminal-controls">
            <button
              onClick={toggleFreeze}
              aria-label={frozen ? 'Terminalweergave hervatten' : 'Terminalweergave vastzetten'}
              title="Weergave vastzetten"
            >
              {frozen ? <Play /> : <Pause />}
            </button>
            <button onClick={() => void copy()} aria-label="Terminallog kopiëren" title="Log kopiëren">
              <Copy />
            </button>
            <button
              onClick={() => setExpanded(!expanded)}
              aria-label={full ? 'Terminal verkleinen' : 'Terminal vergroten'}
              title={full ? 'Verkleinen' : 'Vergroten'}
            >
              {full ? <Minimize /> : <Expand />}
            </button>
          </div>
        </div>
        <div
          className="terminal-feed"
          ref={full ? undefined : feed}
          role="log"
          aria-live={frozen ? 'off' : 'polite'}
          aria-label="Uitvoeringsgebeurtenissen"
        >
          {(frozen && frozenEvents ? frozenEvents : active.events).map((event) => (
            <article key={event.id} className="terminal-event">
              <time>{event.timestamp}</time>
              <div className="terminal-event-body">
                <strong>{event.type}</strong>
                {event.lines.map((line, index) => (
                  <div key={index}>{line}</div>
                ))}
              </div>
              {event.details && (
                <button
                  className="terminal-details"
                  onClick={() =>
                    setModal({ title: `${event.type} · ${event.timestamp}`, content: event.details! })
                  }
                >
                  Details <ChevronRight />
                </button>
              )}
            </article>
          ))}
        </div>
        <footer className="terminal-footer">
          <span>
            <List />
            {(frozen && frozenEvents ? frozenEvents : active.events).length} gebeurtenissen
          </span>
          <span className="terminal-copy-feedback" role="status">
            {copyFeedback}
          </span>
          <button
            className="autoscroll-toggle"
            role="switch"
            aria-checked={autoScroll}
            onClick={() => setAutoScroll(!autoScroll)}
          >
            <i className={autoScroll ? 'on' : ''} />
            <span>Auto-scroll {autoScroll ? 'aan' : 'uit'}</span>
          </button>
        </footer>
      </>
    );
  }

  return (
    <div className="research-page">
      <div className="research-left">
        <header className="research-heading">
          <h1>Research</h1>
          <p>Van vraag naar onderbouwde kennis.</p>
        </header>
        <form
          className="research-command"
          onSubmit={(e) => {
            e.preventDefault();
            void start();
          }}
        >
          <div className="research-prompt-box">
            <textarea
              ref={input}
              aria-label="Onderzoeksopdracht"
              placeholder="Wat wil je onderzoeken?"
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              disabled={busy}
            />
            <p>Een onderwerp, stelling, afbeelding of bestand.</p>
          </div>
          <div className="research-command-actions">
            <button
              type="button"
              className="research-secondary"
              onClick={() => fileInput.current?.click()}
              disabled={busy}
            >
              <Paperclip />
              Bijlage
            </button>
            <div className="research-mode">
              <Layers />
              <select
                aria-label="Onderzoeksmodus"
                value={mode}
                onChange={(e) => setMode(e.target.value)}
                disabled={busy}
              >
                <option>Verdiepend</option>
                <option>Verkennend</option>
              </select>
              <ChevronDown />
            </div>
            <button className="research-start" disabled={busy}>
              {busy ? 'Bezig…' : 'Start onderzoek'} <ArrowRight />
            </button>
          </div>
          {file && (
            <div className="research-file-chip">
              <Paperclip />
              <span>{file.name}</span>
              <button type="button" aria-label="Onderzoeksbijlage verwijderen" onClick={() => setFile(null)}>
                <X />
              </button>
            </div>
          )}
          {error && (
            <p className="inline-error" role="alert">
              {error}
            </p>
          )}
        </form>

        {active.status === 'blocked' && !!active.clarifications?.length && (
          <section className="research-active" aria-label="Verduidelijking">
            <div className="research-active-label">
              <span>INTAKE</span>
              <b>
                <i />
                Verduidelijking
              </b>
            </div>
            <h2>Nog even scherpstellen</h2>
            <div className="research-columns" style={{ gridTemplateColumns: '1fr' }}>
              {(active.clarifications || []).map((c) => (
                <label key={c.id} style={{ display: 'grid', gap: 8, marginBottom: 12, fontSize: 13 }}>
                  {c.question}
                  <input
                    value={clarifierDraft[c.id] || ''}
                    onChange={(e) =>
                      setClarifierDraft((d) => ({ ...d, [c.id]: e.target.value }))
                    }
                    style={{
                      height: 40,
                      border: '1px solid #665544',
                      borderRadius: 7,
                      background: '#111',
                      color: '#f2f0eb',
                      padding: '0 12px',
                    }}
                  />
                </label>
              ))}
              <button className="research-start" type="button" disabled={busy} onClick={() => void submitClarifiers()}>
                Doorgaan <ArrowRight />
              </button>
            </div>
          </section>
        )}

        <section className="research-active">
          <div className="research-active-label">
            <span>ACTIEF ONDERZOEK</span>
            <b>
              <i />
              {statusLabel}
            </b>
          </div>
          <h2>
            {active.example ? (
              <>
                Hoe bouw je een betrouwbaar
                <br />
                lokaal AI-framework?
              </>
            ) : (
              active.title
            )}
          </h2>
          <div className={`research-phases ${active.example ? 'sample-phases' : ''}`}>
            {PHASES.map((phase, index) => {
              const complete = index < rail.complete;
              const current = index === rail.current;
              return (
                <div
                  key={phase}
                  className={`research-phase ${complete ? 'complete' : ''} ${current ? 'current' : ''}`}
                >
                  <span>{complete ? <Check /> : null}</span>
                  <small>{phase}</small>
                </div>
              );
            })}
          </div>
          <div className="research-columns">
            <section className="research-timeline">
              <h3>Onderzoekslijn</h3>
              {active.example ? (
                <>
                  <div className="research-step complete">
                    <i>
                      <Check />
                    </i>
                    <div>
                      <h4>
                        Architecturen vergeleken <time>10:12</time>
                      </h4>
                      <p>Verschillende lokale AI-frameworks en aanpakken in kaart gebracht.</p>
                    </div>
                  </div>
                  <div className="research-step complete">
                    <i>
                      <Check />
                    </i>
                    <div>
                      <h4>
                        Bronnen gecontroleerd <time>11:47</time>
                      </h4>
                      <p>Belangrijke claims getoetst aan betrouwbare bronnen.</p>
                    </div>
                  </div>
                  <div className="research-step current">
                    <i />
                    <div>
                      <h4>
                        Tegenstrijdigheden toetsen <time>14:28</time>
                      </h4>
                      <p>Controleert claims over geheugen en modeluitvoering.</p>
                    </div>
                  </div>
                </>
              ) : (
                <>
                  <div className="research-step complete">
                    <i>
                      <Check />
                    </i>
                    <div>
                      <h4>Opdracht vastgelegd</h4>
                      <p>Job in Atlas Brain · request-driven orchestrator.</p>
                    </div>
                  </div>
                  {(active.findings || []).slice(0, 3).map((f, i) => (
                    <div
                      key={f.source_id || i}
                      className={`research-step ${i === (active.findings?.length || 1) - 1 && active.phase !== 'done' ? 'current' : 'complete'}`}
                    >
                      <i>{i < (active.findings?.length || 0) - 1 || active.phase === 'done' ? <Check /> : null}</i>
                      <div>
                        <h4>{f.title || f.subquestion || 'Bevinding'}</h4>
                        <p>{(f.summary || '').slice(0, 120)}</p>
                      </div>
                    </div>
                  ))}
                  {!active.findings?.length && (
                    <div className="research-step current">
                      <i />
                      <div>
                        <h4>
                          {active.paused
                            ? 'Opdracht gepauzeerd'
                            : active.status === 'blocked'
                              ? 'Wacht op verduidelijking'
                              : connected
                                ? `Fase · ${active.phase || 'intake'}`
                                : 'Wacht op Atlas Brain'}
                        </h4>
                        <p>
                          {connected
                            ? 'Lead-agent en subagents werken via ticks — GUI blijft licht.'
                            : 'Verbind Brain om bronnen op te halen en te synthetiseren.'}
                        </p>
                      </div>
                    </div>
                  )}
                </>
              )}
            </section>
            <section className="research-open">
              <h3>Nog te onderzoeken</h3>
              <ul>
                {openItems.map((item) => (
                  <li key={item}>
                    <i />
                    {item}
                  </li>
                ))}
              </ul>
              <div className="research-done">
                <Target />
                <div>
                  <h4>Klaar wanneer</h4>
                  <p>Kernvragen beantwoord, bronnen getoetst en hiaten benoemd.</p>
                </div>
              </div>
            </section>
          </div>
          <footer className="research-active-footer">
            <button onClick={showSources}>
              <FileText />
              Bronnen · {sourceCount}
              <ChevronRight />
            </button>
            <button onClick={showFindings}>
              <ChartNoAxesCombined />
              Bevindingen · {findingCount}
              <ChevronRight />
            </button>
            <button onClick={() => void pause()}>
              {active.paused ? <Play /> : <Pause />}
              {active.paused ? 'Hervatten' : 'Pauzeren'}
            </button>
            <button
              onClick={() =>
                setModal({
                  title: active.title,
                  content: `${active.example ? 'VOORBEELDONDERZOEK' : 'ATLAS RESEARCH'}\n\nModus: ${active.mode}\nStatus: ${active.status || '—'}\nFase: ${active.phase || '—'}\n${
                    active.attachment ? `Bijlage: ${active.attachment.name}\n` : ''
                  }\n${active.reportMd || 'Nog geen rapport.'}`,
                })
              }
            >
              <ExternalLink />
              Onderzoek openen
            </button>
          </footer>
        </section>
      </div>
      <aside className="research-terminal">{terminalContent()}</aside>
      <input
        ref={fileInput}
        className="sr-only"
        type="file"
        aria-label="Onderzoeksbestand toevoegen"
        onChange={(e) => {
          pickFile(e.target.files?.[0]);
          e.currentTarget.value = '';
        }}
      />
      <dialog ref={fullDialog} className="terminal-expanded" onCancel={() => setExpanded(false)}>
        {terminalContent(true)}
        <button className="expanded-close" aria-label="Vergrote terminal sluiten" onClick={() => setExpanded(false)}>
          <X />
        </button>
      </dialog>
      <dialog
        ref={dialog}
        className="runtime-dialog research-detail-dialog"
        onCancel={() => setModal(null)}
      >
        <div className="dialog-header">
          <h2>{modal?.title}</h2>
          <button aria-label="Onderzoeksdetails sluiten" onClick={() => setModal(null)}>
            <X />
          </button>
        </div>
        <pre>{modal?.content}</pre>
      </dialog>
    </div>
  );
}
