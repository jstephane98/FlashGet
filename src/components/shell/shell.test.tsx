import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { Sidebar } from "./Sidebar";
import { StatusBar } from "./StatusBar";

describe("Sidebar", () => {
  it("le menu suit l'ordre du spec", () => {
    render(<Sidebar active="downloads" counts={{}} onNavigate={() => {}} />);
    const labels = screen.getAllByRole("button").map((b) => b.textContent);
    expect(labels).toEqual(["Nouveau", "Téléchargements", "Terminés", "File d'attente", "Planificateur", "Paramètres"]);
  });

  it("cliquer une entrée appelle onNavigate", () => {
    const onNavigate = vi.fn();
    render(<Sidebar active="downloads" counts={{}} onNavigate={onNavigate} />);
    fireEvent.click(screen.getByText("Terminés"));
    expect(onNavigate).toHaveBeenCalledWith("completed");
  });

  it("affiche les compteurs", () => {
    render(<Sidebar active="downloads" counts={{ downloads: 6, completed: 128 }} onNavigate={() => {}} />);
    expect(screen.getByText("6")).toBeTruthy();
    expect(screen.getByText("128")).toBeTruthy();
  });
});

describe("StatusBar", () => {
  it("barre d'état", () => {
    render(<StatusBar active={3} queued={1} maxSpeedBps={0} bridgeState="connected" totalSpeedBps={14260633} />);
    expect(screen.getByText("3 actifs · 1 en file")).toBeTruthy();
    expect(screen.getByText("Limite : aucune")).toBeTruthy();
    expect(screen.getByText("Extension : connectée")).toBeTruthy();
    expect(screen.getByText("↓ 13,6 Mo/s")).toBeTruthy();
  });
});
