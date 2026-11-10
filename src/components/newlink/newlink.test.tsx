import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/react";
import type { LinkProbe, MediaFormat } from "../../api";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
import { open } from "@tauri-apps/plugin-dialog";
import { FormatList } from "./FormatList";
import { MediaPanel } from "./MediaPanel";
import { FilePanel } from "./FilePanel";
import { DestinationFields } from "./DestinationFields";

const mf = (o: Partial<MediaFormat>): MediaFormat => ({
  index: 0, kind: "av", format_id: "x", height: null, label: "x", container: "mp4",
  codecs: "avc1 · aac", fps: null, est_size: null, recommended: false, ...o,
});
const FORMATS: MediaFormat[] = [
  mf({ index: 0, label: "1080p", height: 1080, recommended: true, est_size: 1.4 * 1024 ** 3, fps: 29.97 }),
  mf({ index: 1, label: "720p", height: 720 }),
  mf({ index: 2, kind: "audio", format_id: "mp3", label: "MP3", container: "mp3" }),
  mf({ index: 3, kind: "audio", format_id: "m4a", label: "M4A", container: "m4a", recommended: true }),
];
const media = (o: Partial<Extract<LinkProbe, { type: "media" }>> = {}, info: Record<string, unknown> = {}): Extract<LinkProbe, { type: "media" }> => ({
  type: "media", probe_id: "p", url: "https://www.youtube.com/watch?v=1", source: "ytdlp",
  info: { title: "Concert Live", uploader: "Chaîne", thumbnail: null, duration_s: 3600, upload_date: "2024-01-02", subtitle_langs: ["fr", "en"], can_cut: true, ...info },
  formats: FORMATS, file_name: "Concert Live", video_dir: "C:/Videos", music_dir: "C:/Music", ...o,
});
const file = (): Extract<LinkProbe, { type: "file" }> => ({
  type: "file", probe_id: "f", url: "https://a.com/x.iso", file_name: "x.iso", total_bytes: 6.1 * 1024 ** 3,
  supports_ranges: true, category: "program", default_dir: "C:/Dl",
});

beforeEach(() => { vi.mocked(open).mockReset(); });
afterEach(() => cleanup());

describe("newlink", () => {
  it("FormatList : badge recommandé et sélection", () => {
    const onSelect = vi.fn();
    render(<FormatList formats={FORMATS.slice(0, 2)} selected={0} onSelect={onSelect} />);
    expect(screen.getByText("Recommandé")).not.toBeNull();
    fireEvent.click(screen.getByText("720p"));
    expect(onSelect).toHaveBeenCalledWith(1);
  });

  it("MediaPanel : onglets disponibles seulement", () => {
    render(<MediaPanel probe={media()} busy={false} onConfirm={vi.fn()} />);
    expect(screen.getByText("Vidéo + audio")).not.toBeNull();
    expect(screen.getByText("Audio seul")).not.toBeNull();
    expect(screen.queryByText("Vidéo seule")).toBeNull();
  });

  it("MediaPanel : passer en Audio seul choisit le format recommandé, extension et dossier Musique", () => {
    const onConfirm = vi.fn();
    render(<MediaPanel probe={media({ formats: [FORMATS[0], FORMATS[2], { ...FORMATS[3], recommended: false }] })} busy={false} onConfirm={onConfirm} />);
    fireEvent.click(screen.getByText("Audio seul"));
    expect((screen.getByLabelText("Nom du fichier") as HTMLInputElement).value).toBe("Concert Live.mp3");
    expect((screen.getByLabelText("Enregistrer dans") as HTMLInputElement).value).toBe("C:/Music");
    fireEvent.click(screen.getByRole("button", { name: /^Télécharger/ }));
    expect(onConfirm).toHaveBeenCalledWith({ formatIndex: 2, subtitles: false, cut: null, dir: "C:/Music", fileName: "Concert Live.mp3" }, true);
  });

  it("MediaPanel : Télécharger envoie le choix avec startNow=true", () => {
    const onConfirm = vi.fn();
    render(<MediaPanel probe={media()} busy={false} onConfirm={onConfirm} />);
    fireEvent.click(screen.getByRole("button", { name: /^Télécharger/ }));
    expect(onConfirm).toHaveBeenCalledWith({ formatIndex: 0, subtitles: false, cut: null, dir: "C:/Videos", fileName: "Concert Live.mp4" }, true);
  });

  it("MediaPanel : Ajouter à la file envoie startNow=false", () => {
    const onConfirm = vi.fn();
    render(<MediaPanel probe={media()} busy={false} onConfirm={onConfirm} />);
    fireEvent.click(screen.getByRole("button", { name: "Ajouter à la file" }));
    expect(onConfirm.mock.calls[0][1]).toBe(false);
  });

  it("MediaPanel : découpe invalide affiche le message et désactive les boutons", () => {
    render(<MediaPanel probe={media()} busy={false} onConfirm={vi.fn()} />);
    fireEvent.click(screen.getByRole("switch", { name: "Découper un extrait" }));
    fireEvent.change(screen.getByLabelText("Début"), { target: { value: "2:00" } });
    fireEvent.change(screen.getByLabelText("Fin"), { target: { value: "1:00" } });
    expect(screen.getByText("La fin doit être après le début")).not.toBeNull();
    expect((screen.getByRole("button", { name: "Ajouter à la file" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("button", { name: /^Télécharger/ }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("MediaPanel : pas d'option découpe si can_cut=false", () => {
    render(<MediaPanel probe={media({}, { can_cut: false })} busy={false} onConfirm={vi.fn()} />);
    expect(screen.queryByText("Découper un extrait")).toBeNull();
  });

  it("MediaPanel : nom modifié à la main conservé au changement de format", () => {
    render(<MediaPanel probe={media()} busy={false} onConfirm={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("Nom du fichier"), { target: { value: "mon.mp4" } });
    fireEvent.click(screen.getByText("Audio seul"));
    expect((screen.getByLabelText("Nom du fichier") as HTMLInputElement).value).toBe("mon.mp4");
  });

  it("MediaPanel : sous-titres désactivés sur Audio seul et non envoyés", () => {
    const onConfirm = vi.fn();
    render(<MediaPanel probe={media()} busy={false} onConfirm={onConfirm} />);
    fireEvent.click(screen.getByRole("switch", { name: "Intégrer les sous-titres" }));
    fireEvent.click(screen.getByText("Audio seul"));
    expect((screen.getByRole("switch", { name: "Intégrer les sous-titres" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: /^Télécharger/ }));
    expect(onConfirm.mock.calls[0][0].subtitles).toBe(false);
  });

  it("MediaPanel : sous-titres envoyés sur l'onglet vidéo", () => {
    const onConfirm = vi.fn();
    render(<MediaPanel probe={media()} busy={false} onConfirm={onConfirm} />);
    fireEvent.click(screen.getByRole("switch", { name: "Intégrer les sous-titres" }));
    fireEvent.click(screen.getByRole("button", { name: /^Télécharger/ }));
    expect(onConfirm.mock.calls[0][0].subtitles).toBe(true);
  });

  it("FilePanel : infos et confirmation", () => {
    const onConfirm = vi.fn();
    render(<FilePanel probe={file()} busy={false} onConfirm={onConfirm} />);
    expect(screen.getByText(/6,1 Go · Programme/)).not.toBeNull();
    expect(screen.getByText(/reprise possible/)).not.toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /^Télécharger/ }));
    expect(onConfirm).toHaveBeenCalledWith({ dir: "C:/Dl", fileName: "x.iso" }, true);
  });

  it("Parcourir remplace le dossier, annuler ne change rien", async () => {
    const onDir = vi.fn();
    vi.mocked(open).mockResolvedValueOnce("D:/X").mockResolvedValueOnce(null);
    render(<DestinationFields dir="C:/a" fileName="f" onDir={onDir} onFileName={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Parcourir" }));
    await waitFor(() => expect(onDir).toHaveBeenCalledWith("D:/X"));
    expect(open).toHaveBeenCalledWith({ directory: true, defaultPath: "C:/a" });
    fireEvent.click(screen.getByRole("button", { name: "Parcourir" }));
    await waitFor(() => expect(open).toHaveBeenCalledTimes(2));
    expect(onDir).toHaveBeenCalledTimes(1);
  });
});
