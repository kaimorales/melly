import './projects.css';
import { useEffect, useRef, useState } from 'react';
import { backend } from './backend';
import { Dialog } from './dialogs';
import type { Folder, Source } from './types';

export interface ProjectFile { id: string; folderId: string; name: string; bytes: number; revision: string; linkedPath: string | null }
interface ProjectLink { folderId: string; name: string; status: string }
export function ProjectFiles({ project, onClose }: { project: Folder; onClose: () => void }) {
  const pending = useRef(false);
  const [operation, setOperation] = useState('');
  const [link, setLink] = useState<ProjectLink | null>(null);
  const [files, setFiles] = useState<ProjectFile[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [removing, setRemoving] = useState<ProjectFile | null>(null);
  useEffect(() => {
    let current = true;
    Promise.all([backend.invoke<ProjectFile[]>('project_files', { folderId: project.id }), backend.invoke<ProjectLink | null>('project_link', { folderId: project.id })]).then(([files, link]) => { if (current) { setFiles(files); setLink(link); } }).catch((reason) => { if (current) setError(String(reason)); });
    return () => { current = false; void backend.invoke('cancel_project_scan', { folderId: project.id }).catch(() => {}); };
  }, [project.id]);
  const act = async (command: string, args: Record<string, unknown>) => {
    if (pending.current) return false;
    pending.current = true; setOperation(command); setBusy(true); setError('');
    try {
      await backend.invoke(command, args);
      const [files, link] = await Promise.all([backend.invoke<ProjectFile[]>('project_files', { folderId: project.id }), backend.invoke<ProjectLink | null>('project_link', { folderId: project.id })]);
      setFiles(files); setLink(link);
      return true;
    } catch (reason) {
      setError(String(reason));
      try { setLink(await backend.invoke<ProjectLink | null>('project_link', { folderId: project.id })); setFiles(await backend.invoke<ProjectFile[]>('project_files', { folderId: project.id })); } catch { /* Keep the operation error visible. */ }
      return false;
    } finally { pending.current = false; setBusy(false); setOperation(''); }
  };
  return <Dialog title={`${project.title} · Files`} onClose={onClose} className="project-dialog">
    <p className="project-note">Add copies of text, Markdown, PDFs, or recognized text from images. Enable Project context to use matching excerpts.</p>
    <h3 className="project-section-title">Project files</h3>
    <div className="project-file-list">
      {files.filter((file) => !file.linkedPath).map((file) => <div className="project-file" key={file.id}><button className="project-file-name" disabled={busy} onClick={() => { void act('open_project_file', { fileId: file.id }); }}>{file.name}</button><small>{Math.max(1, Math.ceil(file.bytes / 1024))} KB</small><div className="project-file-actions"><button disabled={busy} aria-label={`Replace ${file.name}`} onClick={() => { void act('import_project_file', { folderId: project.id, replaceFileId: file.id }); }}>Replace</button><button disabled={busy} aria-label={`Remove ${file.name}`} onClick={() => setRemoving(file)}>Remove</button></div></div>)}
      {!files.some((file) => !file.linkedPath) && <p className="project-note">Add files to keep them with this project.</p>}
    </div>
    {removing && <div className="file-confirm" role="alert"><p>Move {removing.name} to Trash? Its project citations will become unavailable. The original file stays where it is.</p><div className="project-link-actions"><button className="button quiet" disabled={busy} onClick={() => setRemoving(null)}>Cancel</button><button className="button" disabled={busy} onClick={() => { void act('remove_project_file', { fileId: removing.id }).then((ok) => { if (ok) setRemoving(null); }); }}>Move to Trash</button></div></div>}
    <h3 className="project-section-title">Linked folder</h3>
    {link ? <><p className="project-note">{link.name} · Files stay in place<br />{link.status}</p><div className="project-file-list linked-file-list">{files.filter((file) => file.linkedPath).map((file) => <button className="project-file" key={file.id} disabled={busy} onClick={() => { void act('open_project_file', { fileId: file.id }); }}><span>{file.linkedPath}</span></button>)}</div><div className="project-link-actions"><button className="button" disabled={busy} onClick={() => { void act('refresh_linked_project', { folderId: project.id }); }}>Refresh files</button><button className="button quiet" disabled={busy} onClick={() => { void act('link_project_directory', { folderId: project.id }); }}>Reconnect…</button><button className="button quiet" disabled={busy} onClick={() => { void act('unlink_project', { folderId: project.id }); }}>Unlink</button></div></> : <><p className="project-note">Use text, code, and PDFs in an existing folder. Hidden files, credentials, and dependency folders are excluded. Relevant text refreshes when used.</p><button className="button" disabled={busy} onClick={() => { void act('link_project_directory', { folderId: project.id }); }}>Link folder…</button></>}
    {error && <p className="dialog-error" role="alert">{error}</p>}
    {busy && ['link_project_directory', 'refresh_linked_project'].includes(operation) && <button className="text-action" onClick={() => { void backend.invoke('cancel_project_scan', { folderId: project.id }); }}>Cancel indexing</button>}
    <div className="dialog-actions"><button className="button quiet" disabled={busy} onClick={() => { void act('export_data', { chatId: null, folderId: project.id }); }}>Export project…</button><button className="button quiet" disabled={busy} onClick={() => { void act('show_project_directory', { folderId: project.id }); }}>Show in Finder</button><button className="button primary" disabled={busy} onClick={() => { void act('import_project_file', { folderId: project.id }); }}>{busy ? 'Working…' : 'Add file'}</button></div>
  </Dialog>;
}

interface FileSource { fileId: string; title: string; page: number | null; text: string }
export function FileCitation({ source, onClose }: { source: Source; onClose: () => void }) {
  const [passage, setPassage] = useState<FileSource | null>(null);
  const [error, setError] = useState('');
  useEffect(() => {
    let current = true;
    backend.invoke<FileSource>('file_source', { sourceId: source.id }).then((value) => { if (current) setPassage(value); }).catch((reason) => { if (current) setError(String(reason)); });
    return () => { current = false; };
  }, [source.id]);
  return <Dialog title={passage?.title || source.title} onClose={onClose} className="project-dialog">
    {error && <p className="dialog-error" role="alert">{error}</p>}
    {passage && <><pre className="file-passage">{passage.text}</pre><div className="dialog-actions"><button className="button" onClick={() => { void backend.invoke(passage.page ? 'open_pdf_source' : 'open_project_file', passage.page ? { sourceId: source.id } : { fileId: passage.fileId }).catch((reason) => setError(String(reason))); }}>{passage.page ? 'View PDF page' : 'Open file'}</button></div></>}
  </Dialog>;
}

export function DeleteProject({ project, onClose, onChanged }: { project: Folder; onClose: () => void; onChanged: () => Promise<unknown> }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const pending = useRef(false);
  const remove = async () => {
    if (pending.current) return;
    pending.current = true; setBusy(true); setError('');
    try { await backend.invoke('delete_project', { folderId: project.id }); await onChanged(); onClose(); }
    catch (reason) { setError(String(reason)); try { await onChanged(); } catch { /* Retain the deletion error for retry. */ } }
    finally { pending.current = false; setBusy(false); }
  };
  return <Dialog title={`Delete ${project.title}?`} onClose={() => { if (!busy) onClose(); }}>
    <p>Delete all chats in this project and move its imported files to Finder Trash? Chat deletion is permanent. Externally linked files stay in their original folder.</p>
    {error && <p className="dialog-error" role="alert">{error}</p>}
    <div className="dialog-actions"><button className="button quiet" disabled={busy} onClick={onClose}>Cancel</button><button className="button danger" disabled={busy} onClick={() => { void remove(); }}>{busy ? 'Deleting…' : 'Delete project'}</button></div>
  </Dialog>;
}
export function ArchivedProjects({ onClose, onChanged, onDelete }: { onClose: () => void; onChanged: () => Promise<unknown>; onDelete: (project: Folder) => void }) {
  const [projects, setProjects] = useState<Folder[]>([]);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let current = true;
    backend.invoke<Folder[]>('archived_projects').then((value) => { if (current) setProjects(value); }).catch((reason) => { if (current) setError(String(reason)); });
    return () => { current = false; };
  }, []);
  const restore = async (project: Folder) => {
    setBusy(true); setError('');
    try { await backend.invoke('archive_project', { folderId: project.id, archived: false }); setProjects((current) => current.filter((p) => p.id !== project.id)); await onChanged(); }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  };
  return <Dialog title="Archived projects" onClose={onClose} className="project-dialog">
    {!projects.length && <p className="project-note">Archived projects will appear here.</p>}
    {projects.map((project) => <div className="project-file" key={project.id}><span>{project.title}{project.deleting && <small> · Deletion pending</small>}</span>{!project.deleting && <button className="button" disabled={busy} onClick={() => { void restore(project); }}>Restore</button>}<button className="button quiet" disabled={busy} onClick={() => { onClose(); onDelete(project); }}>{project.deleting ? 'Retry deletion' : 'Delete…'}</button></div>)}
    {error && <p className="dialog-error" role="alert">{error}</p>}
  </Dialog>;
}
