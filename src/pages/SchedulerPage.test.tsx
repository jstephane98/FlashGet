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

  it("affiche l'erreur de validation et garde la saisie", async () => {
    setSchedule.mockRejectedValueOnce("Le début et la fin doivent différer");
    await ready();
    fireEvent.change(screen.getByDisplayValue("07:00"), { target: { value: "02:00" } });
    expect((await screen.findByRole("alert")).textContent).toContain("différer");
    expect(screen.getAllByDisplayValue("02:00").length).toBe(2);
  });

  it("affiche le prochain démarrage avec le nombre d'éléments", async () => {
    getSchedule.mockResolvedValue({ ...SCH, enabled: true });
    await ready();
    expect(screen.getByText(/Prochain démarrage : .* · 3 téléchargements concernés/)).toBeTruthy();
  });
});
