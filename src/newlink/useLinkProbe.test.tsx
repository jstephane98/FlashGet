import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import type { LinkProbe } from "../api";

vi.mock("../api", () => ({ probeLink: vi.fn() }));
import * as api from "../api";
import { useLinkProbe } from "./useLinkProbe";

const probeLink = vi.mocked(api.probeLink);
const file = (url: string): LinkProbe => ({
  type: "file", probe_id: "p-" + url, url, file_name: "a.zip", total_bytes: 1,
  supports_ranges: true, category: "Archives" as never, default_dir: "C:/d",
});
const deferred = <T,>() => {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((a) => { resolve = a; });
  return { promise, resolve };
};

beforeEach(() => { probeLink.mockReset(); vi.useFakeTimers(); });
afterEach(() => { vi.useRealTimers(); });

describe("useLinkProbe", () => {
  it("analyse puis prêt", async () => {
    probeLink.mockResolvedValue(file("https://a/x"));
    const { result } = renderHook(() => useLinkProbe());
    expect(result.current.state.status).toBe("idle");
    await act(async () => { result.current.analyze("https://a/x"); });
    expect(result.current.state.status).toBe("ready");
  });
  it("passe par probing", () => {
    probeLink.mockReturnValue(new Promise(() => {}));
    const { result } = renderHook(() => useLinkProbe());
    act(() => { result.current.analyze("https://a/x"); });
    expect(result.current.state).toEqual({ status: "probing", url: "https://a/x" });
  });
  it("ignore un résultat périmé", async () => {
    const A = deferred<LinkProbe>();
    const B = deferred<LinkProbe>();
    probeLink.mockReturnValueOnce(A.promise).mockReturnValueOnce(B.promise);
    const { result } = renderHook(() => useLinkProbe());
    act(() => { result.current.analyze("https://a/A"); });
    act(() => { result.current.analyze("https://a/B"); });
    await act(async () => { B.resolve(file("https://a/B")); });
    await act(async () => { A.resolve(file("https://a/A")); });
    const s = result.current.state;
    expect(s.status === "ready" && s.probe.url).toBe("https://a/B");
  });
  it("erreur puis nouvelle analyse", async () => {
    probeLink.mockRejectedValueOnce("Aucune vidéo trouvée sur cette page.");
    const { result } = renderHook(() => useLinkProbe());
    await act(async () => { result.current.analyze("https://a/x"); });
    expect(result.current.state).toEqual({ status: "error", url: "https://a/x", message: "Aucune vidéo trouvée sur cette page." });
    probeLink.mockReturnValueOnce(new Promise(() => {}));
    act(() => { result.current.analyze("https://a/x"); });
    expect(result.current.state.status).toBe("probing");
  });
  it("délai dépassé", () => {
    probeLink.mockReturnValue(new Promise(() => {}));
    const { result } = renderHook(() => useLinkProbe());
    act(() => { result.current.analyze("https://a/x"); });
    act(() => { vi.advanceTimersByTime(90_000); });
    expect(result.current.state).toEqual({ status: "error", url: "https://a/x", message: "L'analyse a pris trop de temps. Réessaie." });
  });
  it("URL invalide sans appel", () => {
    const { result } = renderHook(() => useLinkProbe());
    act(() => { result.current.analyze("ftp://x"); });
    expect(result.current.state).toMatchObject({ status: "error", message: "Colle un lien qui commence par http:// ou https://" });
    act(() => { result.current.analyze(""); });
    expect(result.current.state.status).toBe("error");
    expect(probeLink).not.toHaveBeenCalled();
  });
  it("load depuis l'extension remplace une analyse en cours", async () => {
    const A = deferred<LinkProbe>();
    probeLink.mockReturnValue(A.promise);
    const { result } = renderHook(() => useLinkProbe());
    act(() => { result.current.analyze("https://a/A"); });
    act(() => { result.current.load(file("https://a/E")); });
    await act(async () => { A.resolve(file("https://a/A")); });
    const s = result.current.state;
    expect(s.status === "ready" && s.probe.url).toBe("https://a/E");
    act(() => { vi.advanceTimersByTime(90_000); });
    expect(result.current.state.status).toBe("ready");
  });
  it("showProbing, fail et reset", () => {
    const { result } = renderHook(() => useLinkProbe());
    act(() => { result.current.showProbing("https://a/x"); });
    expect(result.current.state).toEqual({ status: "probing", url: "https://a/x" });
    act(() => { result.current.fail("https://a/x", "boum"); });
    expect(result.current.state).toEqual({ status: "error", url: "https://a/x", message: "boum" });
    act(() => { result.current.reset(); });
    expect(result.current.state.status).toBe("idle");
  });
  it("showProbing : délai dépassé si l'extension ne répond jamais", () => {
    const { result } = renderHook(() => useLinkProbe());
    act(() => { result.current.showProbing("https://a/x"); });
    act(() => { vi.advanceTimersByTime(89_999); });
    expect(result.current.state.status).toBe("probing");
    act(() => { vi.advanceTimersByTime(1); });
    expect(result.current.state).toEqual({ status: "error", url: "https://a/x", message: "L'analyse a pris trop de temps. Réessaie." });
  });
  it("showProbing : un résultat arrivé à temps annule le délai", () => {
    const { result } = renderHook(() => useLinkProbe());
    act(() => { result.current.showProbing("https://a/E"); });
    act(() => { vi.advanceTimersByTime(30_000); });
    act(() => { result.current.load(file("https://a/E")); });
    act(() => { vi.advanceTimersByTime(90_000); });
    expect(result.current.state.status).toBe("ready");
    // Une nouvelle capture remplace la précédente et repart de zéro.
    act(() => { result.current.showProbing("https://a/F"); });
    act(() => { vi.advanceTimersByTime(60_000); });
    act(() => { result.current.showProbing("https://a/G"); });
    act(() => { vi.advanceTimersByTime(60_000); });
    expect(result.current.state).toEqual({ status: "probing", url: "https://a/G" });
  });
});
