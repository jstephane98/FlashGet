import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor, act } from "@testing-library/react";
import type { Settings } from "../../types";

vi.mock("../../api");
import * as api from "../../api";
import type { UpdateProgress } from "../../api";
import { YtdlpEngineCard } from "./YtdlpEngineCard";
import { resetYtdlpUpdateStore, applyUpdated } from "./ytdlpUpdateStore";

const S: Settings = {
  max_concurrent: 3, max_speed_bps: 0, connections: 8, retries: 5, auto_resume: true,
  conflict: "rename", close_to_tray: true, notify_complete: true, notify_error: true,
  video_max_height: 1080, audio_format: "mp3", audio_kbps: 320, sub_langs: ["fr"],
  ytdlp_auto_update: true, app_update_check: true,
};
const save = vi.fn();
const onUpdated = vi.fn();
const unlisten = vi.fn();
let progressCb: (p: UpdateProgress) => void = () => {};

function mount(latest: string | null = null, settings: Settings = S) {
  return render(<YtdlpEngineCard settings={settings} save={save} latest={latest} onUpdated={onUpdated} />);
}

beforeEach(() => {
  resetYtdlpUpdateStore();
  vi.mocked(api.getYtdlpVersion).mockResolvedValue("2026.10.30");
  vi.mocked(api.getYtdlpLastCheck).mockResolvedValue(null);
  vi.mocked(api.checkYtdlpUpdate).mockResolvedValue({ current: "2026.10.30", latest: "2026.10.30", available: false, last_check: null });
  vi.mocked(api.updateYtdlp).mockResolvedValue({ version: "2026.11.08", pending: false });
  vi.mocked(api.onYtdlpUpdateProgress).mockImplementation(async (cb) => { progressCb = cb; return unlisten; });
});
afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("YtdlpEngineCard", () => {
  it("à l'ouverture : version en mono, aucune recherche automatique", async () => {
    mount();
    const v = await screen.findByText("2026.10.30");
    expect(v.className).toContain("font-mono");
    expect(api.checkYtdlpUpdate).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Rechercher" })).toBeTruthy();
  });

  it("recherche sans résultat : À jour", async () => {
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Rechercher" }));
    expect(await screen.findByText("À jour")).toBeTruthy();
    expect(onUpdated).toHaveBeenCalledWith(null);
  });

  it("recherche avec résultat : prévient le parent", async () => {
    vi.mocked(api.checkYtdlpUpdate).mockResolvedValue({ current: "2026.10.30", latest: "2026.11.08", available: true, last_check: 1 });
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Rechercher" }));
    await waitFor(() => expect(onUpdated).toHaveBeenCalledWith("2026.11.08"));
  });

  it("disponible : nouvelle version et bouton Mettre à jour", async () => {
    mount("2026.11.08");
    expect(await screen.findByText("2026.11.08")).toBeTruthy();
    expect(screen.getByText(/Nouvelle version disponible/)).toBeTruthy();
    expect(screen.getByText("Mise à jour")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Mettre à jour" })).toBeTruthy();
  });

  it("en cours : barre de progression et bouton désactivé", async () => {
    vi.mocked(api.updateYtdlp).mockReturnValue(new Promise(() => {}));
    mount("2026.11.08");
    fireEvent.click(await screen.findByRole("button", { name: "Mettre à jour" }));
    await waitFor(() => expect(api.onYtdlpUpdateProgress).toHaveBeenCalled());
    act(() => progressCb({ downloaded: 1_048_576, total: 2_097_152 }));
    expect(screen.getByText(/Mise à jour de yt-dlp vers/)).toBeTruthy();
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("50");
    expect((screen.getByRole("button", { name: "Mettre à jour" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("en cours sans total : seulement la quantité téléchargée", async () => {
    vi.mocked(api.updateYtdlp).mockReturnValue(new Promise(() => {}));
    mount("2026.11.08");
    fireEvent.click(await screen.findByRole("button", { name: "Mettre à jour" }));
    await waitFor(() => expect(api.onYtdlpUpdateProgress).toHaveBeenCalled());
    act(() => progressCb({ downloaded: 1_048_576, total: null }));
    const el = screen.getByTestId("ytdlp-progress");
    expect(el.textContent).not.toContain("/");
    expect(el.textContent).toContain("1");
  });

  it("succès : la version affichée change et le parent est prévenu", async () => {
    const { rerender } = mount("2026.11.08");
    vi.mocked(api.getYtdlpVersion).mockResolvedValue("2026.11.08");
    fireEvent.click(await screen.findByRole("button", { name: "Mettre à jour" }));
    await waitFor(() => expect(onUpdated).toHaveBeenCalledWith(null));
    rerender(<YtdlpEngineCard settings={S} save={save} latest={null} onUpdated={onUpdated} />);
    expect(await screen.findByText("2026.11.08")).toBeTruthy();
  });

  it("échec : message de Rust, version conservée, Réessayer", async () => {
    vi.mocked(api.updateYtdlp).mockRejectedValue("fichier téléchargé invalide");
    mount("2026.11.08");
    fireEvent.click(await screen.findByRole("button", { name: "Mettre à jour" }));
    expect(await screen.findByText(/fichier téléchargé invalide/)).toBeTruthy();
    expect(screen.getByText(/La mise à jour a échoué : fichier téléchargé invalide\. La version actuelle reste utilisée\./)).toBeTruthy();
    expect(screen.getByText("Échec")).toBeTruthy();
    expect(screen.getByText("2026.10.30")).toBeTruthy();
    vi.mocked(api.updateYtdlp).mockResolvedValue({ version: "2026.11.08", pending: false });
    fireEvent.click(screen.getByRole("button", { name: "Réessayer" }));
    await waitFor(() => expect(api.updateYtdlp).toHaveBeenCalledTimes(2));
  });

  it("en attente (pending) : sera utilisée au prochain démarrage", async () => {
    vi.mocked(api.updateYtdlp).mockResolvedValue({ version: "2026.11.08", pending: true });
    mount("2026.11.08");
    fireEvent.click(await screen.findByRole("button", { name: "Mettre à jour" }));
    expect(await screen.findByText(/sera utilisée au prochain démarrage de FlashGet/)).toBeTruthy();
    expect(onUpdated).toHaveBeenCalledWith(null);
  });

  it("mise à jour automatique au démarrage : la version est relue", async () => {
    mount();
    expect(await screen.findByText("2026.10.30")).toBeTruthy();
    vi.mocked(api.getYtdlpVersion).mockResolvedValue("2026.11.08");
    // L'écoute est faite par App ; elle appelle le store.
    await act(async () => { await applyUpdated({ version: "2026.11.08", pending: false }, onUpdated); });
    expect(await screen.findByText("2026.11.08")).toBeTruthy();
  });

  it("mise à jour automatique en attente avant l'ouverture de la carte : message conservé", async () => {
    await applyUpdated({ version: "2026.11.08", pending: true }, () => {});
    mount();
    expect(await screen.findByText(/sera utilisée au prochain démarrage de FlashGet/)).toBeTruthy();
  });

  it("recherche dont la dernière version est déjà en attente : null, message conservé", async () => {
    await applyUpdated({ version: "2026.11.08", pending: true }, () => {});
    vi.mocked(api.checkYtdlpUpdate).mockResolvedValue({ current: "2026.10.30", latest: "2026.11.08", available: true, last_check: 1 });
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Rechercher" }));
    await waitFor(() => expect(onUpdated).toHaveBeenCalledWith(null));
    expect(onUpdated).not.toHaveBeenCalledWith("2026.11.08");
    expect(screen.getByText(/sera utilisée au prochain démarrage de FlashGet/)).toBeTruthy();
    expect(screen.queryByText(/Nouvelle version disponible/)).toBeNull();
  });

  it("démonter puis remonter pendant la mise à jour garde la progression, un seul appel", async () => {
    vi.mocked(api.updateYtdlp).mockReturnValue(new Promise(() => {}));
    const first = mount("2026.11.08");
    fireEvent.click(await screen.findByRole("button", { name: "Mettre à jour" }));
    await waitFor(() => expect(api.onYtdlpUpdateProgress).toHaveBeenCalled());
    act(() => progressCb({ downloaded: 1_048_576, total: 2_097_152 }));
    first.unmount();
    mount("2026.11.08");
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("50");
    expect((screen.getByRole("button", { name: "Mettre à jour" }) as HTMLButtonElement).disabled).toBe(true);
    expect(api.updateYtdlp).toHaveBeenCalledTimes(1);
  });

  it("l'interrupteur enregistre ytdlp_auto_update", async () => {
    mount();
    fireEvent.click(await screen.findByRole("switch", { name: "Mettre à jour yt-dlp automatiquement" }));
    expect(save).toHaveBeenCalledWith({ ytdlp_auto_update: false });
  });
});
