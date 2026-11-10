import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Download, Progress, Priority, Settings, Category, CategoryDirs, BridgeInfo } from "./types";

export type FormatKind = "av" | "video" | "audio";

export interface MediaFormat {
  index: number;
  kind: FormatKind;
  format_id: string;
  height: number | null;
  label: string;
  container: string | null;
  codecs: string | null;
  fps: number | null;
  est_size: number | null;
  recommended: boolean;
}

export interface MediaInfo {
  title: string;
  uploader: string | null;
  thumbnail: string | null;
  duration_s: number | null;
  upload_date: string | null;
  subtitle_langs: string[];
  can_cut: boolean;
}

export type LinkProbe =
  | {
      type: "file";
      probe_id: string;
      url: string;
      file_name: string;
      total_bytes: number | null;
      supports_ranges: boolean;
      category: Category;
      default_dir: string;
    }
  | {
      type: "media";
      probe_id: string;
      url: string;
      source: "hls" | "dash" | "ytdlp";
      info: MediaInfo;
      formats: MediaFormat[];
      file_name: string;
      video_dir: string;
      music_dir: string;
    };

export interface ConfirmLink {
  probe_id: string;
  format_index: number | null;
  subtitles: boolean;
  cut: { start: string; end: string | null } | null;
  dest_dir: string | null;
  file_name: string;
  start_now: boolean;
}

export function probeLink(url: string): Promise<LinkProbe> {
  return invoke<LinkProbe>("probe_link", { url });
}

export function confirmLink(req: ConfirmLink): Promise<Download> {
  return invoke<Download>("confirm_link", { req });
}

export function onLinkProbing(cb: (e: { url: string }) => void): Promise<UnlistenFn> {
  return listen<{ url: string }>("link://probing", (e) => cb(e.payload));
}

export function onLinkProbed(cb: (p: LinkProbe) => void): Promise<UnlistenFn> {
  return listen<LinkProbe>("link://probed", (e) => cb(e.payload));
}

export function onLinkError(cb: (e: { url: string; message: string }) => void): Promise<UnlistenFn> {
  return listen<{ url: string; message: string }>("link://error", (e) => cb(e.payload));
}

export interface BatchSummary {
  added: number;
  skipped_invalid: number;
  skipped_duplicate: number;
  failed: number;
}

export function addBatchDownloads(text: string, destDir: string, mode: string = "direct", hold = false): Promise<BatchSummary> {
  return invoke<BatchSummary>("add_batch_downloads", { args: { text, dest_dir: destDir, mode, hold } });
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
export function getNotifyOnComplete(): Promise<boolean> {
  return invoke<boolean>("get_notify_on_complete");
}
export function setNotifyOnComplete(value: boolean): Promise<void> {
  return invoke("set_notify_on_complete", { value });
}
export function onYoutubeStatus(cb: () => void): Promise<UnlistenFn> {
  return listen("youtube://status", () => cb());
}
export function getUiPref(key: string): Promise<string | null> {
  return invoke<string | null>("get_ui_pref", { key });
}
export function setUiPref(key: string, value: string): Promise<void> {
  return invoke("set_ui_pref", { key, value });
}
