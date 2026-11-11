import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor, cleanup } from "@testing-library/react";
import type { LinkProbe, MediaFormat } from "../api";

vi.mock("../api");
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
import * as api from "../api";
import { NewPage, type ExternalEvent } from "./NewPage";

const mf = (o: Partial<MediaFormat>): MediaFormat => ({
  index: 0, kind: "av", format_id: "x", height: null, label: "x", container: "mp4",
  codecs: "avc1 · aac", fps: null, est_size: null, recommended: false, ...o,
});
const probe: Extract<LinkProbe, { type: "media" }> = {
  type: "media", probe_id: "p1", url: "https://www.youtube.com/watch?v=1", source: "ytdlp",
  info: { title: "Concert Live", uploader: "Chaîne", thumbnail: null, duration_s: 3600, upload_date: null, subtitle_langs: [], can_cut: true },
  formats: [mf({ index: 0, label: "1080p", height: 1080, recommended: true }), mf({ index: 1, label: "720p", height: 720 })],
  file_name: "Concert Live", video_dir: "D:/Vidéos", music_dir: "D:/Musique",
};
const URL1 = "https://www.youtube.com/watch?v=1";

function setup(props: Partial<Parameters<typeof NewPage>[0]> = {}) {
  const onDone = vi.fn();
  const utils = render(<NewPage external={null} startInBatch={false} onDone={onDone} {...props} />);
  return { onDone, ...utils };
}
async function analyze() {
  fireEvent.change(screen.getByPlaceholderText("Colle un lien (vidéo, page, fichier)…"), { target: { value: URL1 } });
  fireEvent.keyDown(screen.getByPlaceholderText("Colle un lien (vidéo, page, fichier)…"), { key: "Enter" });
  await screen.findByText("Concert Live");
}

beforeEach(() => { vi.mocked(api.retryCapture).mockReset(); vi.mocked(api.probeLink).mockReset(); vi.mocked(api.confirmLink).mockReset(); vi.mocked(api.addBatchDownloads).mockReset(); });
afterEach(cleanup);

describe("NewPage", () => {
  it("Entrée lance l'analyse et affiche le panneau média", async () => {
    vi.mocked(api.probeLink).mockResolvedValue(probe);
    setup();
    expect(screen.getByText("Colle un lien : FlashGet détecte s'il s'agit d'une vidéo ou d'un fichier.")).not.toBeNull();
    await analyze();
    expect(api.probeLink).toHaveBeenCalledWith(URL1);
  });

  it("erreur d'analyse : message et Réessayer relance", async () => {
    vi.mocked(api.probeLink).mockRejectedValueOnce("Lien illisible").mockResolvedValueOnce(probe);
    setup();
    fireEvent.change(screen.getByPlaceholderText("Colle un lien (vidéo, page, fichier)…"), { target: { value: URL1 } });
    fireEvent.keyDown(screen.getByPlaceholderText("Colle un lien (vidéo, page, fichier)…"), { key: "Enter" });
    await screen.findByText("Lien illisible");
    fireEvent.click(screen.getByRole("button", { name: "Réessayer" }));
    await screen.findByText("Concert Live");
    expect(api.probeLink).toHaveBeenCalledTimes(2);
  });

  it("Télécharger appelle confirmLink avec start_now=true puis onDone", async () => {
    vi.mocked(api.probeLink).mockResolvedValue(probe);
    vi.mocked(api.confirmLink).mockResolvedValue({} as never);
    const { onDone } = setup();
    await analyze();
    fireEvent.click(screen.getByText("720p"));
    fireEvent.click(screen.getByRole("button", { name: /^Télécharger/ }));
    await waitFor(() => expect(onDone).toHaveBeenCalledWith());
    expect(api.confirmLink).toHaveBeenCalledWith({
      probe_id: "p1", format_index: 1, subtitles: false, cut: null, dest_dir: "D:/Vidéos", file_name: "Concert Live.mp4", start_now: true,
    });
  });

  it("découpe sans fin : end vaut null", async () => {
    vi.mocked(api.probeLink).mockResolvedValue(probe);
    vi.mocked(api.confirmLink).mockResolvedValue({} as never);
    setup();
    await analyze();
    fireEvent.click(screen.getByRole("switch", { name: "Découper un extrait" }));
    fireEvent.click(screen.getByRole("button", { name: "Ajouter à la file" }));
    await waitFor(() => expect(api.confirmLink).toHaveBeenCalled());
    expect(vi.mocked(api.confirmLink).mock.calls[0][0]).toMatchObject({ cut: { start: "0:00", end: null }, start_now: false });
  });

  it("échec de confirmLink : message et choix conservé", async () => {
    vi.mocked(api.probeLink).mockResolvedValue(probe);
    vi.mocked(api.confirmLink).mockRejectedValue("Disque plein");
    const { onDone } = setup();
    await analyze();
    fireEvent.click(screen.getByText("720p"));
    fireEvent.click(screen.getByRole("button", { name: /^Télécharger/ }));
    await screen.findByText("Disque plein");
    expect(onDone).not.toHaveBeenCalled();
    expect((screen.getByLabelText("Nom du fichier") as HTMLInputElement).value).toBe("Concert Live.mp4");
    fireEvent.click(screen.getByRole("button", { name: /^Télécharger/ }));
    await waitFor(() => expect(api.confirmLink).toHaveBeenCalledTimes(2));
    expect(vi.mocked(api.confirmLink).mock.calls[1][0].format_index).toBe(1);
  });

  it("événement externe probed remplace la page", async () => {
    const ev: ExternalEvent = { seq: 1, kind: "probed", probe };
    const { rerender, onDone } = setup();
    rerender(<NewPage external={ev} startInBatch={false} onDone={onDone} />);
    await screen.findByText("Concert Live");
    expect(api.probeLink).not.toHaveBeenCalled();
  });

  it("external analyze remplit le champ et lance l'analyse", async () => {
    vi.mocked(api.probeLink).mockResolvedValue(probe);
    const { rerender, onDone } = setup();
    rerender(<NewPage external={{ seq: 1, kind: "analyze", url: URL1 }} startInBatch={false} onDone={onDone} />);
    await screen.findByText("Concert Live");
    expect(vi.mocked(api.probeLink).mock.calls[0][0]).toBe(URL1);
    expect((screen.getByPlaceholderText("Colle un lien (vidéo, page, fichier)…") as HTMLInputElement).value).toBe(URL1);
  });

  it("événement externe probing puis error", async () => {
    const { rerender, onDone } = setup();
    rerender(<NewPage external={{ seq: 1, kind: "probing", url: URL1 }} startInBatch={false} onDone={onDone} />);
    await screen.findByText("Recherche des formats disponibles…");
    rerender(<NewPage external={{ seq: 2, kind: "error", url: URL1, message: "Échec ext" }} startInBatch={false} onDone={onDone} />);
    await screen.findByText("Échec ext");
  });

  it("Plusieurs liens : addBatchDownloads(text, '', 'audio', true) puis onDone(résumé)", async () => {
    vi.mocked(api.addBatchDownloads).mockResolvedValue({ added: 2, skipped_invalid: 1, skipped_duplicate: 1, failed: 0, not_replaced: 1 });
    const { onDone } = setup();
    fireEvent.click(screen.getByRole("button", { name: "Plusieurs liens" }));
    fireEvent.change(screen.getByPlaceholderText("Un lien par ligne…"), { target: { value: "https://a.com/1\nhttps://a.com/2" } });
    fireEvent.change(screen.getByLabelText("Type"), { target: { value: "audio" } });
    fireEvent.click(screen.getByRole("button", { name: "Ajouter à la file" }));
    await waitFor(() => expect(onDone).toHaveBeenCalledWith("2 ajouté(s) · 1 doublon(s) · 1 ignoré(s) · 1 renommé(s), Corbeille indisponible"));
    expect(api.addBatchDownloads).toHaveBeenCalledWith("https://a.com/1\nhttps://a.com/2", "", "audio", true);
  });

  it("Ctrl+Entrée dans le lot télécharge (hold=false)", async () => {
    vi.mocked(api.addBatchDownloads).mockResolvedValue({ added: 1, skipped_invalid: 0, skipped_duplicate: 0, failed: 0, not_replaced: 0 });
    setup({ startInBatch: true });
    const ta = screen.getByPlaceholderText("Un lien par ligne…");
    fireEvent.change(ta, { target: { value: "https://a.com/1" } });
    fireEvent.keyDown(ta, { key: "Enter", ctrlKey: true });
    await waitFor(() => expect(api.addBatchDownloads).toHaveBeenCalledWith("https://a.com/1", "", "direct", false));
  });

  it("startInBatch ouvre directement le lot", () => {
    setup({ startInBatch: true });
    expect(screen.getByPlaceholderText("Un lien par ligne…")).not.toBeNull();
    expect(screen.getByRole("button", { name: "Un seul lien" })).not.toBeNull();
  });
});

describe("NewPage : échec d'une capture de l'extension", () => {
  const err = (o: Record<string, unknown> = {}): ExternalEvent =>
    ({ seq: 1, kind: "error", url: URL1, message: "HTTP 403", captureId: "cap-1", origin: "exemple.com", ...o }) as ExternalEvent;
  const show = (ev: ExternalEvent) => {
    const { rerender, onDone } = setup();
    rerender(<NewPage external={ev} startInBatch={false} onDone={onDone} />);
    return { rerender, onDone };
  };

  it("affiche l'origine en mono et le texte explicatif", async () => {
    show(err());
    await screen.findByText("HTTP 403");
    expect(screen.getByText(/Envoyé par l'extension depuis/)).toBeTruthy();
    expect(screen.getByText("exemple.com").className).toContain("font-mono");
    expect(screen.getByText(/renvoie aussi les cookies et la page d'origine/)).toBeTruthy();
  });

  it("sans origine : variante sans « depuis »", async () => {
    show(err({ origin: undefined }));
    await screen.findByText("HTTP 403");
    expect(screen.getByText(/Envoyé par l'extension/)).toBeTruthy();
    expect(screen.queryByText(/depuis/)).toBeNull();
  });

  it("Réessayer appelle retryCapture, pas probeLink, et attend les événements", async () => {
    vi.mocked(api.retryCapture).mockResolvedValue(probe);
    show(err());
    fireEvent.click(await screen.findByRole("button", { name: "Réessayer" }));
    await waitFor(() => expect(api.retryCapture).toHaveBeenCalledWith("cap-1", URL1));
    expect(api.probeLink).not.toHaveBeenCalled();
    await screen.findByText("Recherche des formats disponibles…");
  });

  it("l'échec de retryCapture est affiché", async () => {
    vi.mocked(api.retryCapture).mockRejectedValue("Capture expirée");
    show(err());
    fireEvent.click(await screen.findByRole("button", { name: "Réessayer" }));
    await screen.findByText("Capture expirée");
  });

  it("sans captureId : ancien chemin et pas de texte", async () => {
    vi.mocked(api.probeLink).mockResolvedValue(probe);
    show(err({ captureId: undefined, origin: undefined }));
    await screen.findByText("HTTP 403");
    expect(screen.queryByText(/Envoyé par l'extension/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Réessayer" }));
    await waitFor(() => expect(api.probeLink).toHaveBeenCalledWith(URL1));
    expect(api.retryCapture).not.toHaveBeenCalled();
  });
});
