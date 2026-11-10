import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import type { Download } from "../../types";
import type { DayGroup } from "../../library/library";
import { CategoryChips } from "./CategoryChips";
import { ViewToggle } from "./ViewToggle";
import { LibraryList } from "./LibraryList";
import { LibraryGrid } from "./LibraryGrid";

afterEach(cleanup);

function mk(p: Partial<Download> = {}): Download {
  return {
    id: "a",
    url: "https://example.com/f",
    file_name: "film.mp4",
    dest_path: "C:/f",
    total_bytes: 1048576,
    downloaded_bytes: 1048576,
    status: "completed",
    supports_ranges: true,
    error: null,
    priority: "normal",
    category: "video",
    completed_at: Math.floor(new Date(2026, 10, 10, 9, 5).getTime() / 1000),
    ...p,
  } as Download;
}

describe("CategoryChips", () => {
  it("affiche les compteurs et notifie le clic", () => {
    const onChange = vi.fn();
    const items = [mk({ id: "1" }), mk({ id: "2" }), mk({ id: "3", category: "music" })];
    render(<CategoryChips items={items} value="all" onChange={onChange} />);
    expect(screen.getByText("Tout · 3")).toBeTruthy();
    fireEvent.click(screen.getByText("Vidéos · 2"));
    expect(onChange).toHaveBeenCalledWith("video");
  });
});

describe("ViewToggle", () => {
  it("marque le bouton actif et notifie", () => {
    const onChange = vi.fn();
    render(<ViewToggle value="list" onChange={onChange} />);
    expect(screen.getByLabelText("Liste").getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByLabelText("Grille").getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(screen.getByLabelText("Grille"));
    expect(onChange).toHaveBeenCalledWith("grid");
  });
});

describe("LibraryList", () => {
  const groups: DayGroup[] = [
    { key: "k1", label: "Aujourd'hui", items: [mk({ id: "1", file_name: "a.mp4" })] },
    { key: "k2", label: "Hier", items: [mk({ id: "2", file_name: "b.zip", completed_at: null })] },
  ];
  it("affiche en-têtes, sélection et clic", () => {
    const onSelect = vi.fn();
    render(<LibraryList groups={groups} selectedId="2" onSelect={onSelect} />);
    expect(screen.getByText("Aujourd'hui")).toBeTruthy();
    expect(screen.getByText("Hier")).toBeTruthy();
    expect(screen.getByText("09:05")).toBeTruthy();
    expect(screen.getByText("—")).toBeTruthy();
    fireEvent.click(screen.getByText("a.mp4"));
    expect(onSelect).toHaveBeenCalledWith("1");
    expect(screen.getByText("b.zip").closest("button")!.getAttribute("aria-current")).toBe("true");
    expect(screen.getByText("a.mp4").closest("button")!.getAttribute("aria-current")).toBeNull();
  });
});

describe("LibraryGrid", () => {
  it("vignette img si thumbnail_url, sinon tuile d'icône", () => {
    const items = [
      mk({ id: "1", file_name: "a.mp4", thumbnail_url: "https://x/t.jpg" }),
      mk({ id: "2", file_name: "b.mp4", thumbnail_url: null }),
    ];
    const { container } = render(<LibraryGrid items={items} selectedId={null} onSelect={() => {}} />);
    expect(container.querySelectorAll("img").length).toBe(1);
    expect(container.querySelectorAll("svg").length).toBe(1);
    expect(screen.getByText("b.mp4")).toBeTruthy();
  });
});
