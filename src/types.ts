export type DownloadStatus =
  | "queued"
  | "downloading"
  | "paused"
  | "completed"
  | "error";

export type Priority = "high" | "normal" | "low";

export type DownloadKind = "regular" | "hls" | "dash";

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
}

export interface Settings {
  max_concurrent: number;
  max_speed_bps: number;
}

export interface Progress {
  id: string;
  downloaded_bytes: number;
  total_bytes: number | null;
  speed_bps: number;
  status: DownloadStatus;
  segments_done?: number | null;
  segments_total?: number | null;
}

export interface BridgeInfo {
  token: string;
  port: number;
  listening: boolean;
}
