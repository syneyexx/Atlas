import type { ChatMessage } from './runtime';

export type Conversation = {
  id: string;
  title: string;
  messages: ChatMessage[];
  example?: boolean;
};
export const exampleConversation: Conversation = {
  id: 'atlas-design-example',
  title: 'Atlas leren kennen',
  example: true,
  messages: [
    { role: 'user', content: 'Ik wil een nieuw project beginnen. Waar starten we?', createdAt: '2026-10-22T14:28:00+02:00' },
    { role: 'assistant', content: 'We beginnen bij één helder doel.\n\nVertel me wat je wilt maken en wat het als eerste moet kunnen.', createdAt: '2026-10-22T14:28:00+02:00', callout: { title: 'Eerste stap', content: 'Beschrijf je idee in één of twee zinnen.' } },
    { role: 'user', content: 'Een eigen AI-framework, helemaal lokaal.', createdAt: '2026-10-22T14:29:00+02:00' },
  ],
};
export const defaultConversations: Conversation[] = [exampleConversation,
  { id: 'atlas-new-project', title: 'Een nieuw project', messages: [] },
  { id: 'atlas-ideas', title: 'Ideeën uitwerken', messages: [] },
];
export function isConversation(value: unknown): value is Conversation {
  if (!value || typeof value !== 'object') return false;
  const item = value as Partial<Conversation>;
  return typeof item.id === 'string' && typeof item.title === 'string' && Array.isArray(item.messages) && item.messages.every(message => message && ['user','assistant'].includes(message.role) && typeof message.content === 'string' && (!message.createdAt || typeof message.createdAt === 'string') && (!message.callout || (typeof message.callout.title === 'string' && typeof message.callout.content === 'string')));
}
export function messageText(message: ChatMessage) {
  return message.callout ? `${message.content}\n\n${message.callout.title}\n${message.callout.content}` : message.content;
}
