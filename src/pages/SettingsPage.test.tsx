import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor, act } from "@testing-library/react";
import type { Settings } from "../types";

vi.mock("../api");
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
const setPref = vi.fn();
vi.mock("../theme/ThemeProvider", () => ({ useTheme: () => ({ pref: "dark", theme: "dark", setPref }) }));
import * as api from "../api";
import { open } from "@tauri-apps/plugin-dialog";
import { SettingsPage } from "./SettingsPage";

const S: Settings = {
  max_concurrent: 3, max_speed_bps: 0, connections: 8, retries: 5, auto_resume: true,
  conflict: "rename", close_to_tray: true, notify_complete: true, notify_error: true,
  video_max_height: 1080, audio_format: "mp3", audio_kbps: 320, sub_langs: ["fr"],
  ytdlp_auto_update: true, app_update_check: true,
};
const getSettings = vi.mocked(api.getSettings);
const saveSettings = vi.mocked(api.saveSettings);
const onSaved = vi.fn();

beforeEach(() => {
  getSettings.mockResolvedValue({ ...S });
  saveSettings.mockImplementation(async (s) => s);
  vi.mocked(api.getAutostart).mockResolvedValue(false);
  vi.mocked(api.setAutostart).mockResolvedValue(undefined);
  vi.mocked(api.getEndAction).mockResolvedValue("none");
  vi.mocked(api.setEndAction).mockResolvedValue(undefined);
  vi.mocked(api.getCategoryDirs).mockResolvedValue({
    video: null, document: null, program: null, archive: null, music: null, other: "D:/Autres",
  });
  vi.mocked(api.setCategoryDir).mockResolvedValue(undefined);
});
afterEach(() => { cleanup(); vi.clearAllMocks(); });

async function ready() {
  render(<SettingsPage onSaved={onSaved} updates={{ app: null, ytdlp: null }} onUpdatesChanged={() => {}} recording={false} />);
  await screen.findByRole("switch", { name: /zone de notification/ });
}
const goTo = (name: string) => fireEvent.click(screen.getByRole("button", { name }));

describe("SettingsPage", () => {
  it("liste 8 sections et affiche Général par défaut", async () => {
    await ready();
    expect(screen.getByRole("navigation").querySelectorAll("button")).toHaveLength(8);
    expect(screen.getByRole("heading", { name: "Général" })).toBeTruthy();
  });

  it("change le thème", async () => {
    await ready();
    fireEvent.click(screen.getByRole("button", { name: "Clair" }));
    expect(setPref).toHaveBeenCalledWith("light");
  });

  it("Fermer = réduire enregistre close_to_tray: false", async () => {
    await ready();
    fireEvent.click(screen.getByRole("switch", { name: /zone de notification/ }));
    await waitFor(() => expect(saveSettings).toHaveBeenCalledWith({ ...S, close_to_tray: false }));
    expect(onSaved).toHaveBeenCalled();
  });

  it("curseur de connexions à 16", async () => {
    await ready();
    goTo("Téléchargements");
    const slider = await screen.findByRole("slider", { name: "Connexions par fichier" });
    fireEvent.change(slider, { target: { value: "12" } });
    fireEvent.change(slider, { target: { value: "16" } });
    expect(saveSettings).not.toHaveBeenCalled();
    fireEvent.pointerUp(slider);
    fireEvent.blur(slider);
    await waitFor(() => expect(saveSettings).toHaveBeenCalledWith({ ...S, connections: 16 }));
    expect(saveSettings).toHaveBeenCalledTimes(1);
  });

  it("limite 2,5 enregistrée à la sortie du champ", async () => {
    getSettings.mockResolvedValue({ ...S, max_speed_bps: 1048576 });
    await ready();
    goTo("Téléchargements");
    const f = await screen.findByLabelText("Mo/s");
    fireEvent.change(f, { target: { value: "2,5" } });
    expect(saveSettings).not.toHaveBeenCalled();
    fireEvent.blur(f);
    await waitFor(() => expect(saveSettings).toHaveBeenCalledWith({ ...S, max_speed_bps: 2621440 }));
  });

  it("limite coupée puis réactivée : la dernière valeur est rétablie", async () => {
    getSettings.mockResolvedValue({ ...S, max_speed_bps: 2097152 });
    await ready();
    goTo("Téléchargements");
    const sw = await screen.findByRole("switch", { name: "Limiter la vitesse" });
    fireEvent.click(sw);
    await waitFor(() => expect(saveSettings).toHaveBeenLastCalledWith({ ...S, max_speed_bps: 0 }));
    await waitFor(() => expect(screen.getByRole("switch", { name: "Limiter la vitesse" }).getAttribute("aria-checked")).toBe("false"));
    fireEvent.click(screen.getByRole("switch", { name: "Limiter la vitesse" }));
    await waitFor(() => expect(saveSettings).toHaveBeenLastCalledWith({ ...S, max_speed_bps: 2097152 }));
  });

  it("succès dépassé puis refus du dernier : retour à la valeur du succès", async () => {
    let resolveA!: (s: Settings) => void;
    let rejectB!: (e: string) => void;
    saveSettings
      .mockImplementationOnce(() => new Promise<Settings>((r) => { resolveA = r; }))
      .mockImplementationOnce(() => new Promise<Settings>((_, j) => { rejectB = j; }));
    await ready();
    const sw = () => screen.getByRole("switch", { name: /zone de notification/ });
    fireEvent.click(sw()); // A : close_to_tray false
    fireEvent.click(sw()); // B : close_to_tray true
    await act(async () => { resolveA({ ...S, close_to_tray: false }); });
    await act(async () => { rejectB("Refusé"); });
    expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Refusé");
    await waitFor(() => expect(sw().getAttribute("aria-checked")).toBe("false"));
  });

  it("refus : message affiché et valeur restaurée", async () => {
    await ready();
    goTo("Téléchargements");
    saveSettings.mockRejectedValueOnce("Valeur refusée");
    const f = (await screen.findByLabelText("Téléchargements simultanés")) as HTMLInputElement;
    fireEvent.change(f, { target: { value: "7" } });
    fireEvent.blur(f);
    expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Valeur refusée");
    await waitFor(() => expect((screen.getByLabelText("Téléchargements simultanés") as HTMLInputElement).value).toBe("3"));
  });

  it("Remplacer affiche la note Corbeille", async () => {
    await ready();
    goTo("Téléchargements");
    expect(screen.queryByText(/Corbeille/)).toBeNull();
    fireEvent.change(await screen.findByLabelText("Fichier déjà existant"), { target: { value: "overwrite" } });
    expect(await screen.findByText(/Corbeille/)).toBeTruthy();
    await waitFor(() => expect(saveSettings).toHaveBeenCalledWith({ ...S, conflict: "overwrite" }));
  });

  it("action de fin", async () => {
    await ready();
    goTo("Téléchargements");
    fireEvent.change(await screen.findByLabelText("Quand la file est terminée"), { target: { value: "sleep" } });
    await waitFor(() => expect(api.setEndAction).toHaveBeenCalledWith("sleep"));
  });

  it("dossiers : Parcourir et Par défaut", async () => {
    vi.mocked(open).mockResolvedValue("D:/V");
    await ready();
    goTo("Dossiers");
    await screen.findByText("D:/Autres");
    fireEvent.click(screen.getAllByRole("button", { name: "Parcourir…" })[0]);
    await waitFor(() => expect(api.setCategoryDir).toHaveBeenCalledWith("video", "D:/V"));
    fireEvent.click(screen.getAllByRole("button", { name: "Par défaut" })[5]);
    await waitFor(() => expect(api.setCategoryDir).toHaveBeenCalledWith("other", null));
  });

  it("échec de chargement puis Réessayer", async () => {
    getSettings.mockRejectedValueOnce("x");
    render(<SettingsPage onSaved={onSaved} updates={{ app: null, ytdlp: null }} onUpdatesChanged={() => {}} recording={false} />);
    expect(await screen.findByText("Impossible de charger les paramètres.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Réessayer" }));
    expect(await screen.findByRole("switch", { name: /zone de notification/ })).toBeTruthy();
  });
});
