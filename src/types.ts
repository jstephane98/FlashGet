export type DownloadStatus =
  | "held"
  | "queued"
  | "downloading"
  | "recording"
  | "paused"
  | "completed"
  | "error";

export type Priority = "high" | "normal" | "low";

export type DownloadKind = "regular" | "hls" | "dash" | "ytdlp";

export type Category = "video" | "document" | "program" | "archive" | "music" | "other";

export interface CategoryDirs {
  video: string | null;
  document: string | null;
  program: string | null;
  archive: string | null;
  music: string | null;
  other: string | null;
}

export interface Download {
  id: string;
  url: string;
  file_name: string;
  dest_path: string;
  total_bytes: number | null;
  downloaded_bytes: number;
  status: DownloadStatus;
  supports_ranges: boolean;
  error: string | null;
  priority: Priority;
  category: Category;
  referrer?: string | null;
  cookie?: string | null;
  user_agent?: string | null;
  kind?: DownloadKind;
  audio_url?: string | null;
  media_selection?: string | null;
  thumbnail_url?: string | null;
  duration_s?: number | null;
  created_at?: number | null;
  started_at?: number | null;
  completed_at?: number | null;
  queue_rank?: number | null;
}

export type ConflictPolicy = "rename" | "overwrite";
export type AudioFormat = "mp3" | "m4a" | "opus";

export interface Settings {
  max_concurrent: number;
  max_speed_bps: number;
  connections: number;
  retries: number;
  auto_resume: boolean;
  conflict: ConflictPolicy;
  close_to_tray: boolean;
  notify_complete: boolean;
  notify_error: boolean;
  video_max_height: number;
  audio_format: AudioFormat;
  audio_kbps: number;
  sub_langs: string[];
  ytdlp_auto_update: boolean;
  app_update_check: boolean;
}

export interface Progress {
  id: string;
  downloaded_bytes: number;
  total_bytes: number | null;
  speed_bps: number;
  status: DownloadStatus;
  segments_done?: number | null;
  segments_total?: number | null;
  parts?: number[] | null;
}

export interface BridgeInfo {
  token: string;
  port: number;
  listening: boolean;
}

export type BridgeState = "connected" | "waiting" | "rejected" | "inactive";

export interface BridgeStatus {
  state: BridgeState;
  browser: string | null;
  version: string | null;
  /** Dernier message reçu de l'extension, en secondes Unix. */
  last_seen: number | null;
  clients: number;
  port: number;
}
