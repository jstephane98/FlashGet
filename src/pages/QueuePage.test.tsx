import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, cleanup, waitFor } from "@testing-library/react";
import type { Download } from "../types";

vi.mock("../api");
import * as api from "../api";
import { QueuePage } from "./QueuePage";

const dt = { setData() {}, getData() { return ""; }, effectAllowed: "", dropEffect: "" };

function mk(id: string, p: Partial<Download> = {}): Download {
  return {
    id, url: "https://site.com/" + id, file_name: id + ".bin", dest_path: "C:/dl/" + id + ".bin",
    total_bytes: 1024 * 1024, downloaded_bytes: 0, status: "queued", supports_ranges: true,
    error: null, priority: "normal", category: "other", kind: "regular", ...p,
  } as Download;
}

const three = () => [
  mk("a", { queue_rank: 1 }),
  mk("b", { queue_rank: 2 }),
  mk("c", { queue_rank: 3, priority: "low" }),
];

type Props = Parameters<typeof QueuePage>[0];
function setup(downloads: Download[], over: Partial<Props> = {}) {
  const props: Props = { downloads, maxConcurrent: 3, schedule: null, paused: false, onChanged: vi.fn(), ...over };
  render(<QueuePage {...props} />);
  return props;
}
const rows = () => screen.getAllByTestId("queue-row");

beforeEach(() => {
  vi.mocked(api.reorderQueue).mockResolvedValue(undefined);
  vi.mocked(api.setPriority).mockResolvedValue(undefined);
  vi.mocked(api.startQueue).mockResolvedValue(undefined);
  vi.mocked(api.pauseQueue).mockResolvedValue(undefined);
  vi.mocked(api.removeFromList).mockResolvedValue(undefined);
});
afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.clearAllMocks(); });

describe("QueuePage", () => {
  it("numérote les lignes dans l'ordre de la file et affiche le résumé", () => {
    setup([mk("z", { status: "completed" }), ...three()]);
    const r = rows();
    expect(r).toHaveLength(3);
    expect(r[0].textContent).toContain("a.bin");
    expect(r[2].textContent).toContain("c.bin");
    expect(r[0].textContent).toContain("1");
    expect(screen.getByText(/3 en attente · 3,0 Mo · démarrent dès qu'une place se libère \(3 simultanés max\)/)).toBeTruthy();
  });

  it("affiche — si une taille manque", () => {
    setup([mk("a"), mk("b", { total_bytes: null })]);
    expect(screen.getByText(/2 en attente · — ·/)).toBeTruthy();
  });

  it("file vide et étiquette suspendue", () => {
    setup([], { paused: true });
    expect(screen.getByText("La file est vide.")).toBeTruthy();
    expect(screen.getByText("File suspendue")).toBeTruthy();
  });

  it("Tout démarrer appelle startQueue puis onChanged", async () => {
    const props = setup(three());
    fireEvent.click(screen.getByRole("button", { name: /Tout démarrer maintenant/ }));
    await waitFor(() => expect(props.onChanged).toHaveBeenCalled());
    expect(api.startQueue).toHaveBeenCalled();
  });

  it("Suspendre appelle pauseQueue", async () => {
    const props = setup(three());
    fireEvent.click(screen.getByRole("button", { name: /Suspendre la file/ }));
    await waitFor(() => expect(props.onChanged).toHaveBeenCalled());
    expect(api.pauseQueue).toHaveBeenCalled();
  });

  it("glisser la ligne 3 sur la ligne 1 réordonne et change la priorité", async () => {
    const props = setup(three());
    const r = rows();
    fireEvent.dragStart(r[2], { dataTransfer: dt });
    fireEvent.dragOver(r[0], { dataTransfer: dt });
    fireEvent.drop(r[0], { dataTransfer: dt });
    await waitFor(() => expect(api.reorderQueue).toHaveBeenCalledWith(["c", "a", "b"]));
    expect(api.setPriority).toHaveBeenCalledWith("c", "normal");
    expect(props.onChanged).toHaveBeenCalled();
  });

  it("Monter / Descendre", async () => {
    setup(three());
    fireEvent.click(screen.getAllByRole("button", { name: "Monter" })[1]);
    await waitFor(() => expect(api.reorderQueue).toHaveBeenCalledWith(["b", "a", "c"]));
    vi.mocked(api.reorderQueue).mockClear();
    fireEvent.click(screen.getAllByRole("button", { name: "Descendre" })[0]);
    await waitFor(() => expect(api.reorderQueue).toHaveBeenCalledWith(["b", "a", "c"]));
  });

  it("programmé à 02:00 pour un élément retenu quand le planificateur est actif", () => {
    const schedule = { enabled: true, start: "02:00", end: "06:00", days: [], limit_bps: null, at_end: "nothing" } as unknown as Props["schedule"];
    setup([mk("a", { status: "held" })], { schedule });
    expect(screen.getByText("programmé à 02:00")).toBeTruthy();
  });

  it("élément retenu sans planificateur", () => {
    setup([mk("a", { status: "held" })]);
    expect(screen.getByText("en attente de la file")).toBeTruthy();
  });

  it("Retirer demande confirmation", async () => {
    const props = setup(three());
    const confirm = vi.spyOn(window, "confirm").mockReturnValueOnce(false).mockReturnValueOnce(true);
    fireEvent.click(screen.getAllByRole("button", { name: "Retirer" })[0]);
    expect(api.removeFromList).not.toHaveBeenCalled();
    fireEvent.click(screen.getAllByRole("button", { name: "Retirer" })[0]);
    await waitFor(() => expect(api.removeFromList).toHaveBeenCalledWith("a"));
    expect(props.onChanged).toHaveBeenCalled();
    expect(confirm).toHaveBeenCalledWith("Retirer cet élément de la file ?");
  });

  it("change la priorité via le sélecteur", async () => {
    setup(three());
    fireEvent.change(screen.getAllByRole("combobox")[0], { target: { value: "high" } });
    await waitFor(() => expect(api.setPriority).toHaveBeenCalledWith("a", "high"));
  });

  it("affiche l'erreur de l'API sans lever", async () => {
    vi.mocked(api.startQueue).mockRejectedValue("boom");
    setup(three());
    fireEvent.click(screen.getByRole("button", { name: /Tout démarrer maintenant/ }));
    expect((await screen.findByRole("alert")).textContent).toContain("boom");
  });

  it("reorderQueue échoue : erreur affichée et onChanged appelé", async () => {
    vi.mocked(api.reorderQueue).mockRejectedValue("nope");
    const props = setup(three());
    fireEvent.click(screen.getAllByRole("button", { name: "Monter" })[1]);
    expect((await screen.findByRole("alert")).textContent).toContain("nope");
    expect(props.onChanged).toHaveBeenCalled();
  });
});
