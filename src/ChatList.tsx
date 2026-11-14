import { useEffect, useRef, useState, type DragEvent } from 'react';
import { ChatActionsMenu } from './ChatActions';
import { Icon } from './icons';
import type { Chat, Folder } from './types';

const CHAT_TRANSFER = 'application/x-melly-chat';
type Props = {
  chats: Chat[]; folders: Folder[]; selectedId: string | null; busy: Set<string>;
  onSelect: (chat: Chat) => void; onMove: (chat: Chat, folderId: string | null) => void;
  onDelete: (chat: Chat) => void; onRename: (chat: Chat) => void;
  onNewChat: (folderId: string) => void; onNewFolder: () => void; onRenameFolder: (folder: Folder) => void;
};

export function ChatList({ chats, folders, selectedId, busy, onSelect, onMove, onDelete, onRename, onNewChat, onNewFolder, onRenameFolder }: Props) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [folderMenu, setFolderMenu] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ id: string; anchor: HTMLElement; point?: { x: number; y: number } } | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const [destination, setDestination] = useState<string | null>(null);
  const dragId = useRef<string | null>(null); const target = useRef<string | null>(null);
  const expand = useRef<ReturnType<typeof setTimeout> | null>(null);
  const list = useRef<HTMLDivElement>(null); const scrollFrame = useRef<number | null>(null); const scrollSpeed = useRef(0);
  const stopScroll = () => { if (scrollFrame.current !== null) cancelAnimationFrame(scrollFrame.current); scrollFrame.current = null; scrollSpeed.current = 0; };
  const stopExpand = () => { if (expand.current !== null) clearTimeout(expand.current); expand.current = null; };
  const finish = () => { dragId.current = null; target.current = null; setDragging(null); setDestination(null); stopExpand(); stopScroll(); };
  useEffect(() => {
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape') { finish(); setFolderMenu(null); } };
    const outside = (event: MouseEvent) => { if (!(event.target as Element).closest('[data-menu-root]')) setFolderMenu(null); };
    window.addEventListener('blur', finish); document.addEventListener('keydown', escape); document.addEventListener('mousedown', outside);
    return () => { stopExpand(); stopScroll(); window.removeEventListener('blur', finish); document.removeEventListener('keydown', escape); document.removeEventListener('mousedown', outside); };
  }, []);
  const dragged = () => chats.find((chat) => chat.id === dragId.current && !busy.has(chat.id));
  const over = (event: DragEvent, folderId: string | null) => {
    const chat = dragged(); if (!chat) return;
    event.preventDefault(); event.dataTransfer.dropEffect = chat.folderId === folderId ? 'none' : 'move';
    const id = folderId || '';
    if (target.current !== id) {
      stopExpand(); target.current = id; setDestination(id);
      if (folderId && collapsed.has(folderId)) expand.current = setTimeout(() => {
        expand.current = null;
        setCollapsed((current) => { const next = new Set(current); next.delete(folderId); return next; });
      }, 450);
    }
  };
  const drop = (event: DragEvent, folderId: string | null) => {
    event.preventDefault(); event.stopPropagation();
    const chat = dragged();
    const valid = chat && event.dataTransfer.getData(CHAT_TRANSFER) === chat.id;
    finish();
    if (!valid || chat.folderId === folderId) return;
    if (folderId) setCollapsed((current) => { const next = new Set(current); next.delete(folderId); return next; });
    onMove(chat, folderId);
  };
  const leave = (event: DragEvent) => {
    if (event.currentTarget.contains(event.relatedTarget as Node | null)) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    if (event.clientX > bounds.left && event.clientX < bounds.right && event.clientY > bounds.top && event.clientY < bounds.bottom) return;
    target.current = null; setDestination(null); stopExpand();
  };
  const autoScroll = (event: DragEvent) => {
    if (!dragId.current || !list.current) return;
    const bounds = list.current.getBoundingClientRect(); const margin = 36;
    scrollSpeed.current = event.clientY < bounds.top + margin ? -Math.min(9, (bounds.top + margin - event.clientY) / 4) : event.clientY > bounds.bottom - margin ? Math.min(9, (event.clientY - bounds.bottom + margin) / 4) : 0;
    if (!scrollSpeed.current) { stopScroll(); return; }
    if (scrollFrame.current !== null) return;
    const step = () => {
      if (!dragId.current || !list.current || !scrollSpeed.current) { stopScroll(); return; }
      list.current.scrollTop += scrollSpeed.current;
      scrollFrame.current = requestAnimationFrame(step);
    };
    scrollFrame.current = requestAnimationFrame(step);
  };
  const row = (chat: Chat, nested = false) => <div key={chat.id} className={`chat-row-wrap ${nested ? 'nested' : ''} ${chat.id === dragging ? 'dragging' : ''} ${menu?.id === chat.id ? 'menu-open' : ''}`}>
    <button aria-label={chat.title} className={`${nested ? 'chat-row' : 'recent-row'} ${chat.id === selectedId ? 'selected' : ''}`} title={busy.has(chat.id) ? `${chat.title} · Stop the response before moving` : chat.title} draggable={!busy.has(chat.id)} onClick={() => onSelect(chat)} onContextMenu={(event) => {
      event.preventDefault(); setFolderMenu(null); setMenu({ id: chat.id, anchor: event.currentTarget, point: { x: event.clientX, y: event.clientY } });
    }} onKeyDown={(event) => {
      if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) { event.preventDefault(); setMenu({ id: chat.id, anchor: event.currentTarget }); }
    }} onDragStart={(event) => {
      if (busy.has(chat.id)) { event.preventDefault(); return; }
      event.dataTransfer.effectAllowed = 'move'; event.dataTransfer.setData(CHAT_TRANSFER, chat.id);
      dragId.current = chat.id; setDragging(chat.id); setMenu(null); setFolderMenu(null);
    }} onDragEnd={finish}><span>{chat.title}</span>{busy.has(chat.id) && <span className="sr-only">Responding</span>}</button>
    <button className="icon-button chat-row-options" aria-label={`Chat options for ${chat.title}`} aria-expanded={menu?.id === chat.id} aria-haspopup="menu" onClick={(event) => {
      setFolderMenu(null); setMenu(menu?.id === chat.id ? null : { id: chat.id, anchor: event.currentTarget });
    }}><Icon name="more" className="small" /></button>
  </div>;
  const menuChat = menu && chats.find((chat) => chat.id === menu.id);
  const destinationTitle = destination === '' ? 'Recent' : folders.find((folder) => folder.id === destination)?.title;
  return <div ref={list} className={`sidebar-list ${dragging ? 'is-dragging' : ''}`} onDragOver={autoScroll} onDragLeave={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) stopScroll(); }} onScroll={() => setMenu(null)}>
    <div className="section-label"><span>Folders</span><button className="icon-button" aria-label="New folder" title="New folder" onClick={onNewFolder}><Icon name="plus" className="small" /></button></div>
    {!folders.length && <p className="sidebar-empty">Keep related chats together.</p>}
    {folders.map((folder) => <div className="folder-wrap" key={folder.id} data-menu-root data-drop-active={destination === folder.id} onDragOver={(event) => over(event, folder.id)} onDragLeave={leave} onDrop={(event) => drop(event, folder.id)}>
      <button className="folder" aria-expanded={!collapsed.has(folder.id)} onClick={() => setCollapsed((current) => { const next = new Set(current); if (next.has(folder.id)) next.delete(folder.id); else next.add(folder.id); return next; })}>
        <Icon name={collapsed.has(folder.id) ? 'chevron' : 'down'} className="chevron" /><Icon name="folder" className="folder-icon" /><span>{folder.title}</span>
      </button>
      <button className="icon-button folder-options" aria-label={`Folder options for ${folder.title}`} onClick={() => { setMenu(null); setFolderMenu(folderMenu === folder.id ? null : folder.id); }}><Icon name="more" className="small" /></button>
      {folderMenu === folder.id && <div className="menu folder-menu" role="menu"><button role="menuitem" className="menu-item" onClick={() => { setFolderMenu(null); onNewChat(folder.id); }}>New chat in folder</button><button role="menuitem" className="menu-item" onClick={() => { setFolderMenu(null); onRenameFolder(folder); }}>Rename folder</button></div>}
      {!collapsed.has(folder.id) && chats.filter((chat) => chat.folderId === folder.id).map((chat) => row(chat, true))}
    </div>)}
    <div className="recent-group" role="group" aria-label="Recent chats" data-drop-active={destination === ''} onDragOver={(event) => over(event, null)} onDragLeave={leave} onDrop={(event) => drop(event, null)}>
      <div className="section-label"><span>Recent</span>{dragging && <span className="drop-hint">No folder</span>}</div>
      {chats.filter((chat) => !chat.folderId).map((chat) => row(chat))}
      {!chats.some((chat) => !chat.folderId) && <p className="sidebar-empty">{dragging ? 'Drop here to remove from folder' : 'Your recent chats appear here.'}</p>}
    </div>
    <span className="sr-only" role="status">{dragging ? destinationTitle ? `Move to ${destinationTitle}` : 'Drag onto a folder, or Recent to remove from a folder. Escape cancels.' : ''}</span>
    {menu && menuChat && <ChatActionsMenu chat={menuChat} folders={folders} busy={busy.has(menuChat.id)} anchor={menu.anchor} point={menu.point} onRename={onRename} onDelete={onDelete} onMove={onMove} onClose={() => setMenu(null)} />}
  </div>;
}
