# Nom de fichier vidéo dérivé du titre — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **GARDE-FOU GIT (obligatoire) :** chaque subagent DOIT exécuter `git checkout phase-4b3-media-filename-title` avant tout commit. Un subagent a déjà commité sur la mauvaise branche lors d'une phase précédente.

**Goal:** Remplacer le nom de fichier `video.mp4` par le titre de la page (éditable) lors de la capture d'une vidéo HLS/DASH.

**Architecture:** L'extension envoie `document.title` (via `tab.title`) dans le champ `filename` existant de la capture média (Approche A, aucun changement de protocole). Le cœur Rust assainit ce titre en radical de fichier sûr (`naming::sanitize_title`). La boîte de dialogue des variantes affiche ce nom dans un champ éditable, propagé jusqu'à `add_media_download` qui ajoute `.mp4`.

**Tech Stack:** Rust (crate `download-core`, crate Tauri), React/TypeScript + Vite + Vitest, extension Chromium MV3 (JS pur).

**Spec :** `docs/superpowers/specs/2026-06-05-media-filename-from-title-design.md`

---

## File Structure

- `src-tauri/crates/download-core/src/naming.rs` — **modifier** : nouvelle fonction `sanitize_title` + tests.
- `src-tauri/src/commands.rs` — **modifier** : `probe_hls_inner`/`probe_dash_inner` (sanitize au probe) + `add_media_download` (param `file_name`).
- `src/api.ts` — **modifier** : signature `addMediaDownload`.
- `src/components/HlsVariantDialog.tsx` — **modifier** : champ nom éditable.
- `src/components/HlsVariantDialog.test.tsx` — **créer** : tests Vitest.
- `src/App.tsx` — **modifier** : `handleHlsConfirm` propage le nom.
- `extension/background.js` — **modifier** : `sendMedia` transporte `tab.title`.

---

## Task 1: `sanitize_title` dans le cœur (TDD)

**Files:**
- Modify/Test: `src-tauri/crates/download-core/src/naming.rs`

- [ ] **Step 1: Écrire les tests qui échouent**

Ajouter dans le module `#[cfg(test)] mod tests` de `naming.rs` :

```rust
    #[test]
    fn sanitize_title_empty_or_none_is_video() {
        assert_eq!(sanitize_title(None), "video");
        assert_eq!(sanitize_title(Some("")), "video");
        assert_eq!(sanitize_title(Some("   ")), "video");
    }

    #[test]
    fn sanitize_title_removes_illegal_chars() {
        assert_eq!(sanitize_title(Some("a:b?c*")), "abc");
        assert_eq!(sanitize_title(Some("a<b>c|d\"e")), "abcde");
    }

    #[test]
    fn sanitize_title_slash_is_not_a_path_separator() {
        // Anti-régression : '/' et '\' deviennent des espaces, jamais des séparateurs.
        assert_eq!(sanitize_title(Some("AC/DC - Live")), "AC DC - Live");
        assert_eq!(sanitize_title(Some("a\\b")), "a b");
    }

    #[test]
    fn sanitize_title_reserved_names_are_prefixed() {
        assert_eq!(sanitize_title(Some("CON")), "_CON");
        assert_eq!(sanitize_title(Some("nul")), "_nul");
        assert_eq!(sanitize_title(Some("COM1")), "_COM1");
    }

    #[test]
    fn sanitize_title_trims_trailing_dots_and_spaces() {
        assert_eq!(sanitize_title(Some("titre...  ")), "titre");
        assert_eq!(sanitize_title(Some("  un titre  ")), "un titre");
    }

    #[test]
    fn sanitize_title_caps_length() {
        let long = "a".repeat(300);
        let out = sanitize_title(Some(&long));
        assert_eq!(out.chars().count(), 120);
    }

    #[test]
    fn sanitize_title_keeps_unicode() {
        assert_eq!(sanitize_title(Some("Café 日本")), "Café 日本");
    }
```

- [ ] **Step 2: Lancer les tests pour vérifier l'échec**

Run: `cd src-tauri/crates/download-core && cargo test sanitize_title`
Expected: FAIL à la compilation — `cannot find function sanitize_title in this scope`.

- [ ] **Step 3: Implémenter `sanitize_title`**

Ajouter en haut du fichier `naming.rs` (après le `//! ...`), avant `choose_file_name` :

```rust
/// Noms de périphériques réservés Windows (comparaison insensible à la casse).
const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL",
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Transforme un titre de page libre en radical de nom de fichier sûr (sans extension).
/// `None`, chaîne vide ou inutilisable → `"video"`.
///
/// Contrairement à [`choose_file_name`], cette fonction ne traite jamais `/` ni `\`
/// comme des séparateurs de chemin : un titre comme `AC/DC` est conservé entier.
pub fn sanitize_title(raw: Option<&str>) -> String {
    let raw = match raw {
        Some(s) => s,
        None => return "video".to_string(),
    };
    // 1. Nettoyage caractère par caractère.
    let cleaned: String = raw
        .chars()
        .filter_map(|c| match c {
            '/' | '\\' => Some(' '),                                  // texte, pas séparateur
            '<' | '>' | ':' | '"' | '|' | '?' | '*' => None,         // interdits Windows
            c if (c as u32) < 0x20 => None,                          // caractères de contrôle
            c => Some(c),
        })
        .collect();
    // 2. Réduire les espaces multiples + trim.
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    // 3. Plafonner la longueur (frontières de caractères).
    let capped: String = collapsed.chars().take(120).collect();
    // 4. Couper points et espaces de fin (interdits/problématiques sous Windows).
    let trimmed = capped.trim_end_matches(['.', ' ']).to_string();
    if trimmed.is_empty() {
        return "video".to_string();
    }
    // 5. Neutraliser les noms réservés.
    if RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(&trimmed)) {
        return format!("_{trimmed}");
    }
    trimmed
}
```

- [ ] **Step 4: Lancer les tests pour vérifier le succès**

Run: `cd src-tauri/crates/download-core && cargo test sanitize_title`
Expected: PASS (7 tests `sanitize_title_*`).

- [ ] **Step 5: Vérifier l'absence de warning et la non-régression**

Run: `cd src-tauri/crates/download-core && cargo test`
Expected: `132 + 7 = 139` (ou plus) tests lib passent, 0 échec, **0 warning**.

- [ ] **Step 6: Commit**

```bash
git checkout phase-4b3-media-filename-title
git add src-tauri/crates/download-core/src/naming.rs
git commit -m "feat(core): naming::sanitize_title (titre de page -> radical de fichier sûr)"
```

---

## Task 2: Assainir le titre au probe (HLS + DASH)

**Files:**
- Modify: `src-tauri/src/commands.rs` (lignes `164` et `220`)

- [ ] **Step 1: Remplacer le défaut `"video"` par `sanitize_title` (HLS)**

Dans `probe_hls_inner`, remplacer :

```rust
    let file_name = suggested_name.unwrap_or_else(|| "video".to_string());
```

par :

```rust
    let file_name = download_core::naming::sanitize_title(suggested_name.as_deref());
```

- [ ] **Step 2: Idem pour DASH**

Dans `probe_dash_inner`, remplacer la même ligne :

```rust
    let file_name = suggested_name.unwrap_or_else(|| "video".to_string());
```

par :

```rust
    let file_name = download_core::naming::sanitize_title(suggested_name.as_deref());
```

- [ ] **Step 3: Compiler et vérifier la non-régression**

Run: `cd src-tauri && cargo test`
Expected: PASS (tauri lib 9 + download-core), 0 échec, **0 warning**.

- [ ] **Step 4: Commit**

```bash
git checkout phase-4b3-media-filename-title
git add src-tauri/src/commands.rs
git commit -m "feat(core): sanitize le titre de page au probe HLS/DASH (au lieu de \"video\")"
```

---

## Task 3: Paramètre `file_name` éditable dans `add_media_download`

**Files:**
- Modify: `src-tauri/src/commands.rs` (`add_media_download`, lignes `258`, `260-265`)

- [ ] **Step 1: Ajouter le paramètre à la signature**

Remplacer :

```rust
pub async fn add_media_download(app: AppHandle, probe_id: String, variant_index: u32) -> Result<Download, String> {
```

par :

```rust
pub async fn add_media_download(app: AppHandle, probe_id: String, variant_index: u32, file_name: Option<String>) -> Result<Download, String> {
```

- [ ] **Step 2: Utiliser le nom édité (ou repli sur le titre du probe)**

Remplacer le bloc :

```rust
        let pm = pending.get(&probe_id).ok_or_else(|| "probe expiré".to_string())?;
        let file_name = download_core::naming::with_extension(
            &download_core::naming::choose_file_name(Some(&pm.file_name), None), "mp4");
```

par :

```rust
        let pm = pending.get(&probe_id).ok_or_else(|| "probe expiré".to_string())?;
        // Nom édité par l'utilisateur (assaini de nouveau, défense en profondeur) ;
        // sinon repli sur le titre déjà assaini au probe.
        let stem = match file_name.as_deref() {
            Some(n) => download_core::naming::sanitize_title(Some(n)),
            None => pm.file_name.clone(),
        };
        let file_name = download_core::naming::with_extension(&stem, "mp4");
```

(Le `let file_name` interne réutilise volontairement le nom : le reste de la fonction — destructure `(built, file_name, headers)`, `dest_path`, `Download.file_name` — est inchangé.)

- [ ] **Step 3: Compiler**

Run: `cd src-tauri && cargo build`
Expected: compile sans erreur. (L'appel JS sera mis à jour en Task 4 ; côté Rust le paramètre `Option<String>` accepte l'absence.)

- [ ] **Step 4: Vérifier la non-régression**

Run: `cd src-tauri && cargo test`
Expected: PASS, 0 échec, **0 warning**.

- [ ] **Step 5: Commit**

```bash
git checkout phase-4b3-media-filename-title
git add src-tauri/src/commands.rs
git commit -m "feat(core): add_media_download accepte un nom de fichier édité (Option)"
```

---

## Task 4: Champ nom éditable côté frontend (TDD)

**Files:**
- Modify: `src/api.ts` (ligne `30-32`)
- Modify: `src/components/HlsVariantDialog.tsx`
- Create: `src/components/HlsVariantDialog.test.tsx`
- Modify: `src/App.tsx` (`handleHlsConfirm`, lignes `55-63`)

- [ ] **Step 1: Écrire le test Vitest qui échoue**

Créer `src/components/HlsVariantDialog.test.tsx` :

```tsx
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
    expect(onConfirm).toHaveBeenCalledWith("p1", 0, "Mon clip");
  });
});
```

- [ ] **Step 2: Lancer le test pour vérifier l'échec**

Run: `npm test -- HlsVariantDialog`
Expected: FAIL — `getByLabelText("Nom du fichier")` introuvable et `onConfirm` reçoit 2 arguments au lieu de 3.

- [ ] **Step 3: Rendre le champ éditable dans `HlsVariantDialog.tsx`**

Remplacer la signature `onConfirm` du type des props :

```tsx
  onConfirm: (probeId: string, variantIndex: number) => void;
```

par :

```tsx
  onConfirm: (probeId: string, variantIndex: number, fileName: string) => void;
```

Remplacer l'état + `useEffect` :

```tsx
  const [selected, setSelected] = useState(0);
  useEffect(() => {
    if (event && event.variants.length) {
      const best = event.variants.reduce((a, b) => (b.bandwidth > a.bandwidth ? b : a));
      setSelected(best.index);
    }
  }, [event]);
```

par :

```tsx
  const [selected, setSelected] = useState(0);
  const [name, setName] = useState("");
  useEffect(() => {
    if (event) {
      if (event.variants.length) {
        const best = event.variants.reduce((a, b) => (b.bandwidth > a.bandwidth ? b : a));
        setSelected(best.index);
      }
      setName(event.file_name);
    }
  }, [event]);
```

Remplacer la ligne d'affichage du nom :

```tsx
        <p className="text-sm text-gray-500 mb-3 truncate">{event.file_name}</p>
```

par un champ éditable avec suffixe `.mp4` :

```tsx
        <div className="flex items-center gap-1 mb-3">
          <input
            className="flex-1 text-sm border border-gray-300 rounded px-2 py-1"
            value={name}
            onChange={(e) => setName(e.target.value)}
            aria-label="Nom du fichier"
          />
          <span className="text-sm text-gray-400 shrink-0">.mp4</span>
        </div>
```

Remplacer l'appel `onConfirm` du bouton :

```tsx
            onClick={() => onConfirm(event.probe_id, selected)}
```

par :

```tsx
            onClick={() => onConfirm(event.probe_id, selected, name.trim())}
```

- [ ] **Step 4: Mettre à jour `api.ts`**

Remplacer :

```ts
export function addMediaDownload(probeId: string, variantIndex: number): Promise<Download> {
  return invoke<Download>("add_media_download", { probeId, variantIndex });
}
```

par :

```ts
export function addMediaDownload(probeId: string, variantIndex: number, fileName: string): Promise<Download> {
  return invoke<Download>("add_media_download", { probeId, variantIndex, fileName });
}
```

- [ ] **Step 5: Mettre à jour `handleHlsConfirm` dans `App.tsx`**

Remplacer :

```tsx
  async function handleHlsConfirm(probeId: string, variantIndex: number) {
    setMediaEvent(null);
    try {
      await api.addMediaDownload(probeId, variantIndex);
      await refresh();
    } catch (e) {
      setError(`Impossible de démarrer la vidéo : ${String(e)}`);
    }
  }
```

par :

```tsx
  async function handleHlsConfirm(probeId: string, variantIndex: number, fileName: string) {
    setMediaEvent(null);
    try {
      await api.addMediaDownload(probeId, variantIndex, fileName);
      await refresh();
    } catch (e) {
      setError(`Impossible de démarrer la vidéo : ${String(e)}`);
    }
  }
```

- [ ] **Step 6: Lancer le test pour vérifier le succès**

Run: `npm test -- HlsVariantDialog`
Expected: PASS (2 tests).

- [ ] **Step 7: Vérifier la non-régression + build**

Run: `npm test`
Expected: `38 + 2 = 40` tests passent.

Run: `npm run build`
Expected: build `tsc + vite` sans erreur de type.

- [ ] **Step 8: Commit**

```bash
git checkout phase-4b3-media-filename-title
git add src/api.ts src/components/HlsVariantDialog.tsx src/components/HlsVariantDialog.test.tsx src/App.tsx
git commit -m "feat(frontend): champ nom de fichier éditable dans la boîte de variantes"
```

---

## Task 5: L'extension envoie `document.title`

**Files:**
- Modify: `extension/background.js` (`sendMedia` ~ligne `194`, handler `sendMedia` ~ligne `301`)

> Pas de runner de test pour l'extension à ce stade : vérification par revue manuelle. `buildCapture.js` accepte déjà `filename` (liste blanche), donc inchangé ; `flushPending` envoie déjà `filename` + `kind`, donc le chemin file d'attente fonctionne sans modification.

- [ ] **Step 1: Ajouter le paramètre `title` à `sendMedia`**

Remplacer la fonction `sendMedia` :

```js
async function sendMedia(url, referrer) {
  const kind = isDashUrl(url) ? "dash" : "hls";
  if (socket && socket.readyState === WebSocket.OPEN && connected) {
    try {
      const headers = await gatherHeaders(url, referrer);
      socket.send(JSON.stringify(buildCapture(url, { ...headers, kind })));
      return true;
    } catch (_e) {
      return false;
    }
  }
  pending.push({ url, referrer, kind, ts: Date.now() });
  if (pending.length > 20) pending.shift();
  connect();
  return true;
}
```

par :

```js
async function sendMedia(url, referrer, title) {
  const kind = isDashUrl(url) ? "dash" : "hls";
  if (socket && socket.readyState === WebSocket.OPEN && connected) {
    try {
      const headers = await gatherHeaders(url, referrer);
      socket.send(JSON.stringify(buildCapture(url, { ...headers, kind, filename: title })));
      return true;
    } catch (_e) {
      return false;
    }
  }
  pending.push({ url, referrer, kind, filename: title, ts: Date.now() });
  if (pending.length > 20) pending.shift();
  connect();
  return true;
}
```

- [ ] **Step 2: Récupérer `tab.title` dans le handler `sendMedia`**

Remplacer le handler :

```js
  if (req.cmd === "sendMedia" && req.url) {
    (async () => {
      let referrer;
      try {
        const tab = await chrome.tabs.get(req.tabId);
        referrer = tab && tab.url;
      } catch (_e) {
        // pas de referer
      }
      const ok = await sendMedia(req.url, referrer);
      sendResponse({ ok });
    })();
    return true; // réponse asynchrone
  }
```

par :

```js
  if (req.cmd === "sendMedia" && req.url) {
    (async () => {
      let referrer;
      let title;
      try {
        const tab = await chrome.tabs.get(req.tabId);
        referrer = tab && tab.url;
        title = tab && tab.title; // titre de la page = nom de fichier proposé
      } catch (_e) {
        // pas de referer / titre : Rust retombera sur "video"
      }
      const ok = await sendMedia(req.url, referrer, title);
      sendResponse({ ok });
    })();
    return true; // réponse asynchrone
  }
```

- [ ] **Step 3: Revue manuelle**

Vérifier : `buildCapture` omet les champs vides (donc `filename: undefined` n'est pas envoyé → repli `"video"` côté Rust) ; `flushPending` (ligne ~58) envoie déjà `filename: it.filename` + `kind: it.kind`.

- [ ] **Step 4: Commit**

```bash
git checkout phase-4b3-media-filename-title
git add extension/background.js
git commit -m "feat(extension): transmet document.title (tab.title) comme nom de fichier média"
```

---

## Task 6: Vérification finale

**Files:** aucun (vérification globale).

- [ ] **Step 1: Suite Rust complète**

Run: `cd src-tauri && cargo test` puis `cd src-tauri/crates/download-core && cargo test`
Expected: tout vert (tauri 9, download-core 139+), 0 échec, **0 warning**.

- [ ] **Step 2: Suite frontend + build**

Run: `npm test` puis `npm run build`
Expected: 40 tests passent ; build OK.

- [ ] **Step 3: Smoke manuel (note pour le réviseur)**

Charger l'extension, ouvrir une page vidéo HLS/DASH, capturer via le popup : vérifier que la boîte de dialogue pré-remplit le titre de la page, que l'édition fonctionne, et que le fichier final porte le nom édité (avec `.mp4`). Tester aussi un titre avec caractères interdits (`:`, `/`) et un onglet sans titre (→ `video.mp4`).

- [ ] **Step 4: Commit final éventuel** (si ajustements)

```bash
git checkout phase-4b3-media-filename-title
git add -A
git commit -m "chore(phase-4b3): vérification finale nom de fichier depuis le titre"
```

---

## Notes d'implémentation

- **DRY :** `sanitize_title` est l'unique source d'assainissement de titre (probe + commande).
- **YAGNI :** pas de dédup de noms, pas de nettoyage de suffixe de site, pas d'`og:title`.
- **Mapping Tauri :** l'argument JS `fileName` (camelCase) est mappé sur le paramètre Rust `file_name` (snake_case) automatiquement par Tauri v2.
- **Garde git :** rappel — `git checkout phase-4b3-media-filename-title` avant chaque commit de subagent.
