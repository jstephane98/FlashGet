# Loader pour les sondes déclenchées par l'extension — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Quand une sonde média est déclenchée depuis l'extension, l'app passe au premier plan et affiche un overlay « Recherche des formats… » jusqu'à l'arrivée du sélecteur de formats — même feedback que l'ajout in-app.

**Architecture:** Le bridge émet un nouvel event `media://probing` (+ focus de la fenêtre) au début d'une sonde extension ; le frontend l'écoute (`onMediaProbing`) et affiche un `ProbingOverlay`, masqué par les events existants `media://variants`/`media://error`, avec un timeout client 60 s. L'event est émis uniquement par le bridge → pas de conflit avec le spinner d'ajout in-app.

**Tech Stack:** Rust/Tauri (bridge WebSocket, `app.emit`), React/TypeScript, lucide-react (`Loader2`), event Tauri `media://probing`.

**Branche :** `phase-ext-probe-loader` (déjà créée depuis `main` ; la spec y est committée).

**NB testing :** glue Tauri/React → **pas de nouveau test unitaire**. Critères auto = `cargo check` (depuis `src-tauri/`, 0 erreur) + `npx tsc --noEmit` (0) + `npx vitest run` (52 verts). Smoke manuel = Task 4.

---

## File Structure

- **Modify** `src-tauri/src/bridge/server.rs` — émission `media://probing` + `focus_main_window` au début des sondes ; refactor DRY du focus inline du bras `add_url`.
- **Create** `src/components/ProbingOverlay.tsx` — overlay spinner « Recherche des formats… ».
- **Modify** `src/api.ts` — `onMediaProbing`.
- **Modify** `src/App.tsx` — état/ref/`stopExtProbing`/listener/timeout/rendu + `stopExtProbing()` dans `onMediaVariants`/`onMediaError`.

---

## Task 1: Backend — event `media://probing` + focus au début de la sonde

**Files:**
- Modify: `src-tauri/src/bridge/server.rs` (bras `ServerReply::CaptureRequested`, ~lignes 81-124)

- [ ] **Step 1 : Émettre `media://probing` + focus avant la chaîne de dispatch**

Dans `handle_conn`, bras `ServerReply::CaptureRequested { ... }`, **juste après** la ligne
`let headers = RequestHeaders { referrer, cookie, user_agent };` (server.rs:82) et **avant**
le `if kind.as_deref() == Some("smart")`, insérer :

```rust
                let is_probe = matches!(
                    kind.as_deref(),
                    Some("smart") | Some("hls") | Some("dash") | Some("page")
                );
                if is_probe {
                    // Feedback immédiat pour les sondes déclenchées par l'extension :
                    // l'app passe devant et affiche un overlay de chargement.
                    let _ = app.emit("media://probing", ());
                    crate::commands::focus_main_window(&app);
                }
```

Notes :
- `app` est l'`AppHandle` déjà utilisé dans cette fonction (cf. `app.emit("download://added", ...)` et `app.get_webview_window("main")` plus bas). `app.emit` et `commands::focus_main_window` sont déjà dans le scope du crate.
- `focus_main_window` est `pub(crate) fn focus_main_window(app: &AppHandle)` dans `src-tauri/src/commands.rs` (existant). Appel : `crate::commands::focus_main_window(&app)`.
- Vérifier que le trait `Emitter` (pour `app.emit`) est déjà importé dans server.rs — il l'est, puisque la ligne `app.emit("download://added", &d)` existe déjà dans la même fonction. Ne pas ré-importer.

- [ ] **Step 2 : Refactor DRY du focus inline du bras `add_url`**

Toujours dans `handle_conn`, le bras `else` (capture directe) contient (server.rs ~114-118) :
```rust
                            // Faire surgir la fenêtre de l'app au premier plan (UX type IDM).
                            if let Some(win) = app.get_webview_window("main") {
                                let _ = win.unminimize();
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
```
Le remplacer par :
```rust
                            // Faire surgir la fenêtre de l'app au premier plan (UX type IDM).
                            crate::commands::focus_main_window(&app);
```
(Comportement identique ; une seule définition de « ramener l'app au premier plan ».)

- [ ] **Step 3 : Vérifier que ça compile**

Run (depuis `src-tauri/`) : `cargo check`
Expected : 0 erreur, 0 warning. (Si `app.get_webview_window` n'est plus utilisé ailleurs dans server.rs après le refactor et qu'un import `Manager` devient inutilisé → le retirer ; mais vérifier d'abord qu'il n'est pas utilisé par d'autres fonctions du fichier. En cas de doute, laisser l'import.)

- [ ] **Step 4 : Commit**

Vérifier `git branch --show-current` == `phase-ext-probe-loader` (sinon `git checkout phase-ext-probe-loader`), puis :
```bash
git add src-tauri/src/bridge/server.rs
git commit -m "feat(bridge): event media://probing + focus fenêtre au début des sondes extension"
```

---

## Task 2: Frontend — `ProbingOverlay` + `api.onMediaProbing`

**Files:**
- Create: `src/components/ProbingOverlay.tsx`
- Modify: `src/api.ts`

- [ ] **Step 1 : Créer le composant `ProbingOverlay`**

Créer `src/components/ProbingOverlay.tsx` :
```tsx
import { Loader2 } from "lucide-react";

export function ProbingOverlay({ open, onClose }: { open: boolean; onClose: () => void }) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 bg-deep/80 flex items-center justify-center" onClick={onClose}>
      <div
        className="bg-bg border border-border rounded-xl p-5 w-[360px] text-ink shadow-[0_20px_50px_rgba(0,0,0,.5)] flex items-center gap-3"
        onClick={(e) => e.stopPropagation()}
      >
        <Loader2 size={20} className="animate-spin text-accent-bright" aria-hidden="true" />
        <span className="text-sm text-ink-soft">Recherche des formats disponibles…</span>
      </div>
    </div>
  );
}
```

- [ ] **Step 2 : Ajouter `onMediaProbing` dans api.ts**

Dans `src/api.ts`, juste après la fonction `onMediaError` (~ligne 28), ajouter :
```ts
export function onMediaProbing(cb: () => void): Promise<UnlistenFn> {
  return listen("media://probing", () => cb());
}
```
(`listen` et `UnlistenFn` sont déjà importés en haut de api.ts.)

- [ ] **Step 3 : Vérifier le typage**

Run (racine projet) : `npx tsc --noEmit`
Expected : 0 erreur. (Le composant et la fonction sont exportés mais pas encore consommés — un export non utilisé n'est PAS une erreur TS. Si `noUnusedLocals` se plaignait, ce serait sur des locales, pas des exports : aucun souci attendu ici.)

- [ ] **Step 4 : Commit**

Vérifier la branche `phase-ext-probe-loader`, puis :
```bash
git add src/components/ProbingOverlay.tsx src/api.ts
git commit -m "feat(ui): composant ProbingOverlay + api onMediaProbing"
```

---

## Task 3: Frontend — câblage dans `App.tsx`

**Files:**
- Modify: `src/App.tsx`

Dépend de : Task 2 (`ProbingOverlay`, `api.onMediaProbing`).

- [ ] **Step 1 : Importer le composant**

Dans `src/App.tsx`, après la ligne `import { HlsVariantDialog } from "./components/HlsVariantDialog";`, ajouter :
```tsx
import { ProbingOverlay } from "./components/ProbingOverlay";
```

- [ ] **Step 2 : Ajouter l'état + la ref**

Après la ligne `const probeTimeoutRef = useRef<number | null>(null);` (ajoutée par la feature précédente), ajouter :
```tsx
  const [extProbing, setExtProbing] = useState(false);
  const extProbeTimeoutRef = useRef<number | null>(null);
```

- [ ] **Step 3 : Ajouter le helper `stopExtProbing`**

Juste après le helper `stopProbing()` existant, ajouter :
```tsx
  function stopExtProbing() {
    setExtProbing(false);
    if (extProbeTimeoutRef.current != null) {
      clearTimeout(extProbeTimeoutRef.current);
      extProbeTimeoutRef.current = null;
    }
  }
```

- [ ] **Step 4 : Écouter `media://probing` + masquer sur variants/erreur**

Dans le `useEffect([])`, repérer le bloc des listeners media. Ajouter le listener probing
juste avant `const unMedia = api.onMediaVariants(...)` :
```tsx
    const unProbing = api.onMediaProbing(() => {
      setExtProbing(true);
      if (extProbeTimeoutRef.current != null) clearTimeout(extProbeTimeoutRef.current);
      extProbeTimeoutRef.current = window.setTimeout(() => {
        setExtProbing(false);
        extProbeTimeoutRef.current = null;
        setError("Délai dépassé — la recherche des formats n'a pas abouti.");
      }, 60000);
    });
```
Puis, dans le callback `onMediaVariants`, ajouter `stopExtProbing();` en première ligne du callback. De même, dans le callback `onMediaError`, ajouter `stopExtProbing();` en première ligne (avant la lecture de `addingRef.current`). Concrètement les deux callbacks deviennent :
```tsx
    const unMedia = api.onMediaVariants((e) => {
      stopExtProbing();
      stopProbing();
      setAddError(null);
      setDialogOpen(false);
      setMediaEvent(e);
    });
    const unMediaErr = api.onMediaError((e) => {
      stopExtProbing();
      const fromDialog = addingRef.current;
      stopProbing();
      if (fromDialog) setAddError(`Vidéo : ${e.message}`);
      else setError(`Vidéo : ${e.message}`);
    });
```

- [ ] **Step 5 : Nettoyer au démontage**

Dans le `return () => { ... }` de cleanup du `useEffect([])`, ajouter le désabonnement de
`unProbing` et le clear du timeout. Le cleanup actuel ressemble à :
```tsx
    return () => {
      un.then((f) => f()); unAdded.then((f) => f()); unMedia.then((f) => f()); unMediaErr.then((f) => f());
      if (probeTimeoutRef.current != null) clearTimeout(probeTimeoutRef.current);
    };
```
Le remplacer par :
```tsx
    return () => {
      un.then((f) => f()); unAdded.then((f) => f()); unMedia.then((f) => f()); unMediaErr.then((f) => f()); unProbing.then((f) => f());
      if (probeTimeoutRef.current != null) clearTimeout(probeTimeoutRef.current);
      if (extProbeTimeoutRef.current != null) clearTimeout(extProbeTimeoutRef.current);
    };
```
(Adapter aux noms réels des autres `unXxx` présents ; n'AJOUTER que `unProbing` et le clear de `extProbeTimeoutRef`.)

- [ ] **Step 6 : Rendre l'overlay**

Près des autres dialogues en bas du JSX (à côté de `<HlsVariantDialog ... />`), ajouter :
```tsx
      <ProbingOverlay open={extProbing} onClose={stopExtProbing} />
```

- [ ] **Step 7 : Vérifier typage + tests**

Run (racine projet) :
```
npx tsc --noEmit
npx vitest run
```
Expected : `tsc` 0 erreur ; Vitest 52 tests verts (aucune régression).

- [ ] **Step 8 : Commit**

Vérifier la branche `phase-ext-probe-loader`, puis :
```bash
git add src/App.tsx
git commit -m "feat(ui): overlay de sonde extension (onMediaProbing + timeout + câblage)"
```

---

## Task 4: Vérification finale

- [ ] **Step 1 : Backend**

Run (depuis `src-tauri/`) : `cargo check`
Expected : 0 erreur, 0 warning.

- [ ] **Step 2 : Front**

Run (racine projet) : `npx tsc --noEmit && npx vitest run`
Expected : `tsc` 0 erreur ; Vitest 52 verts.

- [ ] **Step 3 : Smoke-test manuel (utilisateur)**

`npm run tauri dev` + extension chargée →
  1. Depuis le navigateur, cliquer le bouton vidéo de l'extension sur une page média →
     l'app passe **au premier plan** avec l'overlay « Recherche des formats… » → le
     sélecteur de formats s'ouvre (overlay masqué).
  2. Page non supportée → overlay masqué + bandeau d'erreur rouge.
  3. Vérifier qu'un ajout **in-app** (coller une URL) affiche toujours le spinner dans
     `AddUrlDialog` et **pas** l'overlay (l'event probing est bridge-only).

---

## Self-Review

- **Couverture spec :** event `media://probing` + focus début + refactor DRY (Task 1) ; `ProbingOverlay` + `onMediaProbing` (Task 2) ; état/ref/`stopExtProbing`/listener/timeout/rendu + masquage sur variants/erreur (Task 3) ; vérif + smoke (Task 4). ✅ Toutes les sections de la spec ont une tâche.
- **Placeholders :** aucun — code complet à chaque step.
- **Cohérence des noms :** `media://probing`, `onMediaProbing`, `extProbing`/`setExtProbing`, `extProbeTimeoutRef`, `stopExtProbing`, `ProbingOverlay`, `focus_main_window` — identiques entre tâches et entre back/front. ✅ Distincts de l'existant `adding`/`stopProbing`/`probeTimeoutRef` (ajout in-app) → pas de collision.
- **Dépendances :** Task 1 (back) et Task 2 (composant+api) indépendantes ; Task 3 dépend de Task 2. Ordre respecté.
