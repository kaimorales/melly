import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { useChatDrag } from './useChatDrag';
import { ChatActionsMenu } from './ChatActions';
import { Icon } from './icons';
import type { Chat, Folder } from './types';

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
  const list = useRef<HTMLDivElement>(null);
  const reveal = (folderId: string) => setCollapsed((current) => { const next = new Set(current); next.delete(folderId); return next; });
  const { dragging, destination, preview, start, consumeClick } = useChatDrag({ list, chats, busy, collapsed, reveal, onMove, onStart: () => { setMenu(null); setFolderMenu(null); } });
  useEffect(() => {
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape') setFolderMenu(null); };
    const outside = (event: MouseEvent) => { if (!(event.target as Element).closest('[data-menu-root]')) setFolderMenu(null); };
    document.addEventListener('keydown', escape); document.addEventListener('mousedown', outside);
    return () => { document.removeEventListener('keydown', escape); document.removeEventListener('mousedown', outside); };
  }, []);
  const row = (chat: Chat, nested = false) => <div key={chat.id} className={`chat-row-wrap ${nested ? 'nested' : ''} ${chat.id === dragging ? 'dragging' : ''} ${menu?.id === chat.id ? 'menu-open' : ''}`}>
    <button aria-label={chat.title} className={`${nested ? 'chat-row' : 'recent-row'} ${chat.id === selectedId ? 'selected' : ''}`} title={busy.has(chat.id) ? `${chat.title} · Stop the response before moving` : chat.title} draggable={false} onPointerDown={(event) => start(event, chat)} onClick={() => { if (!consumeClick()) onSelect(chat); }} onContextMenu={(event) => {
      event.preventDefault(); setFolderMenu(null); setMenu({ id: chat.id, anchor: event.currentTarget, point: { x: event.clientX, y: event.clientY } });
    }} onKeyDown={(event) => {
      if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) { event.preventDefault(); setMenu({ id: chat.id, anchor: event.currentTarget }); }
    }}><span>{chat.title}</span>{busy.has(chat.id) && <span className="sr-only">Responding</span>}</button>
    <button className="icon-button chat-row-options" aria-label={`Chat options for ${chat.title}`} aria-expanded={menu?.id === chat.id} aria-haspopup="menu" onClick={(event) => {
      setFolderMenu(null); setMenu(menu?.id === chat.id ? null : { id: chat.id, anchor: event.currentTarget });
    }}><Icon name="more" className="small" /></button>
  </div>;
  const menuChat = menu && chats.find((chat) => chat.id === menu.id);
  const destinationTitle = destination === '' ? 'Recent' : folders.find((folder) => folder.id === destination)?.title;
  return <div ref={list} className={`sidebar-list ${dragging ? 'is-dragging' : ''}`} onScroll={() => setMenu(null)}>
    <div className="section-label"><span>Folders</span><button className="icon-button" aria-label="New folder" title="New folder" onClick={onNewFolder}><Icon name="plus" className="small" /></button></div>
    {!folders.length && <p className="sidebar-empty">Keep related chats together.</p>}
    {folders.map((folder) => <div className="folder-wrap" key={folder.id} data-menu-root data-drop-folder={folder.id} data-drop-active={destination === folder.id}>
      <button className="folder" aria-expanded={!collapsed.has(folder.id)} onClick={() => setCollapsed((current) => { const next = new Set(current); if (next.has(folder.id)) next.delete(folder.id); else next.add(folder.id); return next; })}>
        <Icon name={collapsed.has(folder.id) ? 'chevron' : 'down'} className="chevron" /><Icon name="folder" className="folder-icon" /><span>{folder.title}</span>
      </button>
      <button className="icon-button folder-options" aria-label={`Folder options for ${folder.title}`} onClick={() => { setMenu(null); setFolderMenu(folderMenu === folder.id ? null : folder.id); }}><Icon name="more" className="small" /></button>
      {folderMenu === folder.id && <div className="menu folder-menu" role="menu"><button role="menuitem" className="menu-item" onClick={() => { setFolderMenu(null); onNewChat(folder.id); }}>New chat in folder</button><button role="menuitem" className="menu-item" onClick={() => { setFolderMenu(null); onRenameFolder(folder); }}>Rename folder</button></div>}
      {!collapsed.has(folder.id) && chats.filter((chat) => chat.folderId === folder.id).map((chat) => row(chat, true))}
    </div>)}
    <div className="recent-group" role="group" aria-label="Recent chats" data-drop-folder="" data-drop-active={destination === ''}>
      <div className="section-label"><span>Recent</span>{dragging && <span className="drop-hint">No folder</span>}</div>
      {chats.filter((chat) => !chat.folderId).map((chat) => row(chat))}
      {!chats.some((chat) => !chat.folderId) && <p className="sidebar-empty">{dragging ? 'Drop here to remove from folder' : 'Your recent chats appear here.'}</p>}
    </div>
    {dragging && createPortal(<div ref={preview} className="chat-drag-preview" aria-hidden="true"><Icon name="chat" /><span>{chats.find((chat) => chat.id === dragging)?.title}</span></div>, document.body)}
    <span className="sr-only" role="status">{dragging ? destinationTitle ? `Move to ${destinationTitle}` : 'Drag onto a folder, or Recent to remove from a folder. Escape cancels.' : ''}</span>
    {menu && menuChat && <ChatActionsMenu chat={menuChat} folders={folders} busy={busy.has(menuChat.id)} anchor={menu.anchor} point={menu.point} onRename={onRename} onDelete={onDelete} onMove={onMove} onClose={() => setMenu(null)} />}
  </div>;
}
