import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { DownloadCard } from "./DownloadCard";
import type { Download, Priority } from "../../types";

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

interface Extra {
  parts?: number[] | null;
  speed?: number;
  queuePosition?: number | null;
  onStop?: () => void;
  onPriority?: (p: Priority) => void;
}

function renderCard(over: Partial<Download> = {}, extra: Extra = {}) {
  render(
    <DownloadCard
      d={makeDownload(over)}
      speed={extra.speed ?? 0}
      parts={extra.parts}
      queuePosition={extra.queuePosition ?? null}
      seg={{ done: 7, total: 0 }}
      onPause={vi.fn()}
      onResume={vi.fn()}
      onCancel={vi.fn()}
      onStop={extra.onStop ?? vi.fn()}
      onPriority={extra.onPriority ?? vi.fn()}
      onOpenFile={vi.fn()}
      onOpenFolder={vi.fn()}
      onRemove={vi.fn()}
    />,
  );
}

describe("DownloadCard recording mode", () => {
  it("affiche l'enregistrement en direct avec durée et taille", () => {
    renderCard();
    expect(screen.getByText("Enregistrement en direct")).toBeTruthy();
    expect(screen.getByText(/1,0 Mo/)).toBeTruthy();
  });

  it("renders a LIVE badge", () => {
    renderCard();
    expect(screen.getByText("LIVE")).toBeTruthy();
  });

  it("calls onStop when the Stop button is clicked", () => {
    const onStop = vi.fn();
    renderCard({}, { onStop });
    fireEvent.click(screen.getByTitle("Arrêter (finalise l'enregistrement)"));
    expect(onStop).toHaveBeenCalledTimes(1);
  });

  it("no LIVE badge or Stop button once completed", () => {
    renderCard({ status: "completed" });
    expect(screen.queryByText("LIVE")).toBeNull();
    expect(screen.queryByTitle("Arrêter (finalise l'enregistrement)")).toBeNull();
  });

  it("affiche le badge yt-dlp pour kind ytdlp", () => {
    renderCard({ kind: "ytdlp" });
    expect(screen.getByText("yt-dlp")).toBeTruthy();
  });

  it("barre pleine (100%) pour un téléchargement terminé sans taille connue", () => {
    renderCard({ kind: "ytdlp", status: "completed", total_bytes: null, downloaded_bytes: 5000 });
    expect(screen.getByText("terminé")).toBeTruthy();
    const bar = screen.getByTestId("bar-fill");
    expect(bar.style.width).toBe("100%");
  });
});

describe("DownloadCard", () => {
  it("une cellule par segment quand parts est fourni", () => {
    renderCard({ status: "downloading", total_bytes: 800, downloaded_bytes: 400, kind: "regular" }, { parts: [1, 0.5, 0, 0.25] });
    const cells = screen.getAllByTestId("segment");
    expect(cells).toHaveLength(4);
    expect((cells[1].firstChild as HTMLElement).style.width).toBe("50%");
    expect(screen.getByText("4 connexions")).toBeTruthy();
  });

  it("barre indéterminée sans taille", () => {
    renderCard({ status: "downloading", total_bytes: null, kind: "regular" }, { parts: null, speed: 1024 });
    expect(screen.queryAllByTestId("segment")).toHaveLength(0);
    expect(document.body.textContent).not.toMatch(/NaN|Infinity/);
  });

  it("position dans la file", () => {
    renderCard({ status: "queued", kind: "regular" }, { queuePosition: 2 });
    expect(screen.getByText(/n° 2/)).toBeTruthy();
  });

  it("nom long tronqué", () => {
    renderCard({ file_name: "x".repeat(300) });
    expect(screen.getByText("x".repeat(300)).className).toMatch(/truncate/);
  });

  it("le menu change la priorité", () => {
    const onPriority = vi.fn();
    renderCard({ status: "downloading", kind: "regular", total_bytes: 100 }, { onPriority });
    fireEvent.click(screen.getByLabelText("Actions"));
    fireEvent.click(screen.getByText("Priorité haute"));
    expect(onPriority).toHaveBeenCalledWith("high");
  });

  it("un direct affiche une barre pulsée visible", () => {
    renderCard();
    const bar = screen.getByTestId("bar-indeterminate");
    expect(bar.style.width).not.toBe("0%");
    expect(bar.className).toMatch(/w-1\/3/);
    expect(bar.className).toMatch(/bg-err/);
    expect(bar.className).toMatch(/animate-pulse/);
  });

  it("badge de catégorie en français", () => {
    renderCard({ status: "downloading", kind: "regular", category: "video", total_bytes: 100 });
    expect(screen.getByText("Vidéo")).toBeTruthy();
  });

  it("en pause, les segments restent affichés", () => {
    renderCard({ status: "paused", kind: "regular", total_bytes: 100 }, { parts: [1, 0.5] });
    expect(screen.getAllByTestId("segment")).toHaveLength(2);
    expect(screen.getByText("2 connexions")).toBeTruthy();
  });
});
