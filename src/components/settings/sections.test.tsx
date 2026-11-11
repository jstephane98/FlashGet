import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor, act } from "@testing-library/react";
import type { Settings } from "../../types";

vi.mock("../../api");
import * as api from "../../api";
import { VideoSection } from "./VideoSection";
import { NotificationsSection } from "./NotificationsSection";
import { AboutSection } from "./AboutSection";

const S: Settings = {
  max_concurrent: 3, max_speed_bps: 0, connections: 8, retries: 5, auto_resume: true,
  conflict: "rename", close_to_tray: true, notify_complete: true, notify_error: true,
  video_max_height: 1080, audio_format: "mp3", audio_kbps: 320, sub_langs: ["fr"],
  ytdlp_auto_update: true, app_update_check: true,
};
const save = vi.fn();
let ytCb: () => void = () => {};

beforeEach(() => {
  vi.mocked(api.getYtdlpVersion).mockResolvedValue("2026.10.30");
  vi.mocked(api.appVersion).mockResolvedValue("0.1.0");
  vi.mocked(api.youtubeStatus).mockResolvedValue(false);
  vi.mocked(api.youtubeLogin).mockResolvedValue(undefined);
  vi.mocked(api.youtubeLogout).mockResolvedValue(undefined);
  vi.mocked(api.getCookiesFromBrowser).mockResolvedValue(null);
  vi.mocked(api.setCookiesFromBrowser).mockResolvedValue(undefined);
  vi.mocked(api.onYoutubeStatus).mockImplementation(async (cb) => { ytCb = cb; return () => {}; });
});
afterEach(() => { cleanup(); vi.clearAllMocks(); vi.unstubAllGlobals(); });

const props = { settings: S, save, rev: 0 };

describe("VideoSection", () => {
  it("affiche la version de yt-dlp", async () => {
    render(<VideoSection {...props} />);
    expect(await screen.findByText("2026.10.30")).toBeTruthy();
  });

  it("qualité 720p", () => {
    render(<VideoSection {...props} />);
    fireEvent.change(screen.getByLabelText("Qualité vidéo préférée"), { target: { value: "720" } });
    expect(save).toHaveBeenCalledWith({ video_max_height: 720 });
  });

  it("format M4A puis débit 256", () => {
    render(<VideoSection {...props} />);
    fireEvent.click(screen.getByRole("button", { name: "M4A" }));
    expect(save).toHaveBeenCalledWith({ audio_format: "m4a" });
    fireEvent.change(screen.getByLabelText("Débit audio"), { target: { value: "256" } });
    expect(save).toHaveBeenCalledWith({ audio_kbps: 256 });
  });

  it("Maximum (0) est sélectionné par défaut et enregistre 0", () => {
    const { rerender } = render(<VideoSection {...props} settings={{ ...S, audio_kbps: 0 }} />);
    const sel = screen.getByLabelText("Débit audio") as HTMLSelectElement;
    expect(sel.value).toBe("0");
    expect(sel.options[sel.selectedIndex].text).toBe("Maximum");
    expect(sel.options).toHaveLength(5);
    fireEvent.change(sel, { target: { value: "256" } });
    expect(save).toHaveBeenCalledWith({ audio_kbps: 256 });
    rerender(<VideoSection {...props} settings={{ ...S, audio_kbps: 256 }} />);
    fireEvent.change(screen.getByLabelText("Débit audio"), { target: { value: "0" } });
    expect(save).toHaveBeenCalledWith({ audio_kbps: 0 });
  });

  it("décocher la dernière langue n'enregistre rien", () => {
    render(<VideoSection {...props} />);
    fireEvent.click(screen.getByRole("checkbox", { name: "Français" }));
    expect(save).not.toHaveBeenCalled();
    expect((screen.getByRole("checkbox", { name: "Français" }) as HTMLInputElement).checked).toBe(true);
  });

  it("cocher Allemand ajoute de", () => {
    render(<VideoSection {...props} />);
    fireEvent.click(screen.getByRole("checkbox", { name: "Allemand" }));
    expect(save).toHaveBeenCalledWith({ sub_langs: ["fr", "de"] });
  });

  it("YouTube : connexion puis déconnexion", async () => {
    render(<VideoSection {...props} />);
    fireEvent.click(await screen.findByRole("button", { name: "Se connecter à YouTube" }));
    expect(api.youtubeLogin).toHaveBeenCalled();
    vi.mocked(api.youtubeStatus).mockResolvedValue(true);
    await act(async () => { ytCb(); });
    fireEvent.click(await screen.findByRole("button", { name: "Se déconnecter" }));
    expect(api.youtubeLogout).toHaveBeenCalled();
  });

  it("cookies Edge", async () => {
    render(<VideoSection {...props} />);
    fireEvent.change(await screen.findByLabelText("Cookies d'un navigateur"), { target: { value: "edge" } });
    expect(api.setCookiesFromBrowser).toHaveBeenCalledWith("edge");
  });

  it("cookies : Aucun enregistre null", async () => {
    vi.mocked(api.getCookiesFromBrowser).mockResolvedValue("edge");
    render(<VideoSection {...props} />);
    const sel = (await screen.findByLabelText("Cookies d'un navigateur")) as HTMLSelectElement;
    await waitFor(() => expect(sel.value).toBe("edge"));
    fireEvent.change(sel, { target: { value: "none" } });
    expect(api.setCookiesFromBrowser).toHaveBeenCalledWith(null);
  });

  it("se désabonne de onYoutubeStatus au démontage", async () => {
    const unsub = vi.fn();
    vi.mocked(api.onYoutubeStatus).mockResolvedValue(unsub);
    const { unmount } = render(<VideoSection {...props} />);
    await screen.findByText("2026.10.30");
    unmount();
    await waitFor(() => expect(unsub).toHaveBeenCalled());
  });
});

describe("NotificationsSection", () => {
  it("échec désactivé", () => {
    render(<NotificationsSection {...props} />);
    fireEvent.click(screen.getByRole("switch", { name: "En cas d'échec" }));
    expect(save).toHaveBeenCalledWith({ notify_error: false });
  });
});

describe("À propos", () => {
  it("À propos", async () => {
    render(<AboutSection />);
    expect(await screen.findByText("Version 0.1.0")).toBeTruthy();
    expect(screen.getByText(/yt-dlp, FFmpeg/)).toBeTruthy();
    expect(screen.queryByText(/aria2/)).toBeNull();
  });
});
