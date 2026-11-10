import { describe, it, expect, vi, afterEach, beforeEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/react";
import type { Download } from "../../types";
import * as api from "../../api";
import { DetailPanel } from "./DetailPanel";
import { DeleteDialog } from "./DeleteDialog";

vi.mock("../../api");
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => "asset://" + p }));

const getFileInfo = vi.mocked(api.getFileInfo);
const allowPlayback = vi.mocked(api.allowPlayback);

beforeEach(() => {
  getFileInfo.mockReset();
  allowPlayback.mockReset();
  allowPlayback.mockResolvedValue("C:/f/film.mp4");
});
afterEach(cleanup);

function mk(p: Partial<Download> = {}): Download {
  return {
    id: "a", url: "https://example.com/f", file_name: "film.mp4", dest_path: "C:/f/film.mp4",
    total_bytes: 1048576, downloaded_bytes: 1048576, status: "completed", supports_ranges: true,
    error: null, priority: "normal", category: "video",
    started_at: 1000, completed_at: 1000 + 83, duration_s: 125, ...p,
  } as Download;
}
const cbs = () => ({ onOpenFile: vi.fn(), onOpenFolder: vi.fn(), onCopyLink: vi.fn(), onRedownload: vi.fn(), onDelete: vi.fn() });

describe("DetailPanel", () => {
  it("vidéo lisible : balise video avec la source autorisée", async () => {
    getFileInfo.mockResolvedValue({ exists: true, size: 2097152, playable: "video" });
    const { container } = render(<DetailPanel d={mk()} {...cbs()} />);
    await waitFor(() => expect(container.querySelector("video")).not.toBeNull());
    expect(container.querySelector("video")!.getAttribute("src")).toBe("asset://C:/f/film.mp4");
    expect(allowPlayback).toHaveBeenCalledWith("a");
  });

  it("audio : balise audio et vignette", async () => {
    getFileInfo.mockResolvedValue({ exists: true, size: 10, playable: "audio" });
    const { container } = render(<DetailPanel d={mk({ category: "music", thumbnail_url: "https://x/t.jpg" })} {...cbs()} />);
    await waitFor(() => expect(container.querySelector("audio")).not.toBeNull());
    expect(container.querySelector("img")).not.toBeNull();
    expect(container.querySelector("video")).toBeNull();
  });

  it("fichier introuvable : message et Ouvrir désactivé", async () => {
    getFileInfo.mockResolvedValue({ exists: false, size: 0, playable: null });
    render(<DetailPanel d={mk()} {...cbs()} />);
    expect(await screen.findByText("Fichier introuvable (déplacé ou supprimé)")).toBeTruthy();
    expect((screen.getByRole("button", { name: /Ouvrir/ }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("non lisible : pas de lecteur", async () => {
    getFileInfo.mockResolvedValue({ exists: true, size: 5, playable: null });
    const { container } = render(<DetailPanel d={mk({ file_name: "a.zip", category: "archive" })} {...cbs()} />);
    await screen.findByText("5 o");
    expect(container.querySelector("video")).toBeNull();
    expect(container.querySelector("audio")).toBeNull();
    expect(allowPlayback).not.toHaveBeenCalled();
  });

  it("erreur de lecture", async () => {
    getFileInfo.mockResolvedValue({ exists: true, size: 5, playable: "video" });
    const { container } = render(<DetailPanel d={mk()} {...cbs()} />);
    await waitFor(() => expect(container.querySelector("video")).not.toBeNull());
    fireEvent.error(container.querySelector("video")!);
    expect(await screen.findByText(/Lecture impossible ici/)).toBeTruthy();
    expect(container.querySelector("video")).toBeNull();
  });

  it("infos : taille réelle, durée, terminé avec durée du téléchargement", async () => {
    getFileInfo.mockResolvedValue({ exists: true, size: 2097152, playable: null });
    render(<DetailPanel d={mk()} {...cbs()} />);
    expect(await screen.findByText("2,0 Mo")).toBeTruthy();
    expect(screen.getByText("MP4")).toBeTruthy();
    expect(screen.getByText("02:05")).toBeTruthy();
    expect(screen.getByText(/· 01:23/)).toBeTruthy();
    expect(screen.getByText("https://example.com/f")).toBeTruthy();
  });

  it("chaque action appelle son callback", async () => {
    getFileInfo.mockResolvedValue({ exists: true, size: 5, playable: null });
    const c = cbs();
    render(<DetailPanel d={mk()} {...c} />);
    await screen.findByText("5 o");
    fireEvent.click(screen.getByRole("button", { name: /Ouvrir/ }));
    fireEvent.click(screen.getByRole("button", { name: /^Dossier$/ }));
    fireEvent.click(screen.getByRole("button", { name: /Copier le lien/ }));
    fireEvent.click(screen.getByRole("button", { name: /Re-télécharger/ }));
    fireEvent.click(screen.getByRole("button", { name: /Supprimer…/ }));
    expect(c.onOpenFile).toHaveBeenCalledTimes(1);
    expect(c.onOpenFolder).toHaveBeenCalledTimes(1);
    expect(c.onCopyLink).toHaveBeenCalledTimes(1);
    expect(c.onRedownload).toHaveBeenCalledTimes(1);
    expect(c.onDelete).toHaveBeenCalledTimes(1);
  });

  it("changement d'élément : résultat périmé ignoré", async () => {
    let resolveA: (v: api.FileInfo) => void = () => {};
    getFileInfo.mockImplementationOnce(() => new Promise((r) => { resolveA = r; }));
    getFileInfo.mockResolvedValueOnce({ exists: true, size: 5, playable: null });
    const { container, rerender } = render(<DetailPanel d={mk()} {...cbs()} />);
    rerender(<DetailPanel d={mk({ id: "b", file_name: "b.zip" })} {...cbs()} />);
    await screen.findByText("5 o");
    resolveA({ exists: true, size: 9, playable: "video" });
    await Promise.resolve();
    expect(container.querySelector("video")).toBeNull();
    expect(allowPlayback).not.toHaveBeenCalled();
  });
});

describe("DeleteDialog", () => {
  it("trois boutons, erreur affichée, Échap annule", () => {
    const onRemove = vi.fn(), onDeleteFile = vi.fn(), onCancel = vi.fn();
    render(<DeleteDialog name="film.mp4" busy={false} error="Accès refusé" onRemove={onRemove} onDeleteFile={onDeleteFile} onCancel={onCancel} />);
    expect(screen.getByText("Supprimer « film.mp4 » ?")).toBeTruthy();
    expect(screen.getByText("Accès refusé")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Retirer de la liste" }));
    fireEvent.click(screen.getByRole("button", { name: "Supprimer aussi le fichier" }));
    fireEvent.click(screen.getByRole("button", { name: "Annuler" }));
    expect(onRemove).toHaveBeenCalledTimes(1);
    expect(onDeleteFile).toHaveBeenCalledTimes(1);
    expect(onCancel).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(2);
  });
});
