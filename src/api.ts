import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Download, Progress, Priority, Settings } from "./types";

export function addDownload(url: string, destDir: string): Promise<Download> {
  return invoke<Download>("add_download", { args: { url, dest_dir: destDir } });
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

export function onProgress(cb: (p: Progress) => void): Promise<UnlistenFn> {
  return listen<Progress>("download://progress", (e) => cb(e.payload));
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
