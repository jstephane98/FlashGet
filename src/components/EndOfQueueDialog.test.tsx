import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { EndOfQueueDialog } from "./EndOfQueueDialog";

describe("EndOfQueueDialog", () => {
  beforeEach(() => { vi.useFakeTimers(); });
  afterEach(() => { vi.useRealTimers(); });

  it("affiche le titre et décompte de 60 à 59", () => {
    render(<EndOfQueueDialog action="quit" onCancel={() => {}} onRun={() => {}} />);
    expect(screen.getByText("Tous les téléchargements sont terminés")).toBeTruthy();
    expect(screen.getByText("Quitter FlashGet dans 60 s.")).toBeTruthy();
    act(() => { vi.advanceTimersByTime(1000); });
    expect(screen.getByText("Quitter FlashGet dans 59 s.")).toBeTruthy();
  });

  it("appelle onRun une seule fois à 60 s", () => {
    const onRun = vi.fn();
    render(<EndOfQueueDialog action="shutdown" onCancel={() => {}} onRun={onRun} />);
    act(() => { vi.advanceTimersByTime(60_000); });
    expect(onRun).toHaveBeenCalledTimes(1);
    act(() => { vi.advanceTimersByTime(10_000); });
    expect(onRun).toHaveBeenCalledTimes(1);
  });

  it("Annuler puis 60 s : onRun jamais appelé", () => {
    const onRun = vi.fn();
    const onCancel = vi.fn();
    const { unmount } = render(<EndOfQueueDialog action="quit" onCancel={onCancel} onRun={onRun} />);
    fireEvent.click(screen.getByText("Annuler"));
    expect(onCancel).toHaveBeenCalled();
    unmount();
    act(() => { vi.advanceTimersByTime(60_000); });
    expect(onRun).not.toHaveBeenCalled();
  });

  it("Maintenant appelle onRun", () => {
    const onRun = vi.fn();
    render(<EndOfQueueDialog action="sleep" onCancel={() => {}} onRun={onRun} />);
    fireEvent.click(screen.getByText("Maintenant"));
    expect(onRun).toHaveBeenCalledTimes(1);
  });

  it("Échap annule", () => {
    const onCancel = vi.fn();
    render(<EndOfQueueDialog action="quit" onCancel={onCancel} onRun={() => {}} />);
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});
