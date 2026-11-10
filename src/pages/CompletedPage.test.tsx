import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor, within } from "@testing-library/react";
import type { Download } from "../types";

vi.mock("../api");
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => "asset://" + p }));
import * as api from "../api";
import { CompletedPage } from "./CompletedPage";

const getUiPref = vi.mocked(api.getUiPref);
const setUiPref = vi.mocked(api.setUiPref);
const getFileInfo = vi.mocked(api.getFileInfo);
const deleteDownload = vi.mocked(api.deleteDownload);

const BASE = Math.floor(Date.now() / 1000) - 3600;

function mk(id: string, file_name: string, p: Partial<Download> = {}): Download {
  return {
    id, url: "https://x/" + id, file_name, dest_path: "C:/dl/" + file_name,
    total_bytes: 1000, downloaded_bytes: 1000, status: "completed", supports_ranges: true,
    error: null, priority: "normal", category: "other", kind: "regular",
    completed_at: BASE, ...p,
  } as Download;
}

// Ordre de la liste : du plus ancien au plus récent ; affichage inverse (a, b, c).
const abc = () => [
  mk("c", "c.bin", { completed_at: BASE + 100 }),
  mk("b", "b.bin", { completed_at: BASE + 200 }),
  mk("a", "a.bin", { completed_at: BASE + 300 }),
];

type Props = Parameters<typeof CompletedPage>[0];
function setup(downloads: Download[], over: Partial<Props> = {}) {
  const props: Props = {
    downloads, onClear: vi.fn(), onOpenFile: vi.fn(), onOpenFolder: vi.fn(),
    onCopyLink: vi.fn(), onRedownload: vi.fn(), onDeleted: vi.fn(), ...over,
  };
  const utils = render(<CompletedPage {...props} />);
  const update = (list: Download[]) => utils.rerender(<CompletedPage {...props} downloads={list} />);
  return { ...utils, props, update };
}

/** Nom affiché dans le panneau de détail, une fois ses infos chargées. */
async function panelName(): Promise<string | null> {
  const panel = await screen.findByRole("complementary");
  await within(panel).findByText("5 o");
  return within(panel).getByRole("heading", { level: 2 }).textContent;
}

beforeEach(() => {
  getUiPref.mockReset().mockResolvedValue(null);
  setUiPref.mockReset().mockResolvedValue(undefined);
  getFileInfo.mockReset().mockResolvedValue({ exists: true, size: 5, playable: null });
  deleteDownload.mockReset().mockResolvedValue(undefined);
});
afterEach(cleanup);

describe("CompletedPage", () => {
  it("liste par défaut avec panneau sur le premier élément", async () => {
    setup([...abc(), mk("z", "busy.bin", { status: "downloading" })]);
    expect(await panelName()).toBe("a.bin");
    expect(getUiPref).toHaveBeenCalledWith("ui.completedView");
    expect(screen.getByLabelText("Liste").getAttribute("aria-pressed")).toBe("true");
    expect(screen.queryByText("busy.bin")).toBeNull();
    expect(screen.getAllByText("a.bin")[0].closest("button")!.getAttribute("aria-current")).toBe("true");
  });

  it("bascule en grille enregistrée (setUiPref ui.completedView grid)", async () => {
    setup(abc());
    expect(await panelName()).toBe("a.bin");
    fireEvent.click(screen.getByLabelText("Grille"));
    expect(setUiPref).toHaveBeenCalledWith("ui.completedView", "grid");
    expect(screen.getByLabelText("Grille").getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: "Fermer le détail" }));
    expect(screen.queryByRole("complementary")).toBeNull();
    fireEvent.click(screen.getByText("b.bin"));
    expect(await panelName()).toBe("b.bin");
  });

  it("préférence grid au démarrage", async () => {
    getUiPref.mockResolvedValue("grid");
    setup(abc());
    await waitFor(() => expect(screen.getByLabelText("Grille").getAttribute("aria-pressed")).toBe("true"));
    expect(screen.queryByRole("complementary")).toBeNull();
    expect(getFileInfo).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText("c.bin"));
    expect(await panelName()).toBe("c.bin");
  });

  it("filtre catégorie et recherche", async () => {
    setup([
      mk("1", "film.mp4", { category: "video", completed_at: BASE + 300 }),
      mk("2", "song.mp3", { category: "music", completed_at: BASE + 200 }),
      mk("3", "pack.zip", { category: "archive", completed_at: BASE + 100 }),
    ]);
    expect(await panelName()).toBe("film.mp4");
    fireEvent.click(screen.getByText("Musique · 1"));
    expect(await panelName()).toBe("song.mp3");
    expect(screen.queryAllByText("film.mp4")).toHaveLength(0);
    expect(screen.queryAllByText("pack.zip")).toHaveLength(0);
    fireEvent.click(screen.getByText("Tout · 3"));
    fireEvent.change(screen.getByPlaceholderText("Rechercher…"), { target: { value: "PACK" } });
    expect(await panelName()).toBe("pack.zip");
    expect(screen.queryAllByText("film.mp4")).toHaveLength(0);
    expect(screen.queryAllByText("song.mp3")).toHaveLength(0);
    fireEvent.change(screen.getByPlaceholderText("Rechercher…"), { target: { value: "rien" } });
    expect(screen.getByText("Aucun téléchargement terminé.")).toBeTruthy();
    expect(screen.queryByRole("complementary")).toBeNull();
  });

  it("sélection après suppression : suivant, puis précédent quand c'était le dernier", async () => {
    const { props, update } = setup(abc());
    expect(await panelName()).toBe("a.bin");
    fireEvent.click(screen.getByRole("button", { name: /Supprimer…/ }));
    fireEvent.click(screen.getByRole("button", { name: "Retirer de la liste" }));
    await waitFor(() => expect(props.onDeleted).toHaveBeenCalledTimes(1));
    expect(deleteDownload).toHaveBeenCalledWith("a", false);
    expect(screen.queryByRole("dialog")).toBeNull();
    update(abc().filter((d) => d.id !== "a"));
    expect(await panelName()).toBe("b.bin");

    fireEvent.click(screen.getByText("c.bin"));
    expect(await panelName()).toBe("c.bin");
    fireEvent.click(screen.getByRole("button", { name: /Supprimer…/ }));
    fireEvent.click(screen.getByRole("button", { name: "Retirer de la liste" }));
    await waitFor(() => expect(props.onDeleted).toHaveBeenCalledTimes(2));
    expect(deleteDownload).toHaveBeenLastCalledWith("c", false);
    update(abc().filter((d) => d.id === "b"));
    expect(await panelName()).toBe("b.bin");
  });

  it("Supprimer aussi le fichier appelle deleteDownload(id, true) puis onDeleted", async () => {
    const { props } = setup(abc());
    expect(await panelName()).toBe("a.bin");
    fireEvent.click(screen.getByRole("button", { name: /Supprimer…/ }));
    expect(screen.getByText("Supprimer « a.bin » ?")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Supprimer aussi le fichier" }));
    await waitFor(() => expect(props.onDeleted).toHaveBeenCalledTimes(1));
    expect(deleteDownload).toHaveBeenCalledWith("a", true);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("échec de suppression : erreur affichée, élément conservé", async () => {
    deleteDownload.mockRejectedValue("Accès refusé");
    const { props } = setup(abc());
    expect(await panelName()).toBe("a.bin");
    fireEvent.click(screen.getByRole("button", { name: /Supprimer…/ }));
    fireEvent.click(screen.getByRole("button", { name: "Supprimer aussi le fichier" }));
    expect(await within(screen.getByRole("dialog")).findByText("Accès refusé")).toBeTruthy();
    expect(props.onDeleted).not.toHaveBeenCalled();
    expect((screen.getByRole("button", { name: "Annuler" }) as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Annuler" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getAllByText("a.bin").length).toBeGreaterThan(0);
    expect(await panelName()).toBe("a.bin");
  });

  it("Re-télécharger appelle onRedownload(url), Copier le lien onCopyLink(d)", async () => {
    const { props } = setup(abc());
    expect(await panelName()).toBe("a.bin");
    fireEvent.click(screen.getByRole("button", { name: /Re-télécharger/ }));
    expect(props.onRedownload).toHaveBeenCalledWith("https://x/a");
    fireEvent.click(screen.getByRole("button", { name: /Copier le lien/ }));
    expect(vi.mocked(props.onCopyLink).mock.calls[0][0].id).toBe("a");
    fireEvent.click(screen.getByRole("button", { name: /Ouvrir/ }));
    expect(props.onOpenFile).toHaveBeenCalledWith("a");
    fireEvent.click(screen.getByRole("button", { name: /^Dossier$/ }));
    expect(props.onOpenFolder).toHaveBeenCalledWith("a");
  });

  it("vide : message, et Vider la liste appelle onClear", async () => {
    const { props } = setup([mk("z", "busy.bin", { status: "downloading" })]);
    await waitFor(() => expect(getUiPref).toHaveBeenCalled());
    expect(await screen.findByText("Aucun téléchargement terminé.")).toBeTruthy();
    fireEvent.click(screen.getByText("Vider la liste"));
    expect(props.onClear).toHaveBeenCalled();
  });
});
