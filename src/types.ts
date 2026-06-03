export type DownloadStatus =
  | "queued"
  | "downloading"
  | "paused"
  | "completed"
  | "error";

export type Priority = "high" | "normal" | "low";

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
}

export interface BridgeInfo {
  token: string;
  port: number;
  listening: boolean;
}
