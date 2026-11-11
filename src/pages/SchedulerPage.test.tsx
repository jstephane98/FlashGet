import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/react";
import type { Schedule } from "../api";

vi.mock("../api");
import * as api from "../api";
import { SchedulerPage } from "./SchedulerPage";

const getSchedule = vi.mocked(api.getSchedule);
const setSchedule = vi.mocked(api.setSchedule);
const getEndAction = vi.mocked(api.getEndAction);
const setEndAction = vi.mocked(api.setEndAction);

const SCH: Schedule = {
  enabled: false, start: "02:00", end: "07:00",
  days: [true, true, true, true, true, false, false], limit_bps: null, at_end: "pause",
};

beforeEach(() => {
  getSchedule.mockResolvedValue({ ...SCH });
  getEndAction.mockResolvedValue("none");
  setSchedule.mockResolvedValue(undefined);
  setEndAction.mockResolvedValue(undefined);
});
afterEach(() => { cleanup(); vi.clearAllMocks(); vi.useRealTimers(); });

async function ready() {
  render(<SchedulerPage heldCount={3} />);
  await screen.findByDisplayValue("02:00");
}

describe("SchedulerPage", () => {
  it("affiche les valeurs chargées et la frise", async () => {
    await ready();
    expect(screen.getByDisplayValue("07:00")).toBeTruthy();
    expect(screen.getByText("Planification désactivée")).toBeTruthy();
    expect(screen.getByRole("button", { name: "lundi" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("button", { name: "samedi" }).getAttribute("aria-pressed")).toBe("false");
    expect(screen.getByTestId("timeline").querySelectorAll("[data-seg]").length).toBe(1);
    expect(screen.getByText("12h")).toBeTruthy();
  });

  it("activer appelle setSchedule enabled", async () => {
    await ready();
    fireEvent.click(screen.getByRole("switch", { name: "Activé" }));
    await waitFor(() => expect(setSchedule).toHaveBeenCalledWith(expect.objectContaining({ enabled: true })));
  });

  it("coche samedi", async () => {
    await ready();
    fireEvent.click(screen.getByRole("button", { name: "samedi" }));
    await waitFor(() =>
      expect(setSchedule).toHaveBeenCalledWith(
        expect.objectContaining({ days: [true, true, true, true, true, true, false] }),
      ),
    );
  });

  it("10 Mo/s donne 10485760 octets/s", async () => {
    await ready();
    fireEvent.click(screen.getByRole("switch", { name: "Limiter la vitesse" }));
    fireEvent.change(screen.getByLabelText("Mo/s"), { target: { value: "10" } });
    fireEvent.blur(screen.getByLabelText("Mo/s"));
    await waitFor(() =>
      expect(setSchedule).toHaveBeenLastCalledWith(expect.objectContaining({ limit_bps: 10485760 })),
    );
    fireEvent.click(screen.getByRole("switch", { name: "Limiter la vitesse" }));
    await waitFor(() => expect(setSchedule).toHaveBeenLastCalledWith(expect.objectContaining({ limit_bps: null })));
  });

  it("le sélecteur d'action appelle setEndAction", async () => {
    await ready();
    fireEvent.change(screen.getByLabelText("Action"), { target: { value: "sleep" } });
    await waitFor(() => expect(setEndAction).toHaveBeenCalledWith("sleep"));
  });

  it("refus : affiche l'erreur et revient à l'heure enregistrée", async () => {
    setSchedule.mockRejectedValueOnce("Le début et la fin doivent différer");
    await ready();
    const end = screen.getByDisplayValue("07:00");
    fireEvent.change(end, { target: { value: "02:00" } });
    fireEvent.blur(end);
    expect((await screen.findByRole("alert")).textContent).toContain("différer");
    await waitFor(() => expect((end as HTMLInputElement).value).toBe("07:00"));
    expect(screen.getByRole("alert").textContent).toContain("différer");
  });

  it("heures : une seule sauvegarde au blur avec la valeur finale", async () => {
    await ready();
    const start = screen.getByLabelText("Démarrer la file à");
    fireEvent.change(start, { target: { value: "03:00" } });
    fireEvent.change(start, { target: { value: "04:30" } });
    expect(setSchedule).not.toHaveBeenCalled();
    fireEvent.blur(start);
    expect(setSchedule).toHaveBeenCalledTimes(1);
    expect(setSchedule).toHaveBeenCalledWith(expect.objectContaining({ start: "04:30", end: "07:00" }));
    // Entrée enregistre aussi ; le blur qui suit ne renvoie rien.
    const end = screen.getByLabelText("jusqu'à");
    fireEvent.change(end, { target: { value: "08:00" } });
    fireEvent.keyDown(end, { key: "Enter" });
    fireEvent.blur(end);
    expect(setSchedule).toHaveBeenCalledTimes(2);
    expect(setSchedule).toHaveBeenLastCalledWith(expect.objectContaining({ start: "04:30", end: "08:00" }));
  });

  it("refus : décocher le dernier jour restaure les jours enregistrés", async () => {
    getSchedule.mockResolvedValue({ ...SCH, enabled: true, days: [true, false, false, false, false, false, false] });
    setSchedule.mockRejectedValueOnce("au moins un jour doit être coché");
    await ready();
    const monday = screen.getByRole("button", { name: "lundi" });
    fireEvent.click(monday);
    expect((await screen.findByRole("alert")).textContent).toContain("au moins un jour");
    await waitFor(() => expect(monday.getAttribute("aria-pressed")).toBe("true"));
  });

  it("plage en cours : indique la fin de la plage", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 10, 9, 3, 0)); // lundi 03:00, dans 02:00 → 07:00
    getSchedule.mockResolvedValue({ ...SCH, enabled: true });
    render(<SchedulerPage heldCount={0} pendingCount={4} />);
    await screen.findByDisplayValue("02:00");
    expect(screen.getByText("Plage en cours jusqu'à 07:00 · 4 téléchargements concernés")).toBeTruthy();
  });

  it("limite : une seule sauvegarde au blur, virgule acceptée, champ vide restauré", async () => {
    await ready();
    fireEvent.click(screen.getByRole("switch", { name: "Limiter la vitesse" }));
    await waitFor(() => expect(setSchedule).toHaveBeenCalledTimes(1));
    setSchedule.mockClear();
    const input = screen.getByLabelText("Mo/s") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "2" } });
    fireEvent.change(input, { target: { value: "25" } });
    expect(setSchedule).not.toHaveBeenCalled();
    fireEvent.blur(input);
    expect(setSchedule).toHaveBeenCalledTimes(1);
    expect(setSchedule).toHaveBeenCalledWith(expect.objectContaining({ limit_bps: 26214400 }));
    fireEvent.change(input, { target: { value: "2,5" } });
    fireEvent.blur(input);
    expect(setSchedule).toHaveBeenLastCalledWith(expect.objectContaining({ limit_bps: 2621440 }));
    setSchedule.mockClear();
    fireEvent.change(input, { target: { value: "" } });
    fireEvent.blur(input);
    expect(setSchedule).not.toHaveBeenCalled();
    expect(input.value).toBe("2.5");
  });

  it("échec de chargement : erreur puis Réessayer", async () => {
    getSchedule.mockRejectedValueOnce("boom");
    render(<SchedulerPage heldCount={3} />);
    expect((await screen.findByRole("alert")).textContent).toContain("Impossible de charger");
    fireEvent.click(screen.getByRole("button", { name: "Réessayer" }));
    await screen.findByDisplayValue("02:00");
    expect(getSchedule).toHaveBeenCalledTimes(2);
  });

  it("l'erreur d'une autre carte s'affiche en tête de page, une seule fois", async () => {
    setSchedule.mockRejectedValueOnce("Plage invalide");
    await ready();
    fireEvent.click(screen.getByRole("switch", { name: "Limiter la vitesse" }));
    await waitFor(() => expect(screen.getAllByRole("alert").length).toBe(1));
    expect(screen.getByRole("alert").textContent).toContain("Plage invalide");
  });

  it("affiche le prochain démarrage avec le nombre d'éléments", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(new Date(2026, 10, 9, 10, 0)); // lundi 10:00, hors plage
    getSchedule.mockResolvedValue({ ...SCH, enabled: true });
    await ready();
    expect(screen.getByText(/Prochain démarrage : .* · 3 téléchargements concernés/)).toBeTruthy();
  });
});
