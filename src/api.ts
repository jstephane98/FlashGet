import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Download, Progress, Priority, Settings, Category, CategoryDirs, BridgeInfo } from "./types";

export interface HlsVariant {
  index: number;
  resolution: string | null;
  bandwidth: number;
  est_size: number | null;
}

export interface MediaVariantsEvent {
  probe_id: string;
  file_name: string;
  variants: HlsVariant[];
}

export function onMediaVariants(cb: (e: MediaVariantsEvent) => void): Promise<UnlistenFn> {
  return listen<MediaVariantsEvent>("media://variants", (e) => cb(e.payload));
}

export interface MediaErrorEvent {
  message: string;
}

export function onMediaError(cb: (e: MediaErrorEvent) => void): Promise<UnlistenFn> {
  return listen<MediaErrorEvent>("media://error", (e) => cb(e.payload));
}

export function addMediaDownload(probeId: string, variantIndex: number, fileName: string): Promise<Download> {
  return invoke<Download>("add_media_download", { probeId, variantIndex, fileName });
}

export function addDownload(url: string, destDir: string): Promise<Download> {
  return invoke<Download>("add_download", { args: { url, dest_dir: destDir } });
}

export interface BatchSummary {
  added: number;
  skipped_invalid: number;
  skipped_duplicate: number;
  failed: number;
}

export function addBatchDownloads(text: string, destDir: string): Promise<BatchSummary> {
  return invoke<BatchSummary>("add_batch_downloads", { args: { text, dest_dir: destDir } });
}

export function listDownloads(): Promise<Download[]> {
  return invoke<Download[]>("list_downloads");
}

export function startDownload(id: string): Promise<void> {
  return invoke("start_download", { id });
}

export function pauseDownload(id: string): Promise<void> {
  return invoke("pause_download", { id });
}

export function cancelDownload(id: string): Promise<void> {
  return invoke("cancel_download", { id });
}

export function stopRecording(id: string): Promise<void> {
  return invoke("stop_recording", { id });
}

export function onProgress(cb: (p: Progress) => void): Promise<UnlistenFn> {
  return listen<Progress>("download://progress", (e) => cb(e.payload));
}

export function onDownloadAdded(cb: (d: Download) => void): Promise<UnlistenFn> {
  return listen<Download>("download://added", (e) => cb(e.payload));
}

export function setPriority(id: string, priority: Priority): Promise<void> {
  return invoke("set_priority", { id, priority });
}

export function getSettings(): Promise<Settings> {
  return invoke<Settings>("get_settings");
}

export function updateSettings(maxConcurrent: number, maxSpeedBps: number): Promise<void> {
  return invoke("update_settings", { maxConcurrent, maxSpeedBps });
}

export function pauseAll(): Promise<void> {
  return invoke("pause_all");
}

export function resumeAll(): Promise<void> {
  return invoke("resume_all");
}

export function getCategoryDirs(): Promise<CategoryDirs> {
  return invoke<CategoryDirs>("get_category_dirs");
}

export function setCategoryDir(category: Category, path: string | null): Promise<void> {
  return invoke("set_category_dir", { category, path });
}

export function removeFromList(id: string): Promise<void> {
  return invoke("remove_from_list", { id });
}

export function clearCompleted(): Promise<void> {
  return invoke("clear_completed");
}

export function openFile(id: string): Promise<void> {
  return invoke("open_file", { id });
}

export function openFolder(id: string): Promise<void> {
  return invoke("open_folder", { id });
}

export function getBridgeInfo(): Promise<BridgeInfo> {
  return invoke<BridgeInfo>("get_bridge_info");
}

export function regenerateBridgeToken(): Promise<string> {
  return invoke<string>("regenerate_bridge_token");
}

export function getAutostart(): Promise<boolean> {
  return invoke<boolean>("get_autostart");
}

export function setAutostart(enabled: boolean): Promise<void> {
  return invoke("set_autostart", { enabled });
}

export function youtubeLogin(): Promise<void> {
  return invoke("youtube_login");
}
export function youtubeStatus(): Promise<boolean> {
  return invoke<boolean>("youtube_status");
}
export function youtubeLogout(): Promise<void> {
  return invoke("youtube_logout");
}
export function getCookiesFromBrowser(): Promise<string | null> {
  return invoke<string | null>("get_cookies_from_browser");
}
export function setCookiesFromBrowser(value: string | null): Promise<void> {
  return invoke("set_cookies_from_browser", { value });
}
export function onYoutubeStatus(cb: () => void): Promise<UnlistenFn> {
  return listen("youtube://status", () => cb());
}
