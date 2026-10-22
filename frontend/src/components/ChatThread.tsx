'use client';
import { useEffect, useRef, useState } from 'react';
import { Check, Copy, RotateCw, Target, ThumbsUp } from 'lucide-react';
import type { ChatMessage } from '../lib/runtime';
import { messageText, type Conversation } from '../lib/conversations';
import { AtlasMark } from './AtlasMark';

function timestamp(value?: string) {
  if (!value || !Number.isFinite(Date.parse(value))) return '';
  return new Intl.DateTimeFormat('nl-NL', { hour: '2-digit', minute: '2-digit', timeZone: 'Europe/Amsterdam' }).format(new Date(value));
}
function ChatBubble({message, index, retry, canRetry, busy}: {message: ChatMessage; index: number; retry: () => void; canRetry: boolean; busy: boolean}) {
  const [copied, setCopied] = useState(false);
  const [liked, setLiked] = useState(false);
  const [copyError, setCopyError] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => {if (timer.current) clearTimeout(timer.current);}, []);
  async function copy() {
    try {await navigator.clipboard.writeText(messageText(message)); setCopied(true); setCopyError(false); if (timer.current) clearTimeout(timer.current); timer.current = setTimeout(() => setCopied(false), 2000);}
    catch {setCopyError(true);}
  }
  if (message.role === 'user') return <article className="chat-bubble user-bubble"><div className="bubble-meta"><span>JIJ</span><time dateTime={message.createdAt}>{timestamp(message.createdAt)}</time></div><p>{message.content}</p></article>;
  return <article className="chat-bubble atlas-bubble"><AtlasMark/><div className="bubble-meta assistant-meta"><span>ATLAS <b>999</b></span><time dateTime={message.createdAt}>{timestamp(message.createdAt)}</time></div><div className="assistant-content">{message.content.split(/\n\n+/).map((paragraph, i) => <p key={i}>{paragraph}</p>)}{message.callout && <section className="message-callout"><Target aria-hidden="true"/><div><h3>{message.callout.title}</h3><p>{message.callout.content}</p></div></section>}</div><div className="message-actions" aria-label={`Acties voor antwoord ${index + 1}`}><button onClick={() => void copy()} aria-label={copied ? 'Antwoord gekopieerd' : 'Antwoord kopiëren'} title={copied ? 'Gekopieerd' : 'Kopiëren'}>{copied ? <Check/> : <Copy/>}</button><button onClick={retry} disabled={busy || !canRetry} aria-label="Antwoord opnieuw genereren" title={canRetry ? 'Opnieuw genereren' : 'Dit antwoord kan niet opnieuw worden gegenereerd'}><RotateCw/></button><button onClick={() => setLiked(!liked)} aria-label="Antwoord waarderen" aria-pressed={liked} title={liked ? 'Waardering verwijderen' : 'Goed antwoord'} className={liked ? 'is-liked' : ''}><ThumbsUp/></button><span className="action-feedback" role="status">{copied ? 'Gekopieerd' : copyError ? 'Kopiëren niet beschikbaar' : liked ? 'Gewaardeerd' : ''}</span></div></article>;
}
export function ChatThread({conversation, busy, onRetry}: {conversation: Conversation; busy: boolean; onRetry: (index: number) => void}) {
  const feed = useRef<HTMLDivElement>(null);
  const previous = useRef({id: conversation.id, length: conversation.messages.length});
  useEffect(() => {
    if (!feed.current) return;
    const changedConversation = previous.current.id !== conversation.id;
    if (changedConversation) feed.current.scrollTop = conversation.example ? 0 : feed.current.scrollHeight;
    else if (conversation.messages.length > previous.current.length || busy) feed.current.scrollTo({top: feed.current.scrollHeight, behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'instant' : 'smooth'});
    previous.current = {id: conversation.id, length: conversation.messages.length};
  }, [conversation.id, conversation.messages.length, conversation.example, busy]);
  return <div ref={feed} className={`chat-thread ${conversation.example ? 'example-thread' : ''}`} role="log" aria-label="Chatberichten" aria-live={conversation.example ? 'off' : 'polite'} aria-busy={busy}><div className="chat-thread-inner">{conversation.messages.map((message, index) => <ChatBubble key={`${conversation.id}-${index}-${message.createdAt || ''}`} message={message} index={index} retry={() => onRetry(index)} canRetry={index > 0 && conversation.messages[index - 1].role === 'user'} busy={busy}/>)}{busy && <div className="responding" role="status"><span/>Atlas antwoordt…</div>}</div></div>;
}
