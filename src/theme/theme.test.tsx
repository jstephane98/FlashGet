import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, act, waitFor } from "@testing-library/react";
import * as api from "../api";
import { resolveTheme } from "./theme";
import { ThemeProvider, useTheme } from "./ThemeProvider";

vi.mock("../api", () => ({
  getUiPref: vi.fn(),
  setUiPref: vi.fn(),
}));

let systemDark = false;
let listeners: Array<(e: { matches: boolean }) => void> = [];

function stubMatchMedia() {
  window.matchMedia = ((query: string) => ({
    get matches() { return systemDark; },
    media: query,
    addEventListener: (_: string, cb: (e: { matches: boolean }) => void) => { listeners.push(cb); },
    removeEventListener: (_: string, cb: (e: { matches: boolean }) => void) => { listeners = listeners.filter((l) => l !== cb); },
  })) as unknown as typeof window.matchMedia;
}

function Probe() {
  const { pref, theme, setPref } = useTheme();
  return (
    <div>
      <span data-testid="state">{pref}/{theme}</span>
      <button onClick={() => setPref("light")}>light</button>
      <button onClick={() => setPref("system")}>system</button>
    </div>
  );
}

beforeEach(() => {
  systemDark = false;
  listeners = [];
  stubMatchMedia();
  localStorage.clear();
  delete document.documentElement.dataset.theme;
  vi.mocked(api.getUiPref).mockResolvedValue(null);
  vi.mocked(api.setUiPref).mockResolvedValue(undefined);
});
afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("thème", () => {
  it("resolveTheme", () => {
    expect(resolveTheme("dark", false)).toBe("dark");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });

  it("sombre par défaut", async () => {
    render(<ThemeProvider><span /></ThemeProvider>);
    expect(document.documentElement.dataset.theme).toBe("dark");
    await waitFor(() => expect(api.getUiPref).toHaveBeenCalled());
  });

  it("setPref('light') applique et persiste", async () => {
    render(<ThemeProvider><Probe /></ThemeProvider>);
    await waitFor(() => expect(api.getUiPref).toHaveBeenCalledWith("ui.theme"));
    fireEvent.click(screen.getByText("light"));
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(document.documentElement.style.colorScheme).toBe("light");
    expect(api.setUiPref).toHaveBeenCalledWith("ui.theme", "light");
    expect(localStorage.getItem("flashget.theme")).toBe("light");
  });

  it("suit le changement du système en mode system", async () => {
    render(<ThemeProvider><Probe /></ThemeProvider>);
    await waitFor(() => expect(api.getUiPref).toHaveBeenCalled());
    fireEvent.click(screen.getByText("system"));
    expect(document.documentElement.dataset.theme).toBe("light");
    systemDark = true;
    act(() => { listeners.forEach((l) => l({ matches: true })); });
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("la valeur persistée côté Rust l'emporte au démarrage", async () => {
    vi.mocked(api.getUiPref).mockResolvedValue("light");
    render(<ThemeProvider><Probe /></ThemeProvider>);
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe("light"));
    expect(screen.getByTestId("state").textContent).toBe("light/light");
    expect(localStorage.getItem("flashget.theme")).toBe("light");
  });
});
