import { describe, it, expect, vi, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { AddBatchDialog } from "./AddBatchDialog";

afterEach(cleanup);

describe("AddBatchDialog", () => {
  it("ne rend rien quand fermé", () => {
    render(<AddBatchDialog open={false} onClose={vi.fn()} onAdd={vi.fn()} />);
    expect(screen.queryByLabelText("Liens à télécharger")).toBeNull();
  });

  it("transmet le texte brut multi-lignes à onAdd", () => {
    const onAdd = vi.fn();
    render(<AddBatchDialog open onClose={vi.fn()} onAdd={onAdd} />);
    const ta = screen.getByLabelText("Liens à télécharger");
    fireEvent.change(ta, { target: { value: "https://a.com/1.zip\nhttps://a.com/2.zip" } });
    fireEvent.click(screen.getByText("Ajouter"));
    expect(onAdd).toHaveBeenCalledWith("https://a.com/1.zip\nhttps://a.com/2.zip");
  });

  it("désactive Ajouter quand le texte est vide", () => {
    render(<AddBatchDialog open onClose={vi.fn()} onAdd={vi.fn()} />);
    const btn = screen.getByText("Ajouter") as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });
});
