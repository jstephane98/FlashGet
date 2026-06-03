import type { Download, Category, DownloadStatus } from "./types";

export interface Filters {
  query: string;
  category: Category | "all";
  status: DownloadStatus | "all";
}

export function filterDownloads(list: Download[], f: Filters): Download[] {
  const q = f.query.trim().toLowerCase();
  return list.filter(
    (d) =>
      (f.category === "all" || d.category === f.category) &&
      (f.status === "all" || d.status === f.status) &&
      (q === "" || d.file_name.toLowerCase().includes(q)),
  );
}
