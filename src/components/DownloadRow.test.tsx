import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { DownloadRow } from "./DownloadRow";
import type { Download } from "../types";

afterEach(cleanup);

function makeDownload(over: Partial<Download> = {}): Download {
  return {
    id: "rec-1",
    url: "https://example.com/live.m3u8",
    file_name: "live-stream.mp4",
    dest_path: "C:/dl/live-stream.mp4",
    total_bytes: null,
    downloaded_bytes: 1048576,
    status: "recording",
    supports_ranges: false,
    error: null,
    priority: "normal",
    category: "video",
    kind: "hls",
    ...over,
  };
}

function renderRow(over: Partial<Download> = {}, onStop = vi.fn()) {
  render(
    <DownloadRow
      d={makeDownload(over)}
      speed={0}
      queuePosition={null}
      seg={{ done: 7, total: 0 }}
      onPause={vi.fn()}
      onResume={vi.fn()}
      onCancel={vi.fn()}
      onStop={onStop}
      onPriority={vi.fn()}
      onOpenFile={vi.fn()}
      onOpenFolder={vi.fn()}
      onRemove={vi.fn()}
    />,
  );
  return onStop;
}

describe("DownloadRow recording mode", () => {
  it("shows the Enregistrement label with segments and size", () => {
    renderRow();
    expect(screen.getByText(/Enregistrement/)).toBeTruthy();
    expect(screen.getByText(/7 segments/)).toBeTruthy();
    expect(screen.getByText(/1,0 Mo/)).toBeTruthy();
  });

  it("renders a LIVE badge", () => {
    renderRow();
    expect(screen.getByText("LIVE")).toBeTruthy();
  });

  it("calls onStop when the Stop button is clicked", () => {
    const onStop = vi.fn();
    renderRow({}, onStop);
    fireEvent.click(screen.getByTitle("Arrêter (finalise l'enregistrement)"));
    expect(onStop).toHaveBeenCalledTimes(1);
  });

  it("does not show LIVE badge or Stop button for a completed row", () => {
    renderRow({ status: "completed" });
    expect(screen.queryByText("LIVE")).toBeNull();
    expect(screen.queryByTitle("Arrêter (finalise l'enregistrement)")).toBeNull();
  });
});
