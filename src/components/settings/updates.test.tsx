import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor, act } from "@testing-library/react";
import type { Settings } from "../../types";

vi.mock("../../api");
import * as api from "../../api";
import type { AppUpdateInfo, UpdateProgress } from "../../api";
import { UpdatesSection } from "./UpdatesSection";
import { SettingsNav } from "./SettingsNav";
import { formatLastCheck } from "./updateFormat";
import { resetAppUpdateStore } from "./appUpdateStore";
import { SettingsPage } from "../../pages/SettingsPage";
import { Sidebar } from "../shell/Sidebar";

const S: Settings = {
  max_concurrent: 3, max_speed_bps: 0, connections: 8, retries: 5, auto_resume: true,
  conflict: "rename", close_to_tray: true, notify_complete: true, notify_error: true,
  video_max_height: 1080, audio_format: "mp3", audio_kbps: 320, sub_langs: ["fr"],
  ytdlp_auto_update: true, app_update_check: true,
};
const INFO: AppUpdateInfo = { version: "2.1.0", date: "2026-11-12T10:00:00Z", notes: "Sites ignorés pour l'extension." };
const save = vi.fn();
const onChanged = vi.fn();
const unlisten = vi.fn();
let progressCb: (p: UpdateProgress) => void = () => {};

function mount(updates: { app: AppUpdateInfo | null; ytdlp: string | null } = { app: null, ytdlp: null }, recording = false) {
  return render(<UpdatesSection settings={S} save={save} updates={updates} onUpdatesChanged={onChanged} recording={recording} />);
}

beforeEach(() => {
  resetAppUpdateStore();
  vi.mocked(api.appVersion).mockResolvedValue("2.0.0");
  vi.mocked(api.getYtdlpVersion).mockResolvedValue("2026.10.30");
  vi.mocked(api.getAppLastCheck).mockResolvedValue(null);
  vi.mocked(api.checkAppUpdate).mockResolvedValue(null);
  vi.mocked(api.downloadAppUpdate).mockResolvedValue(undefined);
  vi.mocked(api.installAppUpdate).mockResolvedValue(undefined);
  vi.mocked(api.onAppUpdateProgress).mockImplementation(async (cb) => { progressCb = cb; return unlisten; });
});
afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("UpdatesSection", () => {
  it("n'appelle pas checkAppUpdate à l'ouverture et propose Rechercher", async () => {
    mount();
    expect(await screen.findByText("2.0.0")).toBeTruthy();
    expect(api.checkAppUpdate).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Rechercher" })).toBeTruthy();
  });

  it("recherche sans résultat : FlashGet est à jour", async () => {
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Rechercher" }));
    expect(await screen.findByText("FlashGet est à jour")).toBeTruthy();
    expect(onChanged).toHaveBeenCalledWith({ app: null });
  });

  it("recherche en cours : bouton désactivé", async () => {
    let resolve: (v: AppUpdateInfo | null) => void = () => {};
    vi.mocked(api.checkAppUpdate).mockReturnValue(new Promise((r) => { resolve = r; }));
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Rechercher" }));
    expect(await screen.findByText("Recherche d'une nouvelle version…")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Rechercher" }) as HTMLButtonElement).disabled).toBe(true);
    await act(async () => { resolve(null); });
  });

  it("recherche qui trouve une version la signale à App", async () => {
    vi.mocked(api.checkAppUpdate).mockResolvedValue(INFO);
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Rechercher" }));
    await waitFor(() => expect(onChanged).toHaveBeenCalledWith({ app: INFO }));
  });

  it("disponible : version en mono, nouveautés et bouton", async () => {
    mount({ app: INFO, ytdlp: null });
    const v = await screen.findByText("2.1.0");
    expect(v.className).toContain("font-mono");
    expect(screen.getByText("Nouveautés")).toBeTruthy();
    expect(screen.getByText("Sites ignorés pour l'extension.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Télécharger et installer" })).toBeTruthy();
    expect(api.checkAppUpdate).not.toHaveBeenCalled();
  });

  it("téléchargement : barre et progression en français", async () => {
    vi.mocked(api.downloadAppUpdate).mockReturnValue(new Promise(() => {}));
    mount({ app: INFO, ytdlp: null });
    fireEvent.click(await screen.findByRole("button", { name: "Télécharger et installer" }));
    await waitFor(() => expect(api.downloadAppUpdate).toHaveBeenCalled());
    act(() => progressCb({ downloaded: 18_400_000, total: 29_600_000 }));
    expect(screen.getByText("17,5 Mo / 28,2 Mo · 62 %")).toBeTruthy();
    expect(screen.getByRole("progressbar")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Télécharger et installer" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("total inconnu : seulement les Mo téléchargés", async () => {
    vi.mocked(api.downloadAppUpdate).mockReturnValue(new Promise(() => {}));
    mount({ app: INFO, ytdlp: null });
    fireEvent.click(await screen.findByRole("button", { name: "Télécharger et installer" }));
    await waitFor(() => expect(api.downloadAppUpdate).toHaveBeenCalled());
    act(() => progressCb({ downloaded: 5_000_000, total: null }));
    expect(screen.getByText("4,8 Mo")).toBeTruthy();
    expect(screen.queryByText(/%/)).toBeNull();
  });

  it("prête : Installer et redémarrer appelle installAppUpdate", async () => {
    mount({ app: INFO, ytdlp: null });
    fireEvent.click(await screen.findByRole("button", { name: "Télécharger et installer" }));
    fireEvent.click(await screen.findByRole("button", { name: "Installer et redémarrer" }));
    expect(api.installAppUpdate).toHaveBeenCalled();
    expect(screen.getByText(/Les téléchargements en cours sont mis en pause et reprendront\./)).toBeTruthy();
    expect(screen.queryByText(/enregistrement en direct/)).toBeNull();
  });

  it("prête avec un enregistrement actif : avertissement ajouté", async () => {
    mount({ app: INFO, ytdlp: null }, true);
    fireEvent.click(await screen.findByRole("button", { name: "Télécharger et installer" }));
    await screen.findByRole("button", { name: "Installer et redémarrer" });
    expect(screen.getByText(/Un enregistrement en direct sera interrompu, puis finalisé au redémarrage\./)).toBeTruthy();
  });

  it("échec : Réessayer relance checkAppUpdate", async () => {
    vi.mocked(api.checkAppUpdate).mockRejectedValueOnce("réseau").mockResolvedValue(null);
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Rechercher" }));
    expect(await screen.findByText("Impossible de rechercher les mises à jour")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Réessayer" }));
    await waitFor(() => expect(api.checkAppUpdate).toHaveBeenCalledTimes(2));
    expect(await screen.findByText("FlashGet est à jour")).toBeTruthy();
  });

  it("l'interrupteur de démarrage enregistre app_update_check: false", async () => {
    mount();
    fireEvent.click(await screen.findByRole("switch", { name: "Rechercher les mises à jour au démarrage" }));
    expect(save).toHaveBeenCalledWith({ app_update_check: false });
  });
});

describe("état conservé et cas d'erreur", () => {
  it("la progression survit au démontage de la section", async () => {
    vi.mocked(api.downloadAppUpdate).mockReturnValue(new Promise(() => {}));
    const first = mount({ app: INFO, ytdlp: null });
    fireEvent.click(await screen.findByRole("button", { name: "Télécharger et installer" }));
    await waitFor(() => expect(api.downloadAppUpdate).toHaveBeenCalled());
    first.unmount();
    act(() => progressCb({ downloaded: 18_400_000, total: 29_600_000 }));
    mount({ app: INFO, ytdlp: null });
    expect(await screen.findByText("17,5 Mo / 28,2 Mo · 62 %")).toBeTruthy();
    expect(api.downloadAppUpdate).toHaveBeenCalledTimes(1);
  });

  it("échec du téléchargement puis Réessayer relance downloadAppUpdate", async () => {
    vi.mocked(api.downloadAppUpdate).mockRejectedValueOnce("disque plein").mockResolvedValue(undefined);
    mount({ app: INFO, ytdlp: null });
    fireEvent.click(await screen.findByRole("button", { name: "Télécharger et installer" }));
    expect(await screen.findByText("Impossible de télécharger la mise à jour")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Réessayer" }));
    await waitFor(() => expect(api.downloadAppUpdate).toHaveBeenCalledTimes(2));
    expect(await screen.findByRole("button", { name: "Installer et redémarrer" })).toBeTruthy();
  });

  it("échec de l'installation puis Réessayer relance installAppUpdate", async () => {
    vi.mocked(api.installAppUpdate).mockRejectedValueOnce("verrouillé").mockResolvedValue(undefined);
    mount({ app: INFO, ytdlp: null });
    fireEvent.click(await screen.findByRole("button", { name: "Télécharger et installer" }));
    fireEvent.click(await screen.findByRole("button", { name: "Installer et redémarrer" }));
    expect(await screen.findByText("Impossible d'installer la mise à jour")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Réessayer" }));
    await waitFor(() => expect(api.installAppUpdate).toHaveBeenCalledTimes(2));
  });

  it("installation en cours : bouton désactivé « Installation… »", async () => {
    vi.mocked(api.installAppUpdate).mockReturnValue(new Promise(() => {}));
    mount({ app: INFO, ytdlp: null });
    fireEvent.click(await screen.findByRole("button", { name: "Télécharger et installer" }));
    fireEvent.click(await screen.findByRole("button", { name: "Installer et redémarrer" }));
    const btn = (await screen.findByRole("button", { name: "Installation…" })) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  it("installation réussie sans fermeture : retour à l'état prêt", async () => {
    mount({ app: INFO, ytdlp: null });
    fireEvent.click(await screen.findByRole("button", { name: "Télécharger et installer" }));
    fireEvent.click(await screen.findByRole("button", { name: "Installer et redémarrer" }));
    expect(await screen.findByRole("button", { name: "Installer et redémarrer" })).toBeTruthy();
  });

  it("le désabonnement de la progression est appelé après le téléchargement", async () => {
    mount({ app: INFO, ytdlp: null });
    fireEvent.click(await screen.findByRole("button", { name: "Télécharger et installer" }));
    await screen.findByRole("button", { name: "Installer et redémarrer" });
    expect(unlisten).toHaveBeenCalled();
  });

  it("Dernière recherche apparaît après une recherche", async () => {
    vi.mocked(api.getAppLastCheck).mockResolvedValueOnce(null).mockResolvedValue(Math.floor(Date.now() / 1000));
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Rechercher" }));
    expect(await screen.findByText(/Dernière recherche/)).toBeTruthy();
  });

  it("un résultat de recherche est rapporté même si la section est démontée", async () => {
    let resolve: (v: AppUpdateInfo | null) => void = () => {};
    vi.mocked(api.checkAppUpdate).mockReturnValue(new Promise((r) => { resolve = r; }));
    const view = mount();
    fireEvent.click(await screen.findByRole("button", { name: "Rechercher" }));
    view.unmount();
    await act(async () => { resolve(INFO); });
    expect(onChanged).toHaveBeenCalledWith({ app: INFO });
  });

  it("SettingsPage : point sur Vidéo quand yt-dlp a une mise à jour", async () => {
    vi.mocked(api.getSettings).mockResolvedValue({ ...S });
    vi.mocked(api.getAutostart).mockResolvedValue(false);
    vi.mocked(api.getEndAction).mockResolvedValue("none");
    render(<SettingsPage onSaved={() => {}} updates={{ app: null, ytdlp: "2026.11.08" }} onUpdatesChanged={() => {}} recording={false} bridgeStatus={null} />);
    const btn = await screen.findByRole("button", { name: /Vidéo/ });
    expect(btn.querySelector('[aria-label="Mise à jour disponible"]')).toBeTruthy();
    expect(screen.getByRole("button", { name: /Mises à jour/ }).querySelector('[aria-label="Mise à jour disponible"]')).toBeNull();
  });
});

describe("points d'attention", () => {
  it("SettingsNav : point sur Mises à jour seulement", () => {
    render(<SettingsNav active="general" onSelect={() => {}} attention={{ updates: true }} />);
    const sel = '[aria-label="Mise à jour disponible"]';
    expect(screen.getByRole("button", { name: /Mises à jour/ }).querySelector(sel)).toBeTruthy();
    expect(screen.getByRole("button", { name: /Général/ }).querySelector(sel)).toBeNull();
  });

  it("Sidebar : point sur Paramètres quand settingsDot", () => {
    const { rerender } = render(<Sidebar active="downloads" counts={{}} onNavigate={() => {}} settingsDot />);
    expect(screen.getByRole("button", { name: /Paramètres/ }).querySelector('[aria-label="Mise à jour disponible"]')).toBeTruthy();
    rerender(<Sidebar active="downloads" counts={{}} onNavigate={() => {}} settingsDot={false} />);
    expect(screen.getByRole("button", { name: /Paramètres/ }).querySelector('[aria-label="Mise à jour disponible"]')).toBeNull();
  });
});

describe("formatLastCheck", () => {
  const now = new Date(2026, 10, 11, 15, 0);
  const at = (y: number, mo: number, d: number, h: number, mi: number) => new Date(y, mo, d, h, mi).getTime() / 1000;
  it("aujourd'hui, hier, ancien", () => {
    expect(formatLastCheck(at(2026, 10, 11, 9, 5), now)).toBe("aujourd'hui à 09:05");
    expect(formatLastCheck(at(2026, 10, 10, 23, 59), now)).toBe("hier à 23:59");
    expect(formatLastCheck(at(2026, 9, 3, 12, 0), now)).toBe("03/10/2026");
  });
});
