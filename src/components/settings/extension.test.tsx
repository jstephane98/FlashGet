import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor, act } from "@testing-library/react";
import { useState } from "react";
import type { BridgeStatus } from "../../types";

vi.mock("../../api");
import * as api from "../../api";
import { ExtensionSection } from "./ExtensionSection";
import { formatLastSeen } from "./extensionFormat";
import { useBridgePolling } from "../../bridge/useBridgePolling";
import { StatusBar } from "../shell/StatusBar";

const NOW = 1_800_000_000;
const st = (o: Partial<BridgeStatus> = {}): BridgeStatus => ({
  state: "connected", browser: "Chrome", version: "2.0.0", last_seen: NOW, clients: 1, port: 15151, ...o,
});
let sitesCb: (s: string[]) => void = () => {};
const writeText = vi.fn();

beforeEach(() => {
  vi.spyOn(Date, "now").mockReturnValue(NOW * 1000);
  writeText.mockResolvedValue(undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  vi.mocked(api.getBridgeInfo).mockResolvedValue({ listening: true, port: 15151, token: "tok-abc" });
  vi.mocked(api.regenerateBridgeToken).mockResolvedValue("tok-new");
  vi.mocked(api.getIgnoredSites).mockResolvedValue(["github.com"]);
  vi.mocked(api.setIgnoredSites).mockImplementation(async (s) => s);
  vi.mocked(api.openExtensionFolder).mockResolvedValue(undefined);
  vi.mocked(api.onIgnoredSitesChanged).mockImplementation(async (cb) => { sitesCb = cb; return () => {}; });
});
afterEach(() => { cleanup(); vi.clearAllMocks(); vi.unstubAllGlobals(); vi.restoreAllMocks(); });

describe("formatLastSeen", () => {
  it("à l'instant sous 60 s, sinon minutes", () => {
    expect(formatLastSeen(NOW - 59, NOW)).toBe("active à l'instant");
    expect(formatLastSeen(NOW - 60, NOW)).toBe("il y a 1 min");
    expect(formatLastSeen(NOW - 600, NOW)).toBe("il y a 10 min");
  });
});

describe("StatusBar extension", () => {
  const bar = (state: BridgeStatus["state"]) =>
    render(<StatusBar active={0} queued={0} maxSpeedBps={0} bridgeState={state} totalSpeedBps={0} />);
  const dot = (label: string) => screen.getByText(label).querySelector("span") as HTMLElement;

  it("connectée : point vert", () => {
    bar("connected");
    expect(dot("Extension : connectée").className).toContain("bg-ok");
  });
  it("en attente : point en contour", () => {
    bar("waiting");
    const c = dot("Extension : en attente").className;
    expect(c).toContain("border-ink-soft");
    expect(c).not.toContain("bg-ok");
  });
  it("inactive : point rouge", () => {
    bar("inactive");
    expect(dot("Extension : inactive").className).toContain("bg-err");
  });
  it("jeton refusé : point rouge", () => {
    bar("rejected");
    expect(dot("Extension : jeton refusé").className).toContain("bg-err");
  });
});

describe("ExtensionSection connexion", () => {
  it("connectée : navigateur, version en mono, activité", async () => {
    render(<ExtensionSection status={st()} />);
    expect(await screen.findByText("Connectée")).toBeTruthy();
    expect(screen.getByText("2.0.0").className).toContain("font-mono");
    expect(screen.getByText(/Chrome/)).toBeTruthy();
    expect(screen.getByText(/active à l'instant/)).toBeTruthy();
  });
  it("connectée depuis plusieurs minutes", async () => {
    render(<ExtensionSection status={st({ last_seen: NOW - 300 })} />);
    expect(await screen.findByText(/il y a 5 min/)).toBeTruthy();
  });
  it("en attente", async () => {
    render(<ExtensionSection status={st({ state: "waiting", browser: null, version: null, last_seen: null, clients: 0 })} />);
    expect(await screen.findByText("En attente")).toBeTruthy();
    expect(screen.getByText(/FlashGet attend l'extension sur le port/)).toBeTruthy();
    expect(screen.getByText("15151").className).toContain("font-mono");
  });
  it("jeton refusé", async () => {
    render(<ExtensionSection status={st({ state: "rejected" })} />);
    expect(await screen.findByText("Jeton refusé")).toBeTruthy();
    expect(screen.getByText(/mauvais jeton/)).toBeTruthy();
  });
  it("inactive : port occupé", async () => {
    render(<ExtensionSection status={st({ state: "inactive" })} />);
    expect(await screen.findByText("Inactive")).toBeTruthy();
    expect(screen.getByText(/déjà utilisé par un autre programme/)).toBeTruthy();
  });
});

describe("ExtensionSection mise en page", () => {
  it("la description de l'état est l'indication sous « État »", async () => {
    render(<ExtensionSection status={st()} />);
    const hint = (await screen.findByText(/Chrome/)).closest("div") as HTMLElement;
    expect(hint.previousElementSibling?.textContent).toBe("État");
    expect(hint.className).not.toContain("text-right");
  });
});

describe("ExtensionSection jeton", () => {
  it("copie le jeton", async () => {
    render(<ExtensionSection status={st()} />);
    await screen.findByText("tok-abc");
    fireEvent.click(screen.getByRole("button", { name: "Copier" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("tok-abc"));
    expect(await screen.findByRole("button", { name: "Copié" })).toBeTruthy();
  });
  it("régénérer demande confirmation", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<ExtensionSection status={st()} />);
    const btn = await screen.findByRole("button", { name: "Régénérer" });
    fireEvent.click(btn);
    expect(api.regenerateBridgeToken).not.toHaveBeenCalled();
    confirm.mockReturnValue(true);
    fireEvent.click(btn);
    expect(await screen.findByText("tok-new")).toBeTruthy();
  });
  it("erreur du presse-papiers affichée", async () => {
    writeText.mockRejectedValue("refusé");
    render(<ExtensionSection status={st()} />);
    await screen.findByText("tok-abc");
    fireEvent.click(screen.getByRole("button", { name: "Copier" }));
    const msg = (await screen.findByRole("alert")).textContent;
    expect(msg).toContain("presse-papiers");
    expect(msg).not.toContain("refusé");
  });
});

describe("ExtensionSection installation", () => {
  it("copie l'adresse des extensions", async () => {
    render(<ExtensionSection status={st()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Copier l'adresse" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("chrome://extensions"));
    expect(await screen.findByRole("button", { name: "Copié" })).toBeTruthy();
  });
  it("copie le jeton depuis l'étape 3", async () => {
    render(<ExtensionSection status={st()} />);
    await screen.findByText("tok-abc");
    fireEvent.click(screen.getByRole("button", { name: "Copier le jeton" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("tok-abc"));
    expect(await screen.findByRole("button", { name: "Copié" })).toBeTruthy();
  });
  it("ouvre le dossier, et affiche l'erreur", async () => {
    render(<ExtensionSection status={st()} />);
    fireEvent.click(await screen.findByRole("button", { name: "Ouvrir le dossier" }));
    await waitFor(() => expect(api.openExtensionFolder).toHaveBeenCalled());
    vi.mocked(api.openExtensionFolder).mockRejectedValue("Dossier de l'extension introuvable");
    fireEvent.click(screen.getByRole("button", { name: "Ouvrir le dossier" }));
    expect((await screen.findByRole("alert")).textContent).toContain("Dossier de l'extension introuvable");
  });
});

describe("ExtensionSection sites ignorés", () => {
  const input = () => screen.getByPlaceholderText("Ajouter un site, ex. exemple.com") as HTMLInputElement;

  it("liste en mono et ajout", async () => {
    render(<ExtensionSection status={st()} />);
    expect((await screen.findByText("github.com")).className).toContain("font-mono");
    fireEvent.change(input(), { target: { value: "exemple.com" } });
    fireEvent.click(screen.getByRole("button", { name: "Ajouter" }));
    await waitFor(() => expect(api.setIgnoredSites).toHaveBeenCalledWith(["github.com", "exemple.com"]));
    expect(await screen.findByText("exemple.com")).toBeTruthy();
    expect(input().value).toBe("");
  });
  it("Entrée ajoute", async () => {
    render(<ExtensionSection status={st()} />);
    await screen.findByText("github.com");
    fireEvent.change(input(), { target: { value: "a.fr" } });
    fireEvent.keyDown(input(), { key: "Enter" });
    await waitFor(() => expect(api.setIgnoredSites).toHaveBeenCalledWith(["github.com", "a.fr"]));
  });
  it("erreur Rust : affichée, saisie et liste conservées", async () => {
    vi.mocked(api.setIgnoredSites).mockRejectedValue("« a b » n'est pas un site valide");
    render(<ExtensionSection status={st()} />);
    await screen.findByText("github.com");
    fireEvent.change(input(), { target: { value: "a b" } });
    fireEvent.click(screen.getByRole("button", { name: "Ajouter" }));
    expect((await screen.findByRole("alert")).textContent).toContain("n'est pas un site valide");
    expect(input().value).toBe("a b");
    expect(screen.getByText("github.com")).toBeTruthy();
  });
  it("retire un site", async () => {
    render(<ExtensionSection status={st()} />);
    await screen.findByText("github.com");
    fireEvent.click(screen.getByRole("button", { name: "Retirer github.com" }));
    await waitFor(() => expect(api.setIgnoredSites).toHaveBeenCalledWith([]));
    await waitFor(() => expect(screen.queryByText("github.com")).toBeNull());
  });
  it("éditions sérialisées : boutons désactivés pendant l'enregistrement", async () => {
    let release: (s: string[]) => void = () => {};
    vi.mocked(api.setIgnoredSites).mockImplementation(() => new Promise<string[]>((r) => { release = r; }));
    render(<ExtensionSection status={st()} />);
    await screen.findByText("github.com");
    fireEvent.click(screen.getByRole("button", { name: "Retirer github.com" }));
    await waitFor(() => expect((screen.getByRole("button", { name: "Ajouter" }) as HTMLButtonElement).disabled).toBe(true));
    expect((screen.getByRole("button", { name: "Retirer github.com" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(input(), { target: { value: "x.fr" } });
    fireEvent.keyDown(input(), { key: "Enter" });
    expect(api.setIgnoredSites).toHaveBeenCalledTimes(1);
    await act(async () => release([]));
    await waitFor(() => expect((screen.getByRole("button", { name: "Ajouter" }) as HTMLButtonElement).disabled).toBe(false));
  });
  it("l'événement ignored-sites-changed met la liste à jour", async () => {
    render(<ExtensionSection status={st()} />);
    await screen.findByText("github.com");
    act(() => sitesCb(["drive.google.com"]));
    expect(await screen.findByText("drive.google.com")).toBeTruthy();
    expect(screen.queryByText("github.com")).toBeNull();
  });
});

describe("useBridgePolling", () => {
  function Harness({ initial, open }: { initial: BridgeStatus; open: boolean }) {
    const [status, setStatus] = useState<BridgeStatus>(initial);
    useBridgePolling(status.state, open, setStatus);
    return <ExtensionSection status={status} />;
  }
  it("relit l'état toutes les 30 s dans les Paramètres : « active à l'instant »", async () => {
    vi.mocked(api.getBridgeStatus).mockResolvedValue(st({ last_seen: NOW - 300 }));
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    try {
      render(<Harness initial={st({ last_seen: NOW - 300 })} open />);
      expect(screen.getByText(/il y a 5 min/)).toBeTruthy();
      vi.mocked(api.getBridgeStatus).mockResolvedValue(st({ last_seen: NOW }));
      await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
      expect(screen.getByText(/active à l'instant/)).toBeTruthy();
    } finally {
      vi.useRealTimers();
    }
  });
  it("ne relit pas hors Paramètres tant que connectée", async () => {
    vi.mocked(api.getBridgeStatus).mockResolvedValue(st());
    vi.useFakeTimers({ toFake: ["setInterval", "clearInterval"] });
    try {
      render(<Harness initial={st()} open={false} />);
      await act(async () => { await vi.advanceTimersByTimeAsync(90_000); });
      expect(api.getBridgeStatus).not.toHaveBeenCalled();
    } finally {
      vi.useRealTimers();
    }
  });
});
