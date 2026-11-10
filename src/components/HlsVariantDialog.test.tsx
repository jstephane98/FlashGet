import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { HlsVariantDialog } from "./HlsVariantDialog";
import type { MediaVariantsEvent } from "../api";

afterEach(cleanup);

function makeEvent(over: Partial<MediaVariantsEvent> = {}): MediaVariantsEvent {
  return {
    probe_id: "p1",
    file_name: "Ma vidéo - YouTube",
    variants: [
      { index: 0, resolution: "1920x1080", bandwidth: 5_000_000, est_size: null },
      { index: 1, resolution: "1280x720", bandwidth: 2_000_000, est_size: null },
    ],
    ...over,
  };
}

describe("HlsVariantDialog", () => {
  it("pré-remplit le champ nom avec le titre", () => {
    render(<HlsVariantDialog event={makeEvent()} onClose={vi.fn()} onConfirm={vi.fn()} />);
    const input = screen.getByLabelText("Nom du fichier") as HTMLInputElement;
    expect(input.value).toBe("Ma vidéo - YouTube");
  });

  it("transmet le nom édité à onConfirm", () => {
    const onConfirm = vi.fn();
    render(<HlsVariantDialog event={makeEvent()} onClose={vi.fn()} onConfirm={onConfirm} />);
    const input = screen.getByLabelText("Nom du fichier");
    fireEvent.change(input, { target: { value: "Mon clip" } });
    fireEvent.click(screen.getByText("Télécharger"));
    expect(onConfirm).toHaveBeenCalledWith("p1", 0, "Mon clip", undefined);
  });
});
