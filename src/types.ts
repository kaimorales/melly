// JSON counterparts of crates/melly-core/src/models.rs. Rust owns all persisted state.
export interface Folder { id: string; title: string }
export interface Chat { id: string; title: string; folderId: string | null; updatedAt: number }
export interface Source { id: string; kind: string; title: string; url: string | null; chatId: string | null; messageId: string | null; snippet: string }
export interface Message {
  id: string; chatId: string; role: string; content: string; status: string; sources: Source[];
  createdAt: number; providerId: string | null; modelId: string | null; billingRoute: string | null;
}
export interface Settings {
  theme: string; fontSize: number; systemPrompt: string; sendBehavior: string;
  defaultProvider: string; defaultModel: string; reduceMotion: boolean;
}
export interface Connection { providerId: string; baseUrl: string; modelId: string }
export interface ProviderSpec {
  id: string; name: string; billingRoute: string; baseUrl: string; defaultModel: string;
  protocol: string; nativeSearch: boolean; tools: boolean; disabledReason: string | null;
}
export interface ProviderView extends ProviderSpec { configured: boolean; connection: Connection; status: string }
export interface ModelInfo { id: string; name: string }
export interface Bootstrap { folders: Folder[]; chats: Chat[]; settings: Settings; providers: ProviderView[]; searchConfigured: boolean }
export interface SendInput { chatId: string; content: string; providerId: string; modelId: string; webSearch: boolean; folderContext: boolean; retry: boolean }
export interface StreamUpdate { chatId: string; messageId: string; kind: string; text: string; sources: Source[] }
export type Unlisten = () => void;
export interface Backend {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: string, callback: (payload: T) => void): Promise<Unlisten>;
}
