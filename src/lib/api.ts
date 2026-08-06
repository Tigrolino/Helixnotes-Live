import { invoke } from "@tauri-apps/api/core";
import type {
  AppConfig,
  CustomTheme,
  NoteContent,
  NoteEntry,
  NoteMeta,
  NotebookEntry,
  NoteTitleEntry,
  SearchResult,
  TrashContents,
  VaultState,
  VaultStats,
  ImportResult,
  BackupEntry,
  VersionEntry,
  TaskItem,
  ExternalVaultResult,
  StartupView,
} from "./types";

export async function openVault(path: string): Promise<void> {
  return invoke("open_vault", { path });
}

export async function chooseExternalVault(): Promise<ExternalVaultResult | null> {
  return invoke("choose_external_vault");
}

export async function restoreExternalVault(bookmarkId: string): Promise<ExternalVaultResult> {
  return invoke("restore_external_vault", { bookmarkId });
}

export async function removeVault(path: string, bookmarkId?: string | null): Promise<void> {
  return invoke("remove_vault", { path, bookmarkId: bookmarkId ?? null });
}

export async function getAppConfig(): Promise<AppConfig> {
  return invoke("get_app_config");
}

export async function setTheme(theme: string): Promise<void> {
  return invoke("set_theme", { theme });
}

export async function setSystemThemes(light: string, dark: string): Promise<void> {
  return invoke("set_system_themes", { light, dark });
}

export async function setAccentColor(color: string): Promise<void> {
  return invoke("set_accent_color", { color });
}

export async function saveCustomTheme(theme: CustomTheme): Promise<void> {
  return invoke("save_custom_theme", { theme });
}

export async function deleteCustomTheme(id: string): Promise<void> {
  return invoke("delete_custom_theme", { id });
}

export async function exportCustomTheme(id: string, path: string): Promise<void> {
  return invoke("export_custom_theme", { id, path });
}

export async function importCustomThemes(path: string): Promise<CustomTheme[]> {
  return invoke("import_custom_themes", { path });
}

export async function setFontSize(size: number): Promise<void> {
  return invoke("set_font_size", { size });
}

export async function setFontFamily(family: string): Promise<void> {
  return invoke("set_font_family", { family });
}

export async function setLineHeight(height: number): Promise<void> {
  return invoke("set_line_height", { height });
}

export async function setUiScale(scale: number): Promise<void> {
  return invoke("set_ui_scale", { scale });
}

export async function setContentWidth(width: number | null): Promise<void> {
  return invoke("set_content_width", { width });
}

export async function getNotebooks(): Promise<NotebookEntry[]> {
  return invoke("get_notebooks");
}

export async function countRootNotes(): Promise<number> {
  return invoke("count_root_notes");
}

export async function createNotebook(
  parentRelative: string | null,
  name: string,
): Promise<NotebookEntry> {
  return invoke("create_notebook", { parentRelative, name });
}

export async function renameNotebook(
  path: string,
  newName: string,
): Promise<string> {
  return invoke("rename_notebook", { path, newName });
}

export async function deleteNotebook(path: string): Promise<void> {
  return invoke("delete_notebook", { path });
}

export async function moveNotebook(
  notebookPath: string,
  destParent: string,
): Promise<string> {
  return invoke("move_notebook", { notebookPath, destParent });
}

export async function getNotes(
  notebookPath: string | null,
): Promise<NoteEntry[]> {
  return invoke("get_notes", { notebookPath });
}

export async function readNote(path: string): Promise<NoteContent> {
  return invoke("read_note", { path });
}

export async function saveNote(
  path: string,
  meta: NoteMeta,
  body: string,
): Promise<void> {
  return invoke("save_note", { path, meta, body });
}

export async function createNote(
  notebookRelative: string | null,
  title: string,
): Promise<NoteEntry> {
  return invoke("create_note", { notebookRelative, title });
}

export async function duplicateNote(path: string): Promise<NoteEntry> {
  return invoke("duplicate_note", { path });
}

export async function createDailyNote(date?: string): Promise<NoteEntry> {
  return invoke("create_daily_note", { date: date ?? null });
}

export async function renameNote(
  path: string,
  newTitle: string,
): Promise<string> {
  return invoke("rename_note", { path, newTitle });
}

export async function deleteNote(path: string): Promise<void> {
  return invoke("delete_note", { path });
}

export async function moveNote(
  notePath: string,
  destNotebook: string,
): Promise<string> {
  return invoke("move_note", { notePath, destNotebook });
}

export async function getAllTags(): Promise<[string, number][]> {
  return invoke("get_all_tags");
}

export async function getAllNoteTitles(): Promise<NoteTitleEntry[]> {
  return invoke("get_all_note_titles");
}

export async function getGraphData(): Promise<{ nodes: { title: string; path: string }[]; edges: { source: number; target: number }[] }> {
  return invoke("get_graph_data");
}

export async function searchNotes(
  query: string,
  limit?: number,
): Promise<SearchResult[]> {
  return invoke("search_notes", { query, limit });
}

export async function reindex(): Promise<void> {
  return invoke("reindex");
}

export async function getTrash(): Promise<TrashContents> {
  return invoke("get_trash");
}

export async function restoreNote(
  trashPath: string,
  destNotebook: string | null,
): Promise<string> {
  return invoke("restore_note", { trashPath, destNotebook });
}

export async function restoreNotebook(trashPath: string): Promise<string> {
  return invoke("restore_notebook", { trashPath });
}

export async function permanentDelete(path: string): Promise<void> {
  return invoke("permanent_delete", { path });
}

export async function emptyTrash(): Promise<void> {
  return invoke("empty_trash");
}

export async function loadVaultState(): Promise<VaultState> {
  return invoke("load_vault_state");
}

export async function saveVaultState(vaultState: VaultState): Promise<void> {
  return invoke("save_vault_state", { vaultState });
}

export async function copyTextToClipboard(text: string): Promise<void> {
  return invoke("copy_text_to_clipboard", { text });
}

export async function readClipboardImage(): Promise<number[]> {
  return invoke("read_clipboard_image");
}

export async function copyImageToClipboard(path: string): Promise<void> {
  return invoke("copy_image_to_clipboard", { path });
}

export async function saveImage(name: string, data: number[]): Promise<string> {
  return invoke("save_image", { name, data });
}

export async function saveAttachment(
  name: string,
  data: number[],
): Promise<string> {
  return invoke("save_attachment", { name, data });
}

export async function getNotebookIcons(): Promise<Record<string, string>> {
  return invoke("get_notebook_icons");
}

export async function setNotebookIcon(
  notebookRelative: string,
  iconRelative: string | null,
): Promise<void> {
  return invoke("set_notebook_icon", { notebookRelative, iconRelative });
}

export async function setGeneralSettings(
  compactNotes: boolean,
  timeFormat: string,
  weekStart: string,
  dailyTitleFormat: string,
  gpuAcceleration: boolean,
  autostart: boolean,
  pdfPreview: boolean,
  pdfHeight: number,
  titleMode: string,
  hideTitleInBody: boolean,
  showLineNumbers: boolean,
  showLinkArrows: boolean,
  defaultViewMode: boolean,
  newNotesInSourceMode: boolean,
  showTrayIcon: boolean,
  closeToTray: boolean,
  enableWikiLinks: boolean,
  showNoteDates: boolean,
  showNoteSwitcher: boolean,
  startupView: StartupView,
  restoreLastSession: boolean,
  showAllNotes: boolean,
  showQuickAccess: boolean,
  showTasks: boolean,
  showDailyNotes: boolean,
  showTrash: boolean,
): Promise<void> {
  return invoke("set_general_settings", {
    compactNotes,
    timeFormat,
    weekStart,
    dailyTitleFormat,
    gpuAcceleration,
    autostart,
    pdfPreview,
    pdfHeight,
    titleMode,
    hideTitleInBody,
    showLineNumbers,
    showLinkArrows,
    defaultViewMode,
    newNotesInSourceMode,
    showTrayIcon,
    closeToTray,
    enableWikiLinks,
    showNoteDates,
    showNoteSwitcher,
    startupView,
    restoreLastSession,
    showAllNotes,
    showQuickAccess,
    showTasks,
    showDailyNotes,
    showTrash,
  });
}

export async function getQuickAccess(): Promise<NoteEntry[]> {
  return invoke("get_quick_access");
}

export async function addQuickAccess(noteRelative: string): Promise<void> {
  return invoke("add_quick_access", { noteRelative });
}

export async function removeQuickAccess(noteRelative: string): Promise<void> {
  return invoke("remove_quick_access", { noteRelative });
}

export async function reorderQuickAccess(paths: string[]): Promise<void> {
  return invoke("reorder_quick_access", { paths });
}

export async function getVaultStats(): Promise<VaultStats> {
  return invoke("get_vault_stats");
}

export interface OrphanAttachment {
  name: string;
  size: number;
}

export async function findOrphanedAttachments(): Promise<OrphanAttachment[]> {
  return invoke("find_orphaned_attachments");
}

export async function trashOrphanedAttachments(names: string[]): Promise<number> {
  return invoke("trash_orphaned_attachments", { names });
}

export async function importObsidian(): Promise<void> {
  return invoke("import_obsidian");
}

export async function openFile(path: string): Promise<void> {
  return invoke("open_file", { path });
}

export async function openUrl(url: string): Promise<void> {
  return invoke("open_url", { url });
}

export async function copyFileTo(
  source: string,
  destination: string,
): Promise<void> {
  return invoke("copy_file_to", { source, destination });
}

export async function writeBytesTo(
  destination: string,
  data: Uint8Array,
): Promise<void> {
  return invoke("write_bytes_to", { destination, data: Array.from(data) });
}

export async function copyPngToClipboard(data: Uint8Array): Promise<void> {
  return invoke("copy_png_to_clipboard", { data: Array.from(data) });
}

// ── Backup ──

export async function createBackup(): Promise<void> {
  return invoke("create_backup");
}

export async function listBackups(): Promise<BackupEntry[]> {
  return invoke("list_backups");
}

export async function restoreBackup(backupPath: string): Promise<void> {
  return invoke("restore_backup", { backupPath });
}

export async function deleteBackup(backupPath: string): Promise<void> {
  return invoke("delete_backup", { backupPath });
}

export async function setBackupSettings(
  enabled: boolean,
  frequency: string,
  maxCount: number,
  location: string | null,
  includeAttachments: boolean,
): Promise<void> {
  return invoke("set_backup_settings", {
    enabled,
    frequency,
    maxCount,
    location,
    includeAttachments,
  });
}

// ── Tasks ──

export async function getTasks(): Promise<TaskItem[]> {
  return invoke("get_tasks");
}

export async function setTaskDone(
  notePath: string,
  line: number,
  rawLine: string,
  done: boolean,
): Promise<void> {
  return invoke("set_task_done", { notePath, line, rawLine, done });
}

export async function setTaskPriority(
  notePath: string,
  line: number,
  rawLine: string,
  priority: string | null,
): Promise<void> {
  return invoke("set_task_priority", { notePath, line, rawLine, priority });
}

export async function setTaskDue(
  notePath: string,
  line: number,
  rawLine: string,
  due: string | null,
): Promise<void> {
  return invoke("set_task_due", { notePath, line, rawLine, due });
}

// ── Sync (WebDAV) ──

export async function setSyncSettings(
  provider: string | null,
  url: string | null,
  username: string | null,
  password: string | null,
  syncOnOpen: boolean,
  syncOnChange: boolean,
  syncIntervalMinutes: number,
): Promise<void> {
  return invoke("set_sync_settings", { provider, url, username, password, syncOnOpen, syncOnChange, syncIntervalMinutes });
}

export async function testSyncConnection(): Promise<void> {
  return invoke("test_sync_connection");
}

export async function syncNow(): Promise<void> {
  return invoke("sync_now");
}

// ── Version History ──

export async function getNoteVersions(noteId: string): Promise<VersionEntry[]> {
  return invoke("get_note_versions", { noteId });
}

export async function getNoteVersionContent(
  noteId: string,
  timestamp: string,
): Promise<string> {
  return invoke("get_note_version_content", { noteId, timestamp });
}

export async function createVersion(
  path: string,
  noteId: string,
): Promise<void> {
  return invoke("create_version", { path, noteId });
}

// ── AI ──

export async function setAiSettings(
  provider: string | null,
  apiKey: string | null,
  model: string,
  writingStyle: string | null,
  baseUrl: string | null = null,
  ollamaApiKey: string | null = null,
  openaiCompatibleBaseUrl: string | null = null,
  openaiCompatibleApiKey: string | null = null,
): Promise<void> {
  return invoke("set_ai_settings", {
    provider, apiKey, model, writingStyle, baseUrl,
    ollamaApiKey, openaiCompatibleBaseUrl, openaiCompatibleApiKey,
  });
}

export async function testAiConnection(): Promise<void> {
  return invoke("test_ai_connection");
}

export async function aiAsk(
  action: string,
  text: string,
  customPrompt: string | null,
  requestId: string,
): Promise<void> {
  return invoke("ai_ask", { action, text, customPrompt, requestId });
}

export async function getInstallType(): Promise<string> {
  return invoke("get_install_type");
}

export async function isMobilePlatform(): Promise<boolean> {
  return invoke("is_mobile_platform");
}

export async function getPendingOpenFile(): Promise<string | null> {
  return invoke("get_pending_open_file");
}
