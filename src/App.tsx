import { useCallback, useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';
import ReactMarkdown, { defaultUrlTransform } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { backend, nativeApp } from './backend';
import { Dialog, NameDialog, SearchDialog } from './dialogs';
import { Icon, ProviderIcon } from './icons';
import { modelLabel, providerName, routeLabel, SettingsView } from './SettingsView';
import { ModelPicker, ProviderPicker } from './ModelPickers';
import { discoverModels, invalidateModelCatalog } from './model-catalog';
import type { Bootstrap, Chat, Folder, Message, ProviderView, Settings, Source, StreamUpdate } from './types';

const errorText = (reason: unknown) => reason instanceof Error ? reason.message : String(reason);
function requestsFolderContext(value: string) {
  const negative = /\b(?:not|never|without|avoid|exclude|skip|stop|don't|don’t|dont|shouldn't|shouldn’t|mustn't|mustn’t|cannot|can't|can’t)\b/iu;
  // Infer only a direct opening request. Quoted/multiline/reference text and
  // ambiguous later instructions require the visible manual permission toggle.
  if (/[\r\n`'"‘’“”«»:]/u.test(value) || negative.test(value)) return false;
  const opening = value.trim().split(/[.!?]/u)[0].replace(/^(?:please\s+)?(?:(?:can|could|would|will)\s+you\s+)?(?:please\s+)?/iu, '');
  const target = String.raw`(?:my|our)\s+other(?:\s+[\p{L}\p{N}-]+){0,4}\s+chats\b`;
  const direct = new RegExp(String.raw`^(?:look|search|use|read|check|consult|refer|review)\s+(?:(?:in|through|at|to|from|across)\s+)?${target}`, 'iu');
  const findIn = new RegExp(String.raw`^(?:find|retrieve|pull|look\s+for|search\s+for)\s+(?:[\p{L}\p{N}-]+\s+){1,10}(?:in|from|across)\s+${target}`, 'iu');
  return direct.test(opening) || findIn.test(opening);
}
type NameAction = { kind: 'new-folder' | 'rename-folder' | 'rename-chat'; id?: string; title?: string };
type StreamBuffer = { chatId: string; text: string; sources?: Source[]; activity: string };

function MessageView({ message, activity, theme, highlight, onSource, onRetry, retryEnabled, report }: { message: Message; activity?: string; theme: string; highlight: boolean; onSource: (source: Source) => void; onRetry: () => void; retryEnabled: boolean; report: (text: string) => void }) {
  const folderSources = message.sources.filter((source) => source.kind === 'chat');
  const webSources = message.sources.filter((source) => source.kind === 'web');
  if (message.role === 'user') return <div id={`message-${message.id}`} className={`message user-message ${highlight ? 'highlighted' : ''}`}>{message.content}</div>;
  if (message.role !== 'assistant') return null;
  const stopped = message.status === 'stopped' || message.status === 'cancelled';
  const failed = ['error', 'failed', 'interrupted'].includes(message.status);
  return <article id={`message-${message.id}`} className={`message assistant ${highlight ? 'highlighted' : ''}`} aria-label="Melly response">
    {(folderSources.length > 0 || webSources.length > 0) && <div className="retrieval-line">
      {folderSources.length > 0 && <><Icon name="folder" /><span>{folderSources.length} {folderSources.length === 1 ? 'snippet' : 'snippets'} from other chats</span></>}
      {folderSources.length > 0 && webSources.length > 0 && <span className="dot">·</span>}
      {webSources.length > 0 && <><Icon name="globe" /><span>{webSources.length} web {webSources.length === 1 ? 'source' : 'sources'}</span></>}
    </div>}
    {activity && message.status === 'streaming' && <div className="stream-activity" role="status">{activity}</div>}
    {message.content ? <div className="markdown"><ReactMarkdown remarkPlugins={[remarkGfm]} urlTransform={(url) => url.startsWith('melly-source:') ? url : defaultUrlTransform(url)} components={{
      a: ({ href, children }) => {
        if (href?.startsWith('melly-source:')) {
          const source = message.sources.find((entry) => entry.id === href.slice('melly-source:'.length));
          return source ? <button className="source inline-source" aria-label={`Open source ${source.title}`} title={source.snippet} onClick={() => onSource(source)}><Icon name={source.kind === 'chat' ? 'message' : 'globe'} /><span>{children}</span><Icon name={source.kind === 'chat' ? 'right' : 'external'} /></button> : <span>{children}</span>;
        }
        return <a href={href} onClick={(event) => { event.preventDefault(); if (href) onSource({ id: '', kind: 'web', title: String(children), url: href, chatId: null, messageId: null, snippet: '' }); }}>{children}</a>;
      },
      img: ({ alt }) => <span className="message-status">[Image{alt ? `: ${alt}` : ''}]</span>,
    }}>{message.content}</ReactMarkdown></div> : message.status === 'streaming' ? <span className="stream-pending" aria-label="Preparing response" /> : null}
    {message.sources.some((source) => !message.content.includes(`melly-source:${source.id}`)) && <div className="source-row">{message.sources.filter((source) => !message.content.includes(`melly-source:${source.id}`)).map((source) => <button key={source.id || `${source.title}-${source.url}`} className="source" aria-label={`Open source ${source.title}`} title={source.snippet || source.title} onClick={() => onSource(source)}><Icon name={source.kind === 'chat' ? 'message' : 'globe'} /><span>{source.title}</span><Icon name={source.kind === 'chat' ? 'right' : 'external'} /></button>)}</div>}
    {message.status !== 'streaming' && <div className="message-tools">
      {message.content && <button className="icon-button" aria-label="Copy response" title="Copy response" onClick={() => { void navigator.clipboard.writeText(message.content).then(() => report('Response copied.')).catch(() => report('Could not copy the response.')); }}><Icon name="copy" /></button>}
      {stopped && <span className="message-status">Response stopped</span>}
      {failed && <span className="message-status">Response interrupted</span>}
      {retryEnabled && <button className={`text-action ${stopped || failed ? '' : 'retry-quiet'}`} aria-label="Retry response" onClick={onRetry}>Retry</button>}
      {message.providerId && <span className="sr-only">{message.providerId === 'mock' ? 'Synthetic Demo reply' : `${message.modelId}, ${message.billingRoute}`}</span>}
    </div>}
  </article>;
}

export default function App() {
  const [data, setData] = useState<Bootstrap | null>(null); const [bootError, setBootError] = useState('');
  const [selectedId, setSelectedId] = useState<string | null>(null); const selectedRef = useRef<string | null>(null);
  const [view, setView] = useState<'chat' | 'settings'>('chat');
  const [items, setItems] = useState<Message[]>([]); const [loading, setLoading] = useState(false);
  const [hasOlder, setHasOlder] = useState(false); const [viewingOlder, setViewingOlder] = useState(false);
  const [draft, setDraft] = useState(''); const [webSearch, setWebSearch] = useState(false); const [manualFolder, setManualFolder] = useState<boolean | null>(null);
  const [running, setRunning] = useState<Set<string>>(new Set()); const [starting, setStarting] = useState(false); const sendingRef = useRef(false);
  const [notice, setNotice] = useState(''); const [sidebar, setSidebar] = useState(window.innerWidth > 600);
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [chatMenu, setChatMenu] = useState(false); const [folderMenu, setFolderMenu] = useState<string | null>(null); const [picker, setPicker] = useState<'providers' | 'models' | null>(null);
  const [searchOpen, setSearchOpen] = useState(false); const [nameAction, setNameAction] = useState<NameAction | null>(null); const [deleteOpen, setDeleteOpen] = useState(false);
  const [highlight, setHighlight] = useState<string | null>(null); const jump = useRef<{ chatId: string; messageId: string } | null>(null);
  const [systemDark, setSystemDark] = useState(window.matchMedia('(prefers-color-scheme: dark)').matches);
  const [activities, setActivities] = useState<Record<string, string>>({});
  const streamBuffers = useRef(new Map<string, StreamBuffer>()); const frame = useRef<number | null>(null);
  const textarea = useRef<HTMLTextAreaElement>(null); const scroll = useRef<HTMLDivElement>(null); const modelButton = useRef<HTMLButtonElement>(null); const providerButton = useRef<HTMLButtonElement>(null);
  const followBottom = useRef(true); const mounted = useRef(true);
  const messageRequest = useRef(0);
  const readySent = useRef(false);
  selectedRef.current = selectedId;

  const refresh = useCallback(async () => {
    const fresh = await backend.invoke<Bootstrap>('bootstrap');
    if (mounted.current) setData((current) => {
      const selected = current?.chats.find((chat) => chat.id === selectedRef.current);
      return selected && !fresh.chats.some((chat) => chat.id === selected.id) ? { ...fresh, chats: [...fresh.chats, selected] } : fresh;
    });
    return fresh;
  }, []);
  const mergeStreams = useCallback((messages: Message[]) => messages.map((message) => {
    const buffer = streamBuffers.current.get(message.id);
    return buffer && message.status === 'streaming' ? { ...message, content: buffer.text || message.content, sources: buffer.sources || message.sources } : message;
  }), []);
  const loadMessages = useCallback(async (chatId: string, target?: string, retainHistory = false) => {
    const request = ++messageRequest.current;
    if (selectedRef.current === chatId) setLoading(true);
    try {
      let page = await backend.invoke<Message[]>('messages', { chatId, before: null });
      let olderPage = false;
      if (target) {
        // Each read and the displayed page stay bounded while locating an older citation.
        while (!page.some((message) => message.id === target) && page.length === 60) {
          if (selectedRef.current !== chatId || messageRequest.current !== request) return;
          const prior = await backend.invoke<Message[]>('messages', { chatId, before: page[0].createdAt });
          if (!prior.length) break;
          page = prior; olderPage = true;
        }
      }
      if (selectedRef.current !== chatId || messageRequest.current !== request || !mounted.current) return;
      setItems((current) => {
        if (!retainHistory || !current.length) return mergeStreams(page);
        const fresh = new Map(page.map((message) => [message.id, message]));
        const known = new Set(current.map((message) => message.id));
        const result = current.map((message) => fresh.get(message.id) || message);
        if (current.some((message) => fresh.has(message.id))) result.push(...page.filter((message) => !known.has(message.id)));
        return mergeStreams(result.slice(-240));
      });
      if (!retainHistory) { setHasOlder(page.length === 60); setViewingOlder(olderPage); }
      if (target && page.some((message) => message.id === target)) { followBottom.current = false; setHighlight(target); }
      else if (target) setNotice('The source message is no longer available.');
    } catch (reason) { if (selectedRef.current === chatId) setNotice(errorText(reason)); }
    finally { if (selectedRef.current === chatId && messageRequest.current === request && mounted.current) setLoading(false); }
  }, [mergeStreams]);
  useEffect(() => {
    mounted.current = true;
    const unlisteners: Array<() => void> = [];
    const register = async () => {
      const unlisten = await backend.listen<StreamUpdate>('chat-stream', (update) => {
        if (update.kind === 'delta' || update.kind === 'sources' || update.kind === 'tool') {
          const buffer = streamBuffers.current.get(update.messageId) || { chatId: update.chatId, text: '', activity: '' };
          if (update.kind === 'delta') buffer.text += update.text;
          if (update.kind === 'sources') buffer.sources = update.sources;
          if (update.kind === 'tool') buffer.activity = update.text;
          streamBuffers.current.set(update.messageId, buffer);
          if (frame.current === null) frame.current = requestAnimationFrame(() => {
            frame.current = null;
            if (!mounted.current) return;
            setItems((messages) => mergeStreams(messages));
            setActivities(Object.fromEntries([...streamBuffers.current.entries()].filter(([, value]) => value.chatId === selectedRef.current).map(([key, value]) => [key, value.activity])));
          });
        } else if (['done', 'error', 'stopped'].includes(update.kind)) {
          setRunning((current) => { const next = new Set(current); next.delete(update.chatId); return next; });
          streamBuffers.current.delete(update.messageId);
          if (update.chatId === selectedRef.current) {
            if (update.kind === 'error') setNotice(update.text || 'The response was interrupted. You can retry it.');
            void loadMessages(update.chatId, undefined, true);
          }
          void refresh().catch((reason) => setNotice(errorText(reason)));
        }
      });
      if (!mounted.current) unlisten(); else unlisteners.push(unlisten);
      const connectionListener = await backend.listen<unknown>('connection-update', (payload) => {
        const update = payload && typeof payload === 'object' ? payload as Record<string, unknown> : {};
        if (update.error || update.status === 'error') setNotice(String(update.message || update.error || 'Sign-in did not complete.'));
        else if (typeof update.message === 'string') setNotice(update.message);
        if (update.status === 'connected') invalidateModelCatalog('chatgpt');
        void refresh().then((fresh) => {
          const connected = fresh.providers.find((entry) => entry.id === 'chatgpt' && entry.configured);
          if (update.status === 'connected' && connected) void discoverModels(connected, true).catch(() => {});
        }).catch((reason) => setNotice(errorText(reason)));
      });
      if (!mounted.current) connectionListener(); else unlisteners.push(connectionListener);
      const initial = await backend.invoke<Bootstrap>('bootstrap');
      if (mounted.current) { setData(initial); setSelectedId(initial.chats[0]?.id || null); }
    };
    void register().catch((reason) => { if (mounted.current) setBootError(errorText(reason)); });
    return () => { mounted.current = false; unlisteners.forEach((unlisten) => unlisten()); if (frame.current !== null) cancelAnimationFrame(frame.current); };
  }, [loadMessages, mergeStreams, refresh]);
  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)');
    const change = () => setSystemDark(media.matches);
    media.addEventListener('change', change); return () => media.removeEventListener('change', change);
  }, []);
  useEffect(() => {
    if (!nativeApp || !data || readySent.current) return;
    let secondFrame: number | undefined;
    const firstFrame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => {
        readySent.current = true;
        // Content-free launch measurement; it does not change application behavior.
        void backend.invoke('frontend_ready').catch(() => {});
      });
    });
    return () => { cancelAnimationFrame(firstFrame); if (secondFrame !== undefined) cancelAnimationFrame(secondFrame); };
  }, [!!data]);
  const theme = data?.settings.theme === 'system' || !data ? (systemDark ? 'dark' : 'light') : data.settings.theme;
  useLayoutEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.dataset.reduceMotion = String(data?.settings.reduceMotion || false);
    document.documentElement.style.setProperty('--message-font', `${data?.settings.fontSize || 16}px`);
  }, [theme, data?.settings.reduceMotion, data?.settings.fontSize]);
  useEffect(() => {
    setHighlight(null); setManualFolder(null); setChatMenu(false); setActivities({});
    if (!selectedId) { setItems([]); setHasOlder(false); return; }
    const target = jump.current?.chatId === selectedId ? jump.current.messageId : undefined; jump.current = null;
    void loadMessages(selectedId, target);
  }, [selectedId, loadMessages]);
  useLayoutEffect(() => {
    if (highlight) document.getElementById(`message-${highlight}`)?.scrollIntoView({ block: 'center', behavior: 'auto' });
    else if (followBottom.current && scroll.current) scroll.current.scrollTop = scroll.current.scrollHeight;
  }, [items, highlight]);
  useLayoutEffect(() => { if (textarea.current) { textarea.current.style.height = '24px'; textarea.current.style.height = `${Math.min(180, textarea.current.scrollHeight)}px`; } }, [draft]);
  const closePicker = () => { setPicker(null); (picker === 'providers' ? providerButton : modelButton).current?.focus(); };
  useEffect(() => {
    const outside = (event: MouseEvent) => { if (!(event.target as Element).closest('[data-menu-root]')) { setChatMenu(false); setFolderMenu(null); setPicker(null); } };
    const escape = (event: globalThis.KeyboardEvent) => { if (event.key === 'Escape') { setChatMenu(false); setFolderMenu(null); if (picker) closePicker(); } };
    document.addEventListener('mousedown', outside); document.addEventListener('keydown', escape);
    return () => { document.removeEventListener('mousedown', outside); document.removeEventListener('keydown', escape); };
  }, [picker]);

  const selectedChat = data?.chats.find((chat) => chat.id === selectedId);
  const selectedFolder = data?.folders.find((folder) => folder.id === selectedChat?.folderId);
  const provider = data?.providers.find((entry) => entry.id === data.settings.defaultProvider) || data?.providers[0];
  const model = data?.settings.defaultModel || provider?.connection.modelId || '';
  const activeRun = !!selectedId && running.has(selectedId);
  const explicitFolder = requestsFolderContext(draft);
  const folderContext = !!selectedFolder && !!provider?.tools && (manualFolder === true || (explicitFolder && manualFolder !== false));
  const webAvailable = !!provider && (provider.id === 'mock' || provider.nativeSearch || (provider.tools && !!data?.searchConfigured));
  const sendAvailable = !!provider?.configured && !provider.disabledReason;

  const selectChat = (chat: Chat) => {
    selectedRef.current = chat.id; setSelectedId(chat.id);
    setData((current) => current && !current.chats.some((entry) => entry.id === chat.id) ? { ...current, chats: [...current.chats, chat] } : current);
    setView('chat'); setHighlight(null); setNotice(''); followBottom.current = true;
    if (window.innerWidth <= 600) setSidebar(false);
  };
  const createChat = async (folderId: string | null = null) => { const chat = await backend.invoke<Chat>('create_chat', { folderId }); await refresh(); selectChat(chat); setDraft(''); setFolderMenu(null); requestAnimationFrame(() => textarea.current?.focus()); return chat; };
  const seed = async () => { const fresh = await backend.invoke<Bootstrap>('demo_seed'); setData(fresh); const chat = fresh.chats.find((entry) => entry.title === 'Interview prep') || fresh.chats[0]; if (chat) selectChat(chat); setWebSearch(true); };
  const saveSettings = async (settings: Settings) => { await backend.invoke('save_settings', { settings }); await refresh(); };
  const chooseProvider = async (next: ProviderView) => {
    if (!data || (!next.configured && next.id !== 'mock') || next.disabledReason) return;
    await saveSettings({ ...data.settings, defaultProvider: next.id, defaultModel: next.connection.modelId || next.defaultModel });
    setWebSearch(false); setManualFolder(null); setPicker('models');
  };
  const chooseModel = async (modelId: string) => {
    if (!data || !provider || !modelId.trim()) return;
    await backend.invoke('save_connection', { connection: { ...provider.connection, modelId: modelId.trim() } });
    await saveSettings({ ...data.settings, defaultProvider: provider.id, defaultModel: modelId.trim() });
    setWebSearch(false); closePicker();
  };
  const send = async (retry = false) => {
    if (!data || !provider || !sendAvailable || activeRun || sendingRef.current || (retry && (viewingOlder || loading)) || (!retry && !draft.trim())) return;
    sendingRef.current = true; setStarting(true); setNotice(''); setHighlight(null); followBottom.current = true;
    let chatId = selectedId;
    try {
      if (!chatId) { const chat = await createChat(); chatId = chat.id; }
      selectedRef.current = chatId;
      setRunning((current) => new Set(current).add(chatId!));
      await backend.invoke<Message>('start_chat', { input: { chatId, content: draft.trim(), providerId: provider.id, modelId: model, webSearch: webSearch && webAvailable, folderContext, retry } });
      if (!retry) setDraft(''); setManualFolder(null); await loadMessages(chatId); await refresh();
    } catch (reason) { setNotice(errorText(reason)); if (chatId) setRunning((current) => { const next = new Set(current); next.delete(chatId!); return next; }); }
    finally { sendingRef.current = false; setStarting(false); textarea.current?.focus(); }
  };
  const source = async (value: Source) => {
    if (value.kind === 'chat' && value.chatId) {
      let chat = data?.chats.find((entry) => entry.id === value.chatId);
      if (!chat) chat = await backend.invoke<Chat | null>('chat', { chatId: value.chatId }) || undefined;
      if (!chat) { setNotice('The source chat is no longer available.'); return; }
      if (value.messageId) jump.current = { chatId: chat.id, messageId: value.messageId };
      if (chat.id === selectedId) { setView('chat'); await loadMessages(chat.id, value.messageId || undefined); }
      else selectChat(chat);
    } else { try { await backend.invoke('open_source', { source: value }); } catch (reason) { setNotice(errorText(reason)); } }
  };
  const older = async () => {
    if (!selectedId || !items.length || loading) return;
    const chatId = selectedId; const request = ++messageRequest.current; setLoading(true); followBottom.current = false; setHighlight(null);
    const previousHeight = scroll.current?.scrollHeight || 0; const previousTop = scroll.current?.scrollTop || 0;
    try {
      const page = await backend.invoke<Message[]>('messages', { chatId, before: items[0].createdAt });
      if (selectedRef.current === chatId && messageRequest.current === request) {
        if (page.length + items.length > 240) setViewingOlder(true);
        setItems((current) => mergeStreams([...page, ...current].slice(0, 240)));
        setHasOlder(page.length === 60);
        requestAnimationFrame(() => { if (scroll.current) scroll.current.scrollTop = previousTop + scroll.current.scrollHeight - previousHeight; });
      }
    } catch (reason) { setNotice(errorText(reason)); } finally { if (messageRequest.current === request) setLoading(false); }
  };
  useEffect(() => {
    const shortcut = (event: globalThis.KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey)) return;
      if (event.key.toLowerCase() === 'k') { event.preventDefault(); setSearchOpen(true); }
      if (event.key === ',') { event.preventDefault(); setView('settings'); }
      if (event.key.toLowerCase() === 'n') { event.preventDefault(); void createChat().catch((reason) => setNotice(errorText(reason))); }
    };
    document.addEventListener('keydown', shortcut); return () => document.removeEventListener('keydown', shortcut);
  }, [data]);
  const nameSave = async (name: string) => {
    if (!nameAction) return;
    if (nameAction.kind === 'new-folder') await backend.invoke<Folder>('create_folder', { title: name });
    else if (nameAction.kind === 'rename-folder') await backend.invoke('rename_folder', { folderId: nameAction.id, title: name });
    else await backend.invoke('rename_chat', { chatId: nameAction.id, title: name });
    await refresh();
  };
  if (bootError) return <div className="loading-state"><div role="alert">{bootError}<br /><button className="text-action" onClick={() => window.location.reload()}>Try again</button></div></div>;
  if (!data || !provider) return <div className="loading-state">Opening Melly…</div>;

  return <div className={`app ${nativeApp ? 'native' : 'browser-preview'} ${sidebar ? '' : 'sidebar-hidden'}`} style={{ '--message-font': `${data.settings.fontSize}px` } as CSSProperties}>
    {sidebar && <aside className="sidebar" aria-label="Chats">
      <div className="chrome" data-tauri-drag-region><span className="preview-brand" data-tauri-drag-region>Melly</span><button className="icon-button" aria-label="Hide sidebar" onClick={() => setSidebar(false)}><Icon name="sidebar" /></button></div>
      <nav className="nav" aria-label="Main"><button className="nav-row" onClick={() => { void createChat().catch((reason) => setNotice(errorText(reason))); }}><Icon name="edit" />New Chat</button><button className={`nav-row ${view === 'settings' ? 'selected' : ''}`} onClick={() => { setView('settings'); setNotice(''); }}><Icon name="settings" />Settings</button>{view === 'chat' && <button className="nav-row chat-search" onClick={() => setSearchOpen(true)}><Icon name="search" />Search chats</button>}</nav>
      {view === 'chat' && <div className="sidebar-list"><div className="section-label"><span>Folders</span><button className="icon-button" aria-label="New folder" title="New folder" onClick={() => setNameAction({ kind: 'new-folder' })}><Icon name="plus" className="small" /></button></div>
        {!data.folders.length && <p className="sidebar-empty">Keep related chats together.</p>}
        {data.folders.map((folder) => <div className="folder-wrap" key={folder.id} data-menu-root>
          <button className="folder" aria-expanded={!collapsed.has(folder.id)} onClick={() => setCollapsed((current) => { const next = new Set(current); if (next.has(folder.id)) next.delete(folder.id); else next.add(folder.id); return next; })}><Icon name={collapsed.has(folder.id) ? 'chevron' : 'down'} className="chevron" /><Icon name="folder" className="folder-icon" /><span>{folder.title}</span></button>
          <button className="icon-button folder-options" aria-label={`Folder options for ${folder.title}`} onClick={() => setFolderMenu(folderMenu === folder.id ? null : folder.id)}><Icon name="more" className="small" /></button>
          {folderMenu === folder.id && <div className="menu folder-menu" role="menu"><button role="menuitem" className="menu-item" onClick={() => { void createChat(folder.id).catch((reason) => setNotice(errorText(reason))); }}>New chat in folder</button><button role="menuitem" className="menu-item" onClick={() => { setNameAction({ kind: 'rename-folder', id: folder.id, title: folder.title }); setFolderMenu(null); }}>Rename folder</button></div>}
          {!collapsed.has(folder.id) && data.chats.filter((chat) => chat.folderId === folder.id).map((chat) => <button className={`chat-row ${chat.id === selectedId ? 'selected' : ''}`} key={chat.id} onClick={() => selectChat(chat)} title={chat.title}><span>{chat.title}</span>{running.has(chat.id) && <span className="sr-only">Responding</span>}</button>)}
        </div>)}
        <div className="section-label">Recent</div>{data.chats.filter((chat) => !chat.folderId).map((chat) => <button key={chat.id} className={`recent-row ${chat.id === selectedId ? 'selected' : ''}`} onClick={() => selectChat(chat)} title={chat.title}><span>{chat.title}</span></button>)}
        {!data.chats.some((chat) => !chat.folderId) && <p className="sidebar-empty">Your recent chats appear here.</p>}
      </div>}
      {!nativeApp && <div className="sidebar-foot">Browser preview · Mock</div>}
    </aside>}
    <main className="main">
      <header className="main-header" data-tauri-drag-region>
        {!sidebar && <button className="icon-button" aria-label="Show sidebar" onClick={() => setSidebar(true)}><Icon name="sidebar" /></button>}
        {view === 'settings' ? <h1 data-tauri-drag-region>Settings</h1> : selectedChat ? <div className="chat-heading" data-tauri-drag-region><span className="chat-title" data-tauri-drag-region>{selectedChat.title}</span><span className="chat-location" data-tauri-drag-region>{selectedFolder?.title || 'No folder'}</span></div> : <span />}
        {!sidebar && !nativeApp && <span className="preview-badge">Browser preview · Mock</span>}
        {view === 'chat' && selectedChat && <div className="chat-menu-button" data-menu-root><button className="icon-button" aria-label="Chat options" aria-expanded={chatMenu} onClick={() => setChatMenu(!chatMenu)}><Icon name="more" /></button>{chatMenu && <div className="menu chat-menu" role="menu"><button className="menu-item" role="menuitem" onClick={() => { setNameAction({ kind: 'rename-chat', id: selectedChat.id, title: selectedChat.title }); setChatMenu(false); }}>Rename chat</button><label className="menu-field">Move to folder<select aria-label="Move to folder" disabled={activeRun} value={selectedChat.folderId || ''} onChange={(event) => { void backend.invoke('move_chat', { chatId: selectedChat.id, folderId: event.target.value || null }).then(refresh).then(() => { setChatMenu(false); setManualFolder(null); }).catch((reason) => setNotice(errorText(reason))); }}><option value="">No folder</option>{data.folders.map((folder) => <option key={folder.id} value={folder.id}>{folder.title}</option>)}</select></label><button className="menu-item danger" role="menuitem" disabled={activeRun} title={activeRun ? "Stop the response before deleting this chat" : undefined} onClick={() => { setDeleteOpen(true); setChatMenu(false); }}>Delete chat</button></div>}</div>}
      </header>
      {notice && <div className="notice" role="alert"><span>{notice}</span><button className="icon-button" aria-label="Dismiss notification" onClick={() => setNotice('')}><Icon name="close" className="small" /></button></div>}
      {view === 'settings' ? <SettingsView data={data} theme={theme} onSettings={saveSettings} refresh={refresh} onSeed={seed} report={setNotice} /> : <>
        <div className="conversation-scroll" ref={scroll} onScroll={() => { const element = scroll.current; if (element) followBottom.current = element.scrollHeight - element.scrollTop - element.clientHeight < 100; }}>
          {!items.length && !loading ? <div className="empty-state"><h1>{selectedFolder ? `A new thought in ${selectedFolder.title}` : 'What’s on your mind?'}</h1><p>{provider.id === 'mock' ? 'Try a synthetic conversation, or connect a provider in Settings.' : `Start a conversation with ${providerName(provider)}.`}</p>{provider.id === 'mock' && <button className="text-action" onClick={() => { void seed().catch((reason) => setNotice(errorText(reason))); }}>Try an example</button>}</div> : <div className={`transcript ${hasOlder || viewingOlder ? 'has-older' : ''}`} aria-label="Conversation">
            {hasOlder && <button className="load-older" disabled={loading} onClick={() => { void older(); }}>{loading ? 'Loading…' : 'Load older messages'}</button>}
            {viewingOlder && <button className="load-older" onClick={() => { if (selectedId) { setHighlight(null); followBottom.current = true; void loadMessages(selectedId); } }}>Back to latest messages</button>}
            {items.map((message, index) => <MessageView key={message.id} message={message} theme={theme} activity={activities[message.id]} highlight={highlight === message.id} onSource={(value) => { void source(value).catch((reason) => setNotice(errorText(reason))); }} onRetry={() => { void send(true); }} retryEnabled={index === items.length - 1 && !viewingOlder && !loading && !activeRun && !starting} report={setNotice} />)}
          </div>}
        </div>
        <div className="composer-area" data-menu-root>
          {selectedFolder && (folderContext || explicitFolder || manualFolder !== null) && <div className="folder-consent"><button aria-label={`Use other chats in ${selectedFolder.title}`} aria-pressed={folderContext} disabled={!provider.tools} onClick={() => setManualFolder(!folderContext)}><Icon name={folderContext ? 'check' : 'folder'} />{provider.tools ? `Use other chats in ${selectedFolder.title}` : 'This provider does not support folder references'}</button></div>}
          {!selectedFolder && explicitFolder && <div className="folder-consent"><Icon name="folder" /><span>Move this chat into a folder to use other chats.</span></div>}
          <form className="composer" onSubmit={(event) => { event.preventDefault(); void send(); }}>
            <textarea ref={textarea} aria-label="Message" rows={1} placeholder="Send a message" value={draft} onChange={(event) => setDraft(event.target.value)} onKeyDown={(event) => {
              if (event.nativeEvent.isComposing || event.key !== 'Enter') return;
              const shouldSend = data.settings.sendBehavior === 'cmd-enter' ? event.metaKey || event.ctrlKey : !event.shiftKey && !event.altKey;
              if (shouldSend) { event.preventDefault(); void send(); }
            }} />
            <div className="composer-controls">
              {selectedFolder && <button type="button" className="round-control" aria-label="Folder context" aria-pressed={folderContext} disabled={!provider.tools} title={provider.tools ? `Use other chats in ${selectedFolder.title}` : 'Folder context is unavailable for this provider'} onClick={() => setManualFolder(!folderContext)}><Icon name="folder" /></button>}
              <button type="button" className="round-control" aria-label="Search the web" aria-pressed={webSearch && webAvailable} aria-disabled={!webAvailable} title={webAvailable ? (provider.id === 'mock' ? 'Synthetic web search in Demo' : provider.nativeSearch ? 'Use built-in web search' : 'Use external web search') : 'Set up external search in Settings'} onClick={() => { if (webAvailable) setWebSearch(!webSearch); else setNotice(provider.tools ? 'Add an external search key in Settings to use web search with this provider.' : 'Web search is not available for this provider.'); }}><Icon name="globe" /></button>
              <div className="model-pill" role="group" aria-label="Provider and model">
                <button ref={providerButton} type="button" className="provider-picker-trigger" aria-label="Choose provider" title={`${providerName(provider)} · choose provider`} aria-expanded={picker === 'providers'} aria-haspopup="listbox" onClick={() => setPicker(picker === 'providers' ? null : 'providers')}><ProviderIcon id={provider.id} theme={theme} small /></button>
                <span className="provider-divider" aria-hidden="true" />
                <button ref={modelButton} type="button" className="model-picker-trigger" aria-label="Choose model" title="Choose model" aria-expanded={picker === 'models'} aria-haspopup="listbox" onClick={() => setPicker(picker === 'models' ? null : 'models')}><span className="model-name">{modelLabel(provider, model)}</span><span className="divider" /><span className="route">{routeLabel(provider)}</span><Icon name="down" /></button>
              </div>
              {activeRun ? <button type="button" className="round-control stop" aria-label="Stop response" title="Stop response" onClick={() => { if (selectedId) void backend.invoke('stop_chat', { chatId: selectedId }).catch((reason) => setNotice(errorText(reason))); }}><span className="stop-square" /></button> : <button type="submit" className="round-control send" aria-label="Send message" disabled={!draft.trim() || starting || !sendAvailable}><Icon name="up" /></button>}
            </div>
          </form>
          {!sendAvailable && <p className="composer-note">{providerName(provider)} is not connected. <button onClick={() => setView('settings')}>Open Settings</button></p>}
          {picker === 'providers' && <ProviderPicker providers={data.providers} selected={provider.id} theme={theme} close={closePicker} choose={chooseProvider} settings={() => { setPicker(null); setView('settings'); }} />}
          {picker === 'models' && <ModelPicker key={provider.id} provider={provider} selected={model} close={closePicker} choose={chooseModel} />}
        </div>
      </>}
    </main>
    {searchOpen && <SearchDialog folders={data.folders} onClose={() => setSearchOpen(false)} onSelect={selectChat} />}
    {nameAction && <NameDialog title={nameAction.kind === 'new-folder' ? 'New folder' : nameAction.kind === 'rename-folder' ? 'Rename folder' : 'Rename chat'} initial={nameAction.title} action={nameAction.kind === 'new-folder' ? 'Create' : 'Save'} onClose={() => setNameAction(null)} onSave={nameSave} />}
    {deleteOpen && selectedChat && <Dialog title="Delete this chat?" onClose={() => setDeleteOpen(false)}><p>“{selectedChat.title}” and its messages will be removed.</p><div className="dialog-actions"><button className="button quiet" onClick={() => setDeleteOpen(false)}>Cancel</button><button className="button danger" onClick={() => { void backend.invoke('delete_chat', { chatId: selectedChat.id }).then(async () => { selectedRef.current = null; setSelectedId(null); setDeleteOpen(false); setItems([]); await refresh(); }).catch((reason) => setNotice(errorText(reason))); }}>Delete chat</button></div></Dialog>}
  </div>;
}
