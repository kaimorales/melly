import { ResponseReferences } from './ResponseReferences';
import { useComposerDraft } from './useComposerDraft';
import { UndoChanges } from './UndoChanges';
import { advanceStream, streamRevealWindow } from './stream-reveal';
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import ReactMarkdown, { defaultUrlTransform } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import { backend, nativeApp } from './backend';
import { NameDialog, SearchDialog } from './dialogs';
import { ArchivedProjects, DeleteProject, FileCitation, ProjectFiles } from './ProjectFiles';
import { ChatList } from './ChatList';
import { Thinking } from './Thinking';
import { CodingActivity, type CodingActivityState } from './CodingActivity';
import { FileEditMode, InlineFileEdit, splitFileEdit } from './InlineFileEdit';
import { ContextMeter } from './Usage';
import { ChatActions, DeleteChatDialog } from './ChatActions';
import { MarkdownCodeBlock } from './CodeBlock';
import { MarkdownInlineCode } from './MathFormula';
import { normalizeMathDelimiters, remarkMathPresentation } from './math-markdown';
import { Icon, ProviderIcon } from './icons';
import { modelLabel, providerName, routeLabel, SettingsView } from './SettingsView';
import { CompatibleModelPicker, ModelPicker, ProviderPicker } from './ModelPickers';
import { discoverModels, invalidateModelCatalog } from './model-catalog';
import type { AttachmentInfo, Bootstrap, Chat, ChatModel, ContextLimit, Folder, Message, ProviderView, Settings, Source, StreamUpdate } from './types';

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
type StreamBuffer = { chatId: string; text: string; reasoning: string; phase?: 'thinking' | 'answer'; sources?: Source[]; activity: string } & StreamReveal;
type StreamReveal = { visible?: number; deadline?: number; done?: StreamUpdate };

function MessageView({ message, activity, codingActivity, theme, highlight, onSource, onRetry, onBranch, onEdit, canChange, retryEnabled, report }: { message: Message; activity?: string; codingActivity?: CodingActivityState; theme: string; highlight: boolean; onSource: (source: Source) => void; onRetry: () => void; onBranch: () => void; onEdit: () => void; canChange: boolean; retryEnabled: boolean; report: (text: string) => void }) {
  const reply = message.role === 'assistant' ? splitFileEdit(message.content, message.status === 'streaming') : { text: message.content, proposed: false };
  const markdown = useMemo(() => message.role === 'assistant' ? normalizeMathDelimiters(reply.text) : reply.text, [reply.text, message.role]);
  if (message.role === 'user') return <div className="user-message-group"><div id={`message-${message.id}`} className={`message user-message ${highlight ? 'highlighted' : ''}`}>{message.content}{!!message.attachments?.length && <div className="sent-files">{message.attachments.map(file => <span key={file.id} title={file.kind === 'image-text' ? 'Recognized text from image' : 'Shared file text'}><Icon name="folder" />{file.name}{file.kind === 'image-text' ? ' · text' : ''}</span>)}</div>}</div><div className="user-message-actions"><button disabled={!canChange} onClick={onEdit}>Edit</button><button disabled={!canChange} onClick={onBranch}>Branch</button></div></div>;
  if (message.role !== 'assistant') return null;
  const stopped = message.status === 'stopped' || message.status === 'cancelled';
  const failed = ['error', 'failed', 'interrupted'].includes(message.status);
  const keptChanges = codingActivity?.results.some(result => 'action' in result);
  return <article id={`message-${message.id}`} className={`message assistant ${highlight ? 'highlighted' : ''}`} aria-label="Melly response">

    <Thinking message={message} activity={activity} />
    {reply.text ? <div className="markdown"><ReactMarkdown remarkPlugins={[remarkGfm, remarkMath, remarkMathPresentation]} urlTransform={(url) => url.startsWith('melly-source:') ? url : defaultUrlTransform(url)} components={{
      a: ({ href, children }) => {
        if (href?.startsWith('melly-source:')) {
          const source = message.sources.find((entry) => entry.id === href.slice('melly-source:'.length));
          return source ? <button className="source inline-source" aria-label={`Open source ${source.title}`} title={source.snippet} onClick={() => onSource(source)}><Icon name={source.kind === 'file' ? 'folder' : source.kind === 'chat' ? 'message' : 'globe'} /><span>{children}</span><Icon name={source.kind === 'web' ? 'external' : 'right'} /></button> : <span>{children}</span>;
        }
        return <a href={href} onClick={(event) => { event.preventDefault(); if (href) onSource({ id: '', kind: 'web', title: String(children), url: href, chatId: null, messageId: null, snippet: '' }); }}>{children}</a>;
      },
      img: ({ alt }) => <span className="message-status">[Image{alt ? `: ${alt}` : ''}]</span>,
      pre: MarkdownCodeBlock,
      code: MarkdownInlineCode,
    }}>{markdown}</ReactMarkdown></div> : null}
    {codingActivity && <CodingActivity activity={codingActivity} settled={message.status === 'complete'} />}
    {nativeApp && reply.proposed && message.status === 'complete' && <InlineFileEdit chatId={message.chatId} messageId={message.id} busy={!canChange} />}
    <ResponseReferences sources={message.sources} onSource={onSource} />
    {message.status !== 'streaming' && <div className="message-tools">
      {reply.text && <button className="icon-button" aria-label="Copy response" title="Copy response" onClick={() => { void navigator.clipboard.writeText(reply.text).then(() => report('Response copied.')).catch(() => report('Could not copy the response.')); }}><Icon name="copy" /></button>}
      <button className="text-action retry-quiet" disabled={!canChange} aria-label="Branch from response" onClick={onBranch}>Branch</button>
      {stopped && <span className="message-status">Response stopped</span>}
      {failed && <span className="message-status">{message.status === 'interrupted' ? 'Response interrupted' : 'Response failed'}</span>}
      {(stopped || failed) && keptChanges && <span className="message-status">Changes kept</span>}
      {retryEnabled && <button className={`text-action ${stopped || failed ? '' : 'retry-quiet'}`} aria-label={codingActivity ? 'Continue task' : 'Retry response'} onClick={onRetry}>{codingActivity ? 'Continue' : 'Retry'}</button>}
      {nativeApp && !reply.proposed && <UndoChanges chatId={message.chatId} messageId={message.id} busy={!canChange} />}
      {message.providerId && <span className="sr-only">{message.providerId === 'mock' ? 'Synthetic Demo reply' : `${message.modelId}, ${message.billingRoute}`}</span>}
    </div>}
  </article>;
}

export default function App() {
  const [archivedOpen, setArchivedOpen] = useState(false);
  const [deleteProject, setDeleteProject] = useState<Folder | null>(null);
  const [fileCitation, setFileCitation] = useState<Source | null>(null);
  const [projectFiles, setProjectFiles] = useState<Folder | null>(null);
  const [data, setData] = useState<Bootstrap | null>(null); const [bootError, setBootError] = useState('');
  const dataRef = useRef(data); dataRef.current = data;
  const [selectedId, setSelectedId] = useState<string | null>(null); const selectedRef = useRef<string | null>(null);
  const [view, setView] = useState<'chat' | 'settings'>('chat');
  const [items, setItems] = useState<Message[]>([]); const [loading, setLoading] = useState(false);
  const [hasOlder, setHasOlder] = useState(false); const [viewingOlder, setViewingOlder] = useState(false);
  const [webSearch, setWebSearch] = useState(false); const [manualFolder, setManualFolder] = useState<boolean | null>(null);
  const [draggingFiles, setDraggingFiles] = useState(false);
  const attachmentOperation = useRef(false);
  const attachmentEpoch = useRef(0);
  const fileState = useRef({ selectedId, view, starting: false, activeRun: false });
  const [pickingAttachment, setPickingAttachment] = useState(false);
  const [running, setRunning] = useState<Set<string>>(new Set()); const [starting, setStarting] = useState(false); const sendingRef = useRef(false);
  const [notice, setNotice] = useState('');
  const composer = useComposerDraft(selectedId, !!data, setNotice);
  const { content: draft, setContent: setDraft, attachments, setAttachments, editing, setEditing } = composer;
  const [sidebar, setSidebar] = useState(window.innerWidth > 600);
  const [moving, setMoving] = useState<Set<string>>(new Set()); const movingIds = useRef(new Set<string>());
  const removedChatIds = useRef(new Set<string>());
  const [chatMenu, setChatMenu] = useState(false); const [picker, setPicker] = useState<'providers' | 'models' | null>(null);
  const [draftModel, setDraftModel] = useState<ChatModel | null>(null);
  const [draftContext, setDraftContext] = useState<ContextLimit | null>(null);
  const [contextRevision, setContextRevision] = useState(0);
  const [compactingChat, setCompactingChat] = useState<string | null>(null);
  const savingContext = useRef(false);
  const [searchOpen, setSearchOpen] = useState(false); const [nameAction, setNameAction] = useState<NameAction | null>(null); const [deleteTarget, setDeleteTarget] = useState<Chat | null>(null);
  const [highlight, setHighlight] = useState<string | null>(null); const jump = useRef<{ chatId: string; messageId: string } | null>(null);
  const [systemDark, setSystemDark] = useState(window.matchMedia('(prefers-color-scheme: dark)').matches);
  const [codingActivities, setCodingActivities] = useState<Record<string, CodingActivityState>>({});
  const [activities, setActivities] = useState<Record<string, string>>({});
  const streamBuffers = useRef(new Map<string, StreamBuffer>()); const frame = useRef<number | null>(null);
  const textarea = useRef<HTMLTextAreaElement>(null); const scroll = useRef<HTMLDivElement>(null); const modelButton = useRef<HTMLButtonElement>(null); const providerButton = useRef<HTMLButtonElement>(null);
  const followBottom = useRef(true); const mounted = useRef(true);
  const messageRequest = useRef(0);
  const readySent = useRef(false);
  selectedRef.current = selectedId;

  const refresh = useCallback(async () => {
    const fresh = await backend.invoke<Bootstrap>('bootstrap');
    fresh.chats = fresh.chats.filter((chat) => !removedChatIds.current.has(chat.id));
    if (mounted.current) setData((current) => {
      const selected = current?.chats.find((chat) => chat.id === selectedRef.current);
      return selected && !fresh.chats.some((chat) => chat.id === selected.id) ? { ...fresh, chats: [...fresh.chats, selected] } : fresh;
    });
    return fresh;
  }, []);
  const mergeStreams = useCallback((messages: Message[]) => messages.map((message) => {
    const buffer = streamBuffers.current.get(message.id);
    return buffer && message.status === 'streaming' ? { ...message, content: buffer.text.slice(0, buffer.visible ?? buffer.text.length) || message.content, reasoning: buffer.reasoning || message.reasoning, phase: buffer.phase, sources: buffer.sources || message.sources } : message;
  }), []);
  const loadMessages = useCallback(async (chatId: string, target?: string, retainHistory = false) => {
    const request = ++messageRequest.current;
    if (selectedRef.current === chatId) setLoading(true);
    try {
      let [page, savedActivity] = await Promise.all([
        backend.invoke<Message[]>('messages', { chatId, before: null }),
        nativeApp ? backend.invoke<Record<string, CodingActivityState['results']>>('coding_activity', { chatId }).catch(() => null) : null,
      ]);
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
      if (savedActivity) {
        setCodingActivities(previous => {
          const next = { ...previous };
          for (const [id, results] of Object.entries(savedActivity)) {
            if ((next[id]?.results.length ?? 0) < results.length) next[id] = { ...next[id], results };
          }
          return next;
        });
      }
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
    const motionPreference = window.matchMedia('(prefers-reduced-motion: reduce)');
    const reducedMotion = () => motionPreference.matches || document.documentElement.dataset.reduceMotion === 'true';
    let lastPaint = -Infinity;
    const finishStream = (update: StreamUpdate) => {
      setRunning((current) => { const next = new Set(current); next.delete(update.chatId); return next; });
      streamBuffers.current.delete(update.messageId);
      if (update.chatId === selectedRef.current) {
        if (update.kind === 'error' || (update.kind === 'done' && update.text)) setNotice(update.text || 'The response was interrupted. You can retry it.');
        void loadMessages(update.chatId, undefined, true);
      }
      void refresh().catch((reason) => setNotice(errorText(reason)));
    };
    const queuePaint = () => {
      if (frame.current === null) frame.current = requestAnimationFrame(paintStreams);
    };
    const paintStreams = (now: number) => {
      frame.current = null;
      if (!mounted.current) return;
      if (now - lastPaint < 30 && !reducedMotion()) { queuePaint(); return; }
      lastPaint = now;
      let pending = false;
      const activities: Record<string, string> = {};
      for (const [messageId, buffer] of streamBuffers.current) {
        const visible = buffer.chatId === selectedRef.current;
        buffer.visible = visible && !reducedMotion()
          ? advanceStream(buffer.text, buffer.visible ?? buffer.text.length, now, buffer.deadline ?? now)
          : buffer.text.length;
        if (buffer.visible < buffer.text.length) pending = true;
        else if (buffer.done) { finishStream(buffer.done); continue; }
        if (visible) activities[messageId] = buffer.activity;
      }
      setItems(mergeStreams);
      setActivities(activities);
      if (pending) queuePaint();
    };
    const register = async () => {
      const unlisten = await backend.listen<StreamUpdate>('chat-stream', (update) => {
        if (update.kind === 'coding-approval' || update.kind === 'coding-approval-closed' || update.kind === 'coding-result') {
          let payload: unknown = update.text;
          if (update.kind !== 'coding-approval-closed') {
            try { payload = JSON.parse(update.text); } catch { return; }
          }
          setCodingActivities(previous => {
            const current = previous[update.messageId] ?? { results: [] };
            if (update.kind === 'coding-approval-closed' && current.approval?.id !== payload) return previous;
            const next = update.kind === 'coding-result'
              ? { ...current, results: [...current.results, payload as CodingActivityState['results'][number]] }
              : { ...current, approval: update.kind === 'coding-approval' ? payload as CodingActivityState['approval'] : undefined };
            return { ...previous, [update.messageId]: next };
          });
          return;
        }
        if (update.kind === 'delta' || update.kind === 'reasoning' || update.kind === 'sources' || update.kind === 'tool') {
          const buffer: StreamBuffer = streamBuffers.current.get(update.messageId) || { chatId: update.chatId, text: '', reasoning: '', phase: undefined, activity: '' };
          if (update.kind === 'delta' && update.text) {
            if ((buffer.visible ?? buffer.text.length) >= buffer.text.length) buffer.deadline = performance.now() + streamRevealWindow;
            buffer.visible ??= buffer.text.length;
            buffer.text += update.text; buffer.phase = 'answer'; buffer.activity = '';
          }
          if (update.kind === 'reasoning' && update.text) { buffer.reasoning += update.text; buffer.phase = 'thinking'; buffer.activity = ''; }
          if (update.kind === 'sources') buffer.sources = update.sources;
          if (update.kind === 'tool') buffer.activity = update.text;
          streamBuffers.current.set(update.messageId, buffer);
          queuePaint();
        } else if (['done', 'error', 'stopped'].includes(update.kind)) {
          const buffer = streamBuffers.current.get(update.messageId);
          if (update.kind === 'done' && buffer && buffer.chatId === selectedRef.current && !reducedMotion()
            && (buffer.visible ?? 0) < buffer.text.length) {
            buffer.done = update;
            queuePaint();
          } else {
            finishStream(update);
          }
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
    return () => { mounted.current = false; unlisteners.forEach((unlisten) => unlisten()); if (frame.current !== null) cancelAnimationFrame(frame.current); frame.current = null; };
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
    const outside = (event: MouseEvent) => { if (!(event.target as Element).closest('[data-menu-root]')) { setChatMenu(false); setPicker(null); } };
    // File/URL drops must never navigate the webview. Only ChatList handles an
    // authenticated in-app chat drag; dropping elsewhere is a harmless cancel.
    const preventDropNavigation = (event: DragEvent) => event.preventDefault();
    const escape = (event: globalThis.KeyboardEvent) => { if (event.key === 'Escape') { setChatMenu(false); if (picker) closePicker(); } };
    document.addEventListener('mousedown', outside); document.addEventListener('keydown', escape); document.addEventListener('drop', preventDropNavigation);
    return () => { document.removeEventListener('mousedown', outside); document.removeEventListener('keydown', escape); document.removeEventListener('drop', preventDropNavigation); };
  }, [picker]);

  const selectedChat = data?.chats.find((chat) => chat.id === selectedId);
  const selectedFolder = data?.folders.find((folder) => folder.id === selectedChat?.folderId);
  const visibleProviders = data?.providers.filter((entry) => !nativeApp || entry.id !== 'mock') || [];
  const connectedProviders = visibleProviders.filter((entry) => entry.configured && !entry.disabledReason);
  const defaultModel = { providerId: data?.settings.defaultProvider || '', modelId: data?.settings.defaultModel || '' };
  const chosen = (selectedId ? data?.chatModels?.[selectedId] : draftModel) || defaultModel;
  const provider = visibleProviders.find((entry) => entry.id === chosen.providerId && (!nativeApp || (entry.configured && !entry.disabledReason))) || (!nativeApp ? visibleProviders[0] : undefined);
  const model = provider?.id === chosen.providerId ? chosen.modelId || provider.connection.modelId : provider?.connection.modelId || '';
  const compatible = data?.compatibleConnections || [];
  const namedConnection = compatible.find(item => item.id === provider?.id);
  const displayModel = namedConnection?.models.find(item => item.id === model)?.name || (provider ? modelLabel(provider, model) : '');
  const activeRun = !!selectedId && (running.has(selectedId) || compactingChat === selectedId);
  const explicitFolder = requestsFolderContext(draft);
  const folderContext = !!selectedFolder && !!provider && (manualFolder ?? (explicitFolder || !!selectedFolder.contextEnabled));
  const toggleProjectContext = async () => {
    if (!selectedFolder) return;
    const folderId = selectedFolder.id;
    const enabled = !folderContext;
    try {
      await backend.invoke('set_project_context', { folderId, enabled });
      setData((current) => current && { ...current, folders: current.folders.map((folder) => folder.id === folderId ? { ...folder, contextEnabled: enabled } : folder) });
      if (selectedRef.current === selectedId) setManualFolder(enabled);
    } catch (reason) { setNotice(errorText(reason)); }
  };
  const museRoute = provider?.id === 'muse-api';
  const webAvailable = !!provider && (provider.id === 'mock' || provider.nativeSearch || ((provider.tools || museRoute) && !!data?.searchConfigured));
  const sendAvailable = !!provider?.configured && !provider.disabledReason;

  fileState.current = { selectedId, view, starting, activeRun };
  const fileDiscards = useRef<Promise<void>>(Promise.resolve());
  const discardFiles = (files: AttachmentInfo[]) => { fileDiscards.current = fileDiscards.current.then(async () => { await composer.flush(); await Promise.allSettled(files.map(file => backend.invoke('discard_chat_file', { id: file.id }))); }); };
  const clearAttachment = () => { attachmentEpoch.current += 1; discardFiles(attachments); setAttachments([]); };
  const removeAttachment = (file: AttachmentInfo) => { discardFiles([file]); setAttachments(current => current.filter(entry => entry.id !== file.id)); };
  const receiveFiles = async (command: string, args?: Record<string, unknown>) => {
    const target = selectedRef.current; const epoch = attachmentEpoch.current;
    attachmentOperation.current = true; setPickingAttachment(true);
    try {
      await fileDiscards.current;
      const files = await backend.invoke<AttachmentInfo[]>(command, args);
      if (epoch === attachmentEpoch.current && target === selectedRef.current && fileState.current.view === 'chat') setAttachments(current => [...current, ...files]);
      else discardFiles(files);
    } catch (reason) { setNotice(errorText(reason)); }
    finally { attachmentOperation.current = false; setPickingAttachment(false); }
  };
  const pickAttachment = () => receiveFiles('pick_chat_files');
  useEffect(() => {
    if (!nativeApp) return;
    let disposed = false;
    const cleanup: Array<() => void> = [];
    void Promise.all([
      backend.listen<boolean>('file-drag', entered => { const current = fileState.current; setDraggingFiles(entered && current.view === 'chat' && !current.starting && !current.activeRun && !attachmentOperation.current && !document.querySelector('[role="dialog"]')); }),
      backend.listen<string>('file-drop', ticket => {
        setDraggingFiles(false);
        const current = fileState.current;
        const accept = current.view === 'chat' && !current.starting && !current.activeRun && !attachmentOperation.current && !document.querySelector('[role="dialog"]');
        if (accept) void receiveFiles('accept_file_drop', { ticket, accept: true });
        else { void backend.invoke('accept_file_drop', { ticket, accept: false }).catch(() => {}); setNotice('Open an idle chat to attach files.'); }
      }),
    ]).then(listeners => { if (disposed) listeners.forEach(stop => stop()); else cleanup.push(...listeners); }).catch(reason => setNotice(errorText(reason)));
    return () => { disposed = true; cleanup.forEach(stop => stop()); };
  }, []);

  const selectChat = (chat: Chat, preserveAttachment = false) => {
    setDraftModel(null);
    setDraftContext(null);
    if (chat.id !== selectedRef.current) {
      attachmentEpoch.current += 1;
      if (preserveAttachment) composer.adopt(chat.id);
    }
    selectedRef.current = chat.id; setSelectedId(chat.id);
    setData((current) => current && !current.chats.some((entry) => entry.id === chat.id) ? { ...current, chats: [...current.chats, chat] } : current);
    setView('chat'); setHighlight(null); setNotice(''); followBottom.current = true;
    if (window.innerWidth <= 600) setSidebar(false);
  };
  const createChat = async (folderId: string | null = null, preserveAttachment = false) => {
    const preferredId = selectedRef.current;
    const chat = await backend.invoke<Chat>('create_chat', { folderId, preferredId });
    await refresh();
    if (chat.id === selectedRef.current) {
      setView('chat');
    } else {
      selectChat(chat, preserveAttachment);
    }
    requestAnimationFrame(() => textarea.current?.focus());
    return chat;
  };
  const seed = async () => { const fresh = await backend.invoke<Bootstrap>('demo_seed'); setData(fresh); const chat = fresh.chats.find((entry) => entry.title === 'Interview prep') || fresh.chats[0]; if (chat) selectChat(chat); setWebSearch(true); };
  const saveSettings = async (settings: Settings) => { await backend.invoke('save_settings', { settings }); await refresh(); };
  const chooseContext = async (limit: ContextLimit | null) => {
    if (!selectedId) { setDraftContext(limit); return; }
    savingContext.current = true;
    try {
      await backend.invoke('set_chat_context_limit', { chatId: selectedId, limit });
      setContextRevision(value => value + 1);
    } finally {
      savingContext.current = false;
    }
  };
  const chooseChatModel = async (route: ChatModel) => {
    if (selectedId) { await backend.invoke('save_chat_model', { chatId: selectedId, ...route }); await refresh(); }
    else setDraftModel(route);
    setWebSearch(false);
  };
  const chooseProvider = async (next: ProviderView) => {
    if (!data || (!next.configured && next.id !== 'mock') || next.disabledReason) return;
    const modelId = next.connection.modelId || next.defaultModel;
    if (next.id.startsWith('compatible:') || namedConnection || draftModel || (selectedId && data.chatModels?.[selectedId])) await chooseChatModel({ providerId: next.id, modelId });
    else await saveSettings({ ...data.settings, defaultProvider: next.id, defaultModel: modelId });
    setWebSearch(false); setManualFolder(null); setPicker('models');
  };
  const chooseModel = async (modelId: string) => {
    if (!data || !provider || !modelId.trim()) return;
    await backend.invoke('save_connection', { connection: { ...provider.connection, modelId: modelId.trim() } });
    if (draftModel || (selectedId && data.chatModels?.[selectedId])) await chooseChatModel({ providerId: provider.id, modelId: modelId.trim() });
    else await saveSettings({ ...data.settings, defaultProvider: provider.id, defaultModel: modelId.trim() });
    setWebSearch(false); closePicker();
  };
  const send = async (retry = false) => {
    if (!composer.ready || !data || !provider || !sendAvailable || activeRun || compactingChat !== null || pickingAttachment || savingContext.current || sendingRef.current || (retry && (viewingOlder || loading || attachments.length > 0)) || (!retry && !draft.trim())) return;
    sendingRef.current = true; setStarting(true); setNotice(''); setHighlight(null); followBottom.current = true;
    let chatId = selectedId;
    try {
      await composer.flush();
      if (editing && !retry) {
        const branch = await backend.invoke<Chat>('branch_chat', { chatId: editing.chatId, messageId: editing.id, includeMessage: false });
        chatId = branch.id; composer.adopt(branch.id); setSelectedId(branch.id); await refresh();
      }
      if (!chatId) {
        const chat = await createChat(null, true);
        chatId = chat.id;
        if (draftContext !== null) await backend.invoke('set_chat_context_limit', { chatId, limit: draftContext });
      }
      if (draftModel || namedConnection) await backend.invoke('save_chat_model', { chatId, providerId: provider.id, modelId: model });
      await composer.flush();
      selectedRef.current = chatId;
      setRunning((current) => new Set(current).add(chatId!));
      await backend.invoke<Message>('start_chat', { input: { chatId, content: draft.trim(), providerId: provider.id, modelId: model, webSearch: webSearch && webAvailable, folderContext, retry }, attachmentIds: retry ? [] : attachments.map(file => file.id), draftKey: composer.key });
      if (!retry) { composer.clear(composer.key); if (chatId !== composer.key) composer.clear(chatId); } setManualFolder(null); await loadMessages(chatId); await refresh();
    } catch (reason) { setNotice(errorText(reason)); if (chatId) setRunning((current) => { const next = new Set(current); next.delete(chatId!); return next; }); }
    finally { sendingRef.current = false; setStarting(false); textarea.current?.focus(); }
  };
  const exportChat = (chat: Chat) => { void backend.invoke<boolean>('export_data', { chatId: chat.id, folderId: null }).then(saved => { if (saved) setNotice('Conversation exported.'); }).catch(reason => setNotice(errorText(reason))); };
  const branchMessage = async (message: Message) => {
    if (sendingRef.current) return;
    sendingRef.current = true; setStarting(true);
    try { const branch = await backend.invoke<Chat>('branch_chat', { chatId: message.chatId, messageId: message.id, includeMessage: true }); await refresh(); selectChat(branch); setNotice('New branch created. The original chat is unchanged.'); }
    catch (reason) { setNotice(errorText(reason)); }
    finally { sendingRef.current = false; setStarting(false); }
  };
  const source = async (value: Source) => {
    if (value.kind === 'file') { setFileCitation(value); return; }
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
  const moveChat = async (chat: Chat, folderId: string | null) => {
    if (chat.folderId === folderId || running.has(chat.id) || movingIds.current.has(chat.id)) return;
    movingIds.current.add(chat.id); setMoving(new Set(movingIds.current));
    try {
      await backend.invoke('move_chat', { chatId: chat.id, folderId });
      setData((current) => current && { ...current, chats: current.chats.map((entry) => entry.id === chat.id ? { ...entry, folderId } : entry) });
      // A permission granted for the previous folder never follows a move.
      if (selectedRef.current === chat.id) setManualFolder(false);
      setNotice(`Moved “${chat.title}” to ${data?.folders.find((folder) => folder.id === folderId)?.title || 'Recent'}.`);
      await refresh();
    } catch (reason) { setNotice(errorText(reason)); }
    finally { movingIds.current.delete(chat.id); setMoving(new Set(movingIds.current)); }
  };
  const deleteChat = async (chat: Chat) => {
    await backend.invoke('delete_chat', { chatId: chat.id });
    removedChatIds.current.add(chat.id);
    setData((current) => current && { ...current, chats: current.chats.filter((entry) => entry.id !== chat.id) });
    if (selectedRef.current === chat.id) {
      attachmentEpoch.current += 1; if (selectedId) composer.forget(selectedId); discardFiles(attachments); selectedRef.current = null; setSelectedId(null); setItems([]); setManualFolder(null);
      setActivities({}); setHighlight(null); setHasOlder(false); setViewingOlder(false); ++messageRequest.current;
    }
    setNotice('Chat deleted.');
  };
  const projectChanged = async (folderId: string) => {
    const fresh = await backend.invoke<Bootstrap>('bootstrap');
    const hidden = !fresh.folders.some((folder) => folder.id === folderId);
    const selected = dataRef.current?.chats.find((chat) => chat.id === selectedRef.current);
    if (hidden && selected?.folderId === folderId) {
      attachmentEpoch.current += 1; if (selectedId) composer.forget(selectedId); discardFiles(attachments); selectedRef.current = null; setSelectedId(null); setItems([]); setManualFolder(null);
      setActivities({}); setHighlight(null); setHasOlder(false); setViewingOlder(false); setLoading(false); ++messageRequest.current;
    }
    setData((current) => {
      const selected = current?.chats.find((chat) => chat.id === selectedRef.current);
      return selected && !fresh.chats.some((chat) => chat.id === selected.id) ? { ...fresh, chats: [...fresh.chats, selected] } : fresh;
    });
  };
  const archiveProject = async (folder: Folder) => {
    try { await backend.invoke('archive_project', { folderId: folder.id, archived: true }); await projectChanged(folder.id); }
    catch (reason) { setNotice(errorText(reason)); }
  };
  if (bootError) return <div className="loading-state"><div role="alert">{bootError}<br /><button className="text-action" onClick={() => window.location.reload()}>Try again</button></div></div>;
  if (!data) return <div className="loading-state">Opening Melly…</div>;

  return <div className={`app ${nativeApp ? 'native' : 'browser-preview'} ${sidebar ? '' : 'sidebar-hidden'}`} style={{ '--message-font': `${data.settings.fontSize}px` } as CSSProperties}>
    {sidebar && <aside className="sidebar" aria-label="Chats">
      <div className="chrome" data-tauri-drag-region><span className="preview-brand" data-tauri-drag-region>Melly</span><button className="icon-button" aria-label="Hide sidebar" onClick={() => setSidebar(false)}><Icon name="sidebar" /></button></div>
      <nav className="nav" aria-label="Main"><button className="nav-row" onClick={() => { void createChat().catch((reason) => setNotice(errorText(reason))); }}><Icon name="edit" />New Chat</button><button className={`nav-row ${view === 'settings' ? 'selected' : ''}`} onClick={() => { setView('settings'); setNotice(''); }}><Icon name="settings" />Settings</button>{view === 'chat' && <button className="nav-row chat-search" onClick={() => setSearchOpen(true)}><Icon name="search" />Search chats</button>}</nav>
      {view === 'chat' && <ChatList onExport={exportChat} onArchive={archiveProject} onDeleteProject={setDeleteProject} onArchived={() => setArchivedOpen(true)} onFiles={setProjectFiles} chats={data.chats} folders={data.folders} selectedId={selectedId} busy={new Set([...running, ...moving])}
        onSelect={selectChat} onMove={moveChat} onDelete={setDeleteTarget} onRename={(chat) => setNameAction({ kind: 'rename-chat', id: chat.id, title: chat.title })}
        onNewChat={(folderId) => { void createChat(folderId).catch((reason) => setNotice(errorText(reason))); }} onNewFolder={() => setNameAction({ kind: 'new-folder' })}
        onRenameFolder={(folder) => setNameAction({ kind: 'rename-folder', id: folder.id, title: folder.title })} />}
      {!nativeApp && <div className="sidebar-foot">Browser preview · Mock</div>}
    </aside>}
    <main className="main">
      <header className="main-header" data-tauri-drag-region>
        {!sidebar && <button className="icon-button" aria-label="Show sidebar" onClick={() => setSidebar(true)}><Icon name="sidebar" /></button>}
        {view === 'settings' && <button className="icon-button" aria-label="Back to chat" title="Back to chat" onClick={() => { setView('chat'); setNotice(''); }}><Icon name="right" style={{ transform: 'rotate(180deg)' }} /></button>}
        {view === 'settings' ? <h1 data-tauri-drag-region>Settings</h1> : selectedChat ? <div className="chat-heading" data-tauri-drag-region><span className="chat-title" data-tauri-drag-region>{selectedChat.title}</span><span className="chat-location" data-tauri-drag-region>{selectedFolder?.title || 'No project'}</span></div> : <span />}
        {!sidebar && !nativeApp && <span className="preview-badge">Browser preview · Mock</span>}
        {view === 'chat' && selectedChat && <div className="chat-menu-button" data-menu-root><button className="icon-button" aria-label="Chat options" aria-expanded={chatMenu} onClick={() => setChatMenu(!chatMenu)}><Icon name="more" /></button>{chatMenu && <div className="menu chat-menu" role="menu"><ChatActions onExport={exportChat} chat={selectedChat} folders={data.folders} busy={activeRun || moving.has(selectedChat.id)} onRename={(chat) => setNameAction({ kind: 'rename-chat', id: chat.id, title: chat.title })} onDelete={setDeleteTarget} onMove={moveChat} onClose={() => setChatMenu(false)} /></div>}</div>}
      </header>
      {notice && <div className="notice" role="alert"><span>{notice}</span><button className="icon-button" aria-label="Dismiss notification" onClick={() => setNotice('')}><Icon name="close" className="small" /></button></div>}
      {view === 'settings' ? <SettingsView data={data} theme={theme} onSettings={saveSettings} refresh={refresh} onSeed={seed} report={setNotice} /> : <>
        <div className="conversation-scroll" ref={scroll} onScroll={() => { const element = scroll.current; if (element) followBottom.current = element.scrollHeight - element.scrollTop - element.clientHeight < 100; }}>
          {!items.length && !loading ? <div className="empty-state"><h1>{selectedFolder ? `A new thought in ${selectedFolder.title}` : 'What’s on your mind?'}</h1>{!nativeApp && provider?.id === 'mock' && <><p>Try a synthetic conversation, or connect a provider in Settings.</p><button className="text-action" onClick={() => { void seed().catch((reason) => setNotice(errorText(reason))); }}>Try an example</button></>}</div> : <div className={`transcript ${hasOlder || viewingOlder ? 'has-older' : ''}`} aria-label="Conversation">
            {hasOlder && <button className="load-older" disabled={loading} onClick={() => { void older(); }}>{loading ? 'Loading…' : 'Load older messages'}</button>}
            {viewingOlder && <button className="load-older" onClick={() => { if (selectedId) { setHighlight(null); followBottom.current = true; void loadMessages(selectedId); } }}>Back to latest messages</button>}
            {items.map((message, index) => <MessageView key={message.id} message={message} theme={theme} activity={activities[message.id]} codingActivity={codingActivities[message.id]} highlight={highlight === message.id} onSource={(value) => { void source(value).catch((reason) => setNotice(errorText(reason))); }} onRetry={() => { void send(true); }} onBranch={() => { void branchMessage(message); }} onEdit={() => { clearAttachment(); setEditing(message); setDraft(message.content); if (message.attachments?.length) void receiveFiles('copy_message_files', { messageId: message.id }); textarea.current?.focus(); }} canChange={!activeRun && !starting && !loading && !pickingAttachment} retryEnabled={index === items.length - 1 && !viewingOlder && !loading && !activeRun && !starting} report={setNotice} />)}
          </div>}
        </div>
        <div className="composer-area" data-menu-root>
          {draggingFiles && <div className="file-drop-overlay" aria-hidden="true"><Icon name="plus" /><span>Add files to this conversation</span><small>Shared when you send</small></div>}
          {selectedFolder && folderContext && <div className="folder-consent"><span>Project files and matching chats may be sent to {provider ? providerName(provider) : 'the selected provider'}.</span></div>}
          {!selectedFolder && explicitFolder && <div className="folder-consent"><Icon name="folder" /><span>Move this chat into a project to use its files and other chats.</span></div>}
          {editing && <div className="editing-note"><span>Editing in a new branch</span><button onClick={() => { clearAttachment(); setEditing(null); setDraft(''); }}>Cancel</button></div>}
          <form className="composer" onSubmit={(event) => { event.preventDefault(); void send(); }}>
            {!!attachments.length && <div className="attachment-list">{attachments.map(file => <div className="attachment-draft" key={file.id}><span title={file.name}>{file.name}{file.kind === 'image-text' && <small>Recognized text</small>}</span><button type="button" className="icon-button" aria-label={`Remove ${file.name}`} disabled={starting} onClick={() => removeAttachment(file)}><Icon name="close" /></button></div>)}</div>}
            {pickingAttachment && <div className="attachment-progress" role="status">Reading files on your Mac…</div>}

            <textarea ref={textarea} aria-label="Message" rows={1} placeholder="Send a message" disabled={!composer.ready || starting} value={draft} onChange={(event) => setDraft(event.target.value)} onKeyDown={(event) => {
              if (event.nativeEvent.isComposing || event.key !== 'Enter') return;
              const shouldSend = data.settings.sendBehavior === 'cmd-enter' ? event.metaKey || event.ctrlKey : !event.shiftKey && !event.altKey;
              if (shouldSend) { event.preventDefault(); void send(); }
            }} />
            <div className="composer-controls">
              {nativeApp && selectedId && selectedFolder && !projectFiles && <FileEditMode key={selectedId} chatId={selectedId} projectId={selectedFolder.id} report={setNotice} />}
              {nativeApp && <button type="button" className="round-control" aria-label="Attach files" title="Text, PDF, or text from images · up to 8 files" disabled={starting || activeRun || pickingAttachment} onClick={() => { void pickAttachment(); }}><Icon name="plus" /></button>}
              {selectedFolder && <button type="button" className="round-control" aria-label="Project context" aria-pressed={folderContext} title={`Use files and other chats in ${selectedFolder.title}`} onClick={() => { void toggleProjectContext(); }}><Icon name="folder" /></button>}
              <button type="button" className="round-control" aria-label="Search the web" aria-pressed={webSearch && webAvailable} aria-disabled={!webAvailable} title={webAvailable ? (provider?.id === 'mock' ? 'Synthetic web search in Demo' : provider?.nativeSearch ? 'Use built-in web search' : 'Use external web search') : 'Set up external search in Settings'} onClick={() => { if (webAvailable) setWebSearch(!webSearch); else setNotice(provider?.tools || museRoute ? 'Add an external search key in Settings to use web search with this provider.' : 'Web search is not available for this provider.'); }}><Icon name="globe" /></button>
              {provider ? <div className="model-pill" role="group" aria-label="Provider and model">
                <button ref={providerButton} type="button" className="provider-picker-trigger" aria-label="Choose provider" title={`${providerName(provider)} · choose provider`} aria-expanded={picker === 'providers'} aria-haspopup="listbox" onClick={() => setPicker(picker === 'providers' ? null : 'providers')}><ProviderIcon id={provider.id} theme={theme} small /></button>
                <span className="provider-divider" aria-hidden="true" />
                <button ref={modelButton} type="button" className="model-picker-trigger" aria-label="Choose model" title="Choose model" aria-expanded={picker === 'models'} aria-haspopup="listbox" onClick={() => setPicker(picker === 'models' ? null : 'models')}><span className="model-name">{displayModel}</span><span className="divider" /><span className="route">{routeLabel(provider)}</span><Icon name="down" /></button>
              </div> : <button ref={providerButton} type="button" className="button" onClick={() => { if (connectedProviders.length) setPicker('providers'); else setView('settings'); }}>{connectedProviders.length ? 'Choose provider' : 'Connect provider'}</button>}
              {provider && <ContextMeter onCompacting={setCompactingChat} draftContext={draftContext} contextRevision={contextRevision} onLimit={chooseContext} chatId={selectedId} providerId={provider.id} modelId={model} draft={draft} attachments={attachments} folderContext={folderContext} revision={selectedChat?.updatedAt || 0} busy={activeRun || starting || pickingAttachment || compactingChat !== null} onSource={value => { void source(value).catch(reason => setNotice(errorText(reason))); }} />}
              {activeRun ? <button type="button" className="round-control stop" aria-label="Stop response" title="Stop response" onClick={() => { if (selectedId) void backend.invoke('stop_chat', { chatId: selectedId }).catch((reason) => setNotice(errorText(reason))); }}><span className="stop-square" /></button> : <button type="submit" className="round-control send" aria-label="Send message" disabled={!composer.ready || !draft.trim() || starting || compactingChat !== null || pickingAttachment || !sendAvailable}><Icon name="up" /></button>}
            </div>
          </form>
          {!sendAvailable && provider && <p className="composer-note">{providerName(provider)} is not connected. <button onClick={() => setView('settings')}>Open Settings</button></p>}
          {picker === 'providers' && <ProviderPicker providers={visibleProviders} selected={provider?.id || ''} theme={theme} close={closePicker} choose={chooseProvider} settings={() => { setPicker(null); setView('settings'); }} />}
          {picker === 'models' && provider && (namedConnection
            ? <CompatibleModelPicker connections={compatible} selected={{ providerId: provider.id, modelId: model }} close={closePicker} choose={async route => { await chooseChatModel(route); closePicker(); }} />
            : <ModelPicker key={provider.id} provider={provider} selected={model} close={closePicker} choose={chooseModel} />)}
        </div>
      </>}
    </main>
    {archivedOpen && <ArchivedProjects onClose={() => setArchivedOpen(false)} onChanged={refresh} onDelete={setDeleteProject} />}
    {deleteProject && <DeleteProject project={deleteProject} onClose={() => setDeleteProject(null)} onChanged={() => projectChanged(deleteProject.id)} />}
    {fileCitation && <FileCitation source={fileCitation} onClose={() => setFileCitation(null)} />}
    {projectFiles && <ProjectFiles project={projectFiles} onClose={() => setProjectFiles(null)} />}
    {searchOpen && <SearchDialog folders={data.folders} onClose={() => setSearchOpen(false)} onSelect={selectChat} />}
    {nameAction && <NameDialog title={nameAction.kind === 'new-folder' ? 'New project' : nameAction.kind === 'rename-folder' ? 'Rename project' : 'Rename chat'} initial={nameAction.title} action={nameAction.kind === 'new-folder' ? 'Create' : 'Save'} onClose={() => setNameAction(null)} onSave={nameSave} />}
    {deleteTarget && <DeleteChatDialog chat={deleteTarget} busy={running.has(deleteTarget.id) || moving.has(deleteTarget.id)} onClose={() => setDeleteTarget(null)} onDelete={deleteChat} />}
  </div>;
}
