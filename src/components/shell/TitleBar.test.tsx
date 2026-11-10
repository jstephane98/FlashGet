import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";

const win = { minimize: vi.fn(), toggleMaximize: vi.fn(), close: vi.fn() };
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => win }));

import { TitleBar } from "./TitleBar";

describe("TitleBar", () => {
  it("affiche le nom de l'application", () => {
    render(<TitleBar />);
    expect(screen.getByText("FlashGet")).toBeTruthy();
  });

  it("branche les trois boutons sur la fenêtre", () => {
    render(<TitleBar />);
    fireEvent.click(screen.getByLabelText("Réduire"));
    expect(win.minimize).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByLabelText("Agrandir"));
    expect(win.toggleMaximize).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByLabelText("Fermer"));
    expect(win.close).toHaveBeenCalledTimes(1);
  });
});
