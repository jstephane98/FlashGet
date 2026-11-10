import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { DownloadsPage, type CardActions } from "./DownloadsPage";
import { CompletedPage } from "./CompletedPage";
import type { Download } from "../types";

afterEach(cleanup);

function mk(id: string, status: Download["status"], file_name: string): Download {
  return {
    id, url: "https://x/" + id, file_name, dest_path: "C:/dl/" + file_name,
    total_bytes: 1000, downloaded_bytes: 100, status, supports_ranges: true,
    error: null, priority: "normal", category: "other", kind: "regular",
  };
}
const actions: CardActions = {
  pause: vi.fn(), resume: vi.fn(), cancel: vi.fn(), stop: vi.fn(), priority: vi.fn(),
  openFile: vi.fn(), openFolder: vi.fn(), remove: vi.fn(),
};
function page(downloads: Download[], over: Partial<Parameters<typeof DownloadsPage>[0]> = {}) {
  return render(
    <DownloadsPage downloads={downloads} speeds={{}} parts={{}} segments={{}}
      onAddLink={vi.fn()} onAddBatch={vi.fn()} onPauseAll={vi.fn()} onResumeAll={vi.fn()}
      actions={actions} {...over} />,
  );
}
const list = [
  mk("1", "downloading", "alpha.bin"),
  mk("2", "recording", "beta.ts"),
  mk("3", "paused", "gamma.zip"),
];

describe("pages", () => {
  it("onglets avec compteurs, filtrage au clic", () => {
    page(list);
    expect(screen.getByText("Tous · 3")).toBeTruthy();
    fireEvent.click(screen.getByText("Actifs · 2"));
    expect(screen.queryByText("gamma.zip")).toBeNull();
    expect(screen.getByText("alpha.bin")).toBeTruthy();
    expect(screen.getByText("beta.ts")).toBeTruthy();
  });
  it("Tout suspendre s'il y a un actif, sinon Tout reprendre", () => {
    const onPauseAll = vi.fn();
    const { unmount } = page(list, { onPauseAll });
    fireEvent.click(screen.getByText("Tout suspendre"));
    expect(onPauseAll).toHaveBeenCalled();
    unmount();
    const onResumeAll = vi.fn();
    page([mk("3", "paused", "gamma.zip")], { onResumeAll });
    expect(screen.queryByText("Tout suspendre")).toBeNull();
    fireEvent.click(screen.getByText("Tout reprendre"));
    expect(onResumeAll).toHaveBeenCalled();
  });
  it("Ajouter un lien appelle onAddLink", () => {
    const onAddLink = vi.fn();
    page(list, { onAddLink });
    fireEvent.click(screen.getByText("Ajouter un lien"));
    expect(onAddLink).toHaveBeenCalled();
  });
  it("Terminés liste les terminés, le plus récent en premier, et Vider appelle onClear", () => {
    const onClear = vi.fn();
    render(<CompletedPage onClear={onClear} actions={actions}
      downloads={[mk("1", "completed", "old.zip"), mk("2", "downloading", "busy.bin"), mk("3", "completed", "new.zip")]} />);
    expect(screen.queryByText("busy.bin")).toBeNull();
    const names = screen.getAllByText(/\.zip$/).map((e) => e.textContent);
    expect(names).toEqual(["new.zip", "old.zip"]);
    fireEvent.click(screen.getByText("Vider la liste"));
    expect(onClear).toHaveBeenCalled();
  });
});
