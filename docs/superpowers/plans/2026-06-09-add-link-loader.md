# Loader fluide à l'ajout d'un lien — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Quand on ajoute un lien, `AddUrlDialog` reste ouvert avec un spinner « Recherche des formats… » du clic jusqu'à l'arrivée des formats, puis cède la main à `HlsVariantDialog` déjà rempli — fini l'impression de figement.

**Architecture:** `App.tsx` porte un état `adding` (spinner) + `addError` (erreur inline) et des refs (`addingRef`, `probeTimeoutRef`) lues par les listeners d'events `media://*` enregistrés une seule fois. `handleAdd` ne ferme plus la boîte au clic ; elle se ferme à l'arrivée de `media://variants` (qui ouvre le sélecteur rempli), affiche l'erreur inline sur échec, et un timeout client de 60 s évite tout spinner infini. `AddUrlDialog` gagne deux props `busy`/`error`.

**Tech Stack:** React + TypeScript, lucide-react (`Loader2`), Tailwind (`animate-spin`), events Tauri `media://variants` / `media://error`.

**Branche :** `phase-add-loader` (déjà créée depuis `main` ; la spec y est committée).

**NB testing :** glue UI événementielle → **pas de nouveau test unitaire** (cohérent avec les features UI précédentes). Critères automatiques = `npx tsc --noEmit` (0 erreur) + `npx vitest run` (52 verts). Smoke manuel listé en Task 3.

---

## File Structure

- **Modify** `src/components/AddUrlDialog.tsx` — props `busy`/`error`, spinner `Loader2`, reset `url` à l'ouverture, non-vidage de `url` sur soumission, anti-double-soumission.
- **Modify** `src/App.tsx` — états `adding`/`addError` + refs, helper `stopProbing`, réécriture `handleAdd` (timeout), modifs listeners `onMediaVariants`/`onMediaError`, câblage des props + ouverture.

---

## Task 1: `AddUrlDialog` — props `busy`/`error` + spinner

**Files:**
- Modify: `src/components/AddUrlDialog.tsx` (remplacement complet du composant)

- [ ] **Step 1 : Remplacer le composant**

Remplacer **tout** le contenu de `src/components/AddUrlDialog.tsx` par :

```tsx
import { useEffect, useState } from "react";
import { Loader2 } from "lucide-react";

export function AddUrlDialog({
  open,
  busy,
  error,
  onClose,
  onAdd,
}: {
  open: boolean;
  busy: boolean;
  error: string | null;
  onClose: () => void;
  onAdd: (url: string) => void;
}) {
  const [url, setUrl] = useState("");

  // Reset du champ à chaque ouverture fraîche de la boîte.
  useEffect(() => {
    if (open) setUrl("");
  }, [open]);

  if (!open) return null;

  const canSubmit = !busy && url.trim().length > 0;
  function submit() {
    if (!canSubmit) return;
    onAdd(url.trim());
    // On NE vide PAS `url` : en cas d'erreur inline, l'utilisateur peut corriger/réessayer.
  }

  return (
    <div className="fixed inset-0 bg-deep/80 flex items-center justify-center" onClick={onClose}>
      <div className="bg-bg border border-border rounded-xl p-5 w-[480px] text-ink shadow-[0_20px_50px_rgba(0,0,0,.5)]" onClick={(e) => e.stopPropagation()}>
        <h3 className="font-bold mb-3 text-ink">Nouveau téléchargement</h3>
        <input
          autoFocus
          className="w-full bg-surface text-ink placeholder:text-ink-mute border border-border rounded-md px-3 py-2 text-sm disabled:opacity-50"
          placeholder="Colle une URL…"
          value={url}
          disabled={busy}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") submit();
          }}
        />
        {busy && (
          <div className="flex items-center gap-2 mt-3 text-sm text-ink-soft">
            <Loader2 size={16} className="animate-spin" aria-hidden="true" />
            Recherche des formats disponibles…
          </div>
        )}
        {error && (
          <div className="mt-3 text-sm text-err break-words">{error}</div>
        )}
        <div className="flex justify-end gap-2 mt-4">
          <button className="px-3 py-1.5 text-sm rounded-md bg-surface text-ink-soft border border-border" onClick={onClose}>
            Annuler
          </button>
          <button
            className="px-3 py-1.5 text-sm rounded-md cta-grad font-semibold disabled:opacity-50 inline-flex items-center gap-2"
            disabled={!canSubmit}
            onClick={submit}
          >
            {busy && <Loader2 size={14} className="animate-spin" aria-hidden="true" />}
            Ajouter
          </button>
        </div>
      </div>
    </div>
  );
}
```

Notes :
- `busy` désactive le champ et le bouton « Ajouter » et affiche le spinner (sous le champ + dans le bouton). « Annuler » reste actif.
- `error` s'affiche en rouge (`text-err`) sans vider l'URL.
- `Loader2` + `animate-spin` (Tailwind built-in) = spinner ; pas de config Tailwind nécessaire.

- [ ] **Step 2 : Vérifier le typage**

Run (racine projet) : `npx tsc --noEmit`
Expected : **erreur attendue** dans `src/App.tsx` — l'appel actuel `<AddUrlDialog open=... onClose=... onAdd=... />` ne fournit pas les nouveaux props requis `busy`/`error`. C'est normal : Task 2 met à jour `App.tsx`. (Si tu veux un check isolé du composant, c'est la seule erreur attendue, et elle pointe `App.tsx`, pas `AddUrlDialog.tsx`.)

- [ ] **Step 3 : Commit**

Vérifier `git branch --show-current` == `phase-add-loader` (sinon `git checkout phase-add-loader`), puis :
```bash
git add src/components/AddUrlDialog.tsx
git commit -m "feat(ui): AddUrlDialog props busy/error + spinner Loader2"
```

---

## Task 2: `App.tsx` — état de sonde, timeout, câblage

**Files:**
- Modify: `src/App.tsx`

Dépend de : Task 1 (props `busy`/`error`).

- [ ] **Step 1 : Importer `useRef`**

Ligne 1 de `src/App.tsx`, remplacer :
```tsx
import { useEffect, useState } from "react";
```
par :
```tsx
import { useEffect, useRef, useState } from "react";
```

- [ ] **Step 2 : Ajouter les états et refs**

Juste après la ligne `const [mediaEvent, setMediaEvent] = useState<MediaVariantsEvent | null>(null);` (App.tsx:29), ajouter :
```tsx
  const [adding, setAdding] = useState(false);
  const [addError, setAddError] = useState<string | null>(null);
  const addingRef = useRef(false);
  const probeTimeoutRef = useRef<number | null>(null);
```

- [ ] **Step 3 : Ajouter le helper `stopProbing`**

Juste avant la fonction `refresh()` (App.tsx:32), ajouter :
```tsx
  function stopProbing() {
    setAdding(false);
    addingRef.current = false;
    if (probeTimeoutRef.current != null) {
      clearTimeout(probeTimeoutRef.current);
      probeTimeoutRef.current = null;
    }
  }
```

- [ ] **Step 4 : Brancher les listeners media**

Dans le `useEffect([])`, remplacer les deux lignes actuelles :
```tsx
    const unMedia = api.onMediaVariants((e) => setMediaEvent(e));
    const unMediaErr = api.onMediaError((e) => setError(`Vidéo : ${e.message}`));
```
par :
```tsx
    const unMedia = api.onMediaVariants((e) => {
      stopProbing();
      setAddError(null);
      setDialogOpen(false);   // ferme AddUrlDialog (sans effet si déjà fermée — cas extension)
      setMediaEvent(e);       // ouvre HlsVariantDialog, déjà rempli
    });
    const unMediaErr = api.onMediaError((e) => {
      const fromDialog = addingRef.current;  // lire AVANT stopProbing (qui le remet à false)
      stopProbing();
      if (fromDialog) setAddError(`Vidéo : ${e.message}`);
      else setError(`Vidéo : ${e.message}`);
    });
```

- [ ] **Step 5 : Réécrire `handleAdd`**

Remplacer la fonction `handleAdd` actuelle (App.tsx:69-82) :
```tsx
  async function handleAdd(url: string) {
    setDialogOpen(false);
    setError(null);
    setInfo(null);
    try {
      await api.addDownload(url, DEST_DIR);
      await refresh();
    } catch (e) {
      const msg = String(e);
      if (!msg.includes("__media_pending__")) {
        setError(`Impossible d'ajouter ce lien : ${msg}`);
      }
    }
  }
```
par :
```tsx
  async function handleAdd(url: string) {
    // La boîte reste ouverte : spinner pendant la sonde.
    setAdding(true);
    addingRef.current = true;
    setAddError(null);
    setError(null);
    setInfo(null);
    try {
      await api.addDownload(url, DEST_DIR);
      // Fichier normal : la sonde a réussi sans variantes → on ferme et on rafraîchit.
      stopProbing();
      setDialogOpen(false);
      await refresh();
    } catch (e) {
      const msg = String(e);
      if (msg.includes("__media_pending__")) {
        // Sonde média en tâche de fond : le spinner continue jusqu'à media://variants
        // ou media://error. Filet de sécurité : timeout 60 s.
        probeTimeoutRef.current = window.setTimeout(() => {
          stopProbing();
          setAddError("Délai dépassé — réessaie.");
        }, 60000);
      } else {
        stopProbing();
        setAddError(`Impossible d'ajouter ce lien : ${msg}`);
      }
    }
  }
```

- [ ] **Step 6 : Câbler les props de `AddUrlDialog`**

Remplacer la ligne actuelle (App.tsx:149) :
```tsx
      <AddUrlDialog open={dialogOpen} onClose={() => setDialogOpen(false)} onAdd={handleAdd} />
```
par :
```tsx
      <AddUrlDialog
        open={dialogOpen}
        busy={adding}
        error={addError}
        onClose={() => { stopProbing(); setAddError(null); setDialogOpen(false); }}
        onAdd={handleAdd}
      />
```

- [ ] **Step 7 : Reset à l'ouverture de la boîte**

Remplacer dans le `<Toolbar ... />` la prop `onAdd` (App.tsx:113) :
```tsx
        onAdd={() => setDialogOpen(true)}
```
par :
```tsx
        onAdd={() => { stopProbing(); setAddError(null); setDialogOpen(true); }}
```

- [ ] **Step 8 : Vérifier typage + tests**

Run (racine projet) :
```
npx tsc --noEmit
npx vitest run
```
Expected : `tsc` 0 erreur ; Vitest 52 tests verts (aucune régression).

- [ ] **Step 9 : Commit**

Vérifier la branche `phase-add-loader`, puis :
```bash
git add src/App.tsx
git commit -m "feat(ui): spinner d'ajout + erreurs inline + timeout sonde (App)"
```

---

## Task 3: Vérification finale

- [ ] **Step 1 : Typage**

Run (racine projet) : `npx tsc --noEmit`
Expected : 0 erreur.

- [ ] **Step 2 : Tests front**

Run (racine projet) : `npx vitest run`
Expected : 52 tests verts.

- [ ] **Step 3 : Smoke-test manuel (utilisateur)**

`npm run tauri dev` →
  1. Coller une URL de page média (ex. YouTube) → clic « Ajouter » → **spinner immédiat** « Recherche des formats… » dans la boîte → le sélecteur de formats s'ouvre **rempli** (plus de trou/figement).
  2. Coller un lien invalide / non supporté → **erreur inline** rouge dans la boîte, URL conservée pour corriger.
  3. Coller un fichier direct (ex. `.exe`/`.zip`) → spinner bref → la boîte se ferme, le fichier apparaît dans la liste.
  4. Cliquer « Annuler » pendant le spinner → la boîte se ferme, pas de spinner résiduel.

---

## Self-Review

- **Couverture spec :** props `busy`/`error` + spinner + reset url + non-vidage (Task 1) ; `adding`/`addError` + refs + `stopProbing` + `handleAdd` timeout + listeners variants/error + câblage + ouverture (Task 2) ; vérif + smoke (Task 3). ✅ Toutes les sections de la spec ont une tâche.
- **Placeholders :** aucun — code complet à chaque step.
- **Cohérence des noms :** `adding`/`setAdding`, `addError`/`setAddError`, `addingRef`, `probeTimeoutRef`, `stopProbing`, props `busy`/`error` — identiques entre les tâches et entre composant/parent. ✅
- **Dépendances :** Task 1 (composant) doit précéder Task 2 (qui fournit les props) ; à la fin de Task 1, `tsc` signale **volontairement** l'appel non encore mis à jour dans App.tsx — résolu en Task 2 step 8. Ordre respecté.
