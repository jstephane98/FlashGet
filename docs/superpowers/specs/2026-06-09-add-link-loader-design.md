# Loader fluide à l'ajout d'un lien — Design

**Date :** 2026-06-09
**Branche prévue :** `phase-add-loader` (depuis `main`)
**Statut :** validé par l'utilisateur (Approche 1 ; erreurs inline ; timeout client ~60 s)

## Problème

Au clic sur « Ajouter » dans `AddUrlDialog`, la boîte se ferme immédiatement
([App.tsx:70](../../../src/App.tsx)), puis `add_download` sonde l'URL. Pour un lien
média/HTML, le backend renvoie l'erreur sentinelle `__media_pending__` et lance une
sonde **en tâche de fond** (yt-dlp / HLS / DASH) qui émet plus tard l'event
`media://variants` (→ ouverture de `HlsVariantDialog`) ou `media://error`. Entre la
fermeture de la boîte et l'arrivée de l'event, il s'écoule 1–3 s (parfois plus) **sans
aucun retour visuel** → sensation d'application figée.

## Décisions validées

| Sujet | Choix |
|-------|-------|
| Approche | **Loader dans `AddUrlDialog`** : la boîte reste ouverte avec un spinner du clic jusqu'aux formats, puis cède la main à `HlsVariantDialog` déjà rempli. |
| Erreur de sonde | **Inline** dans la boîte (la boîte reste ouverte pour corriger l'URL). |
| Filet de sécurité | **Timeout client ~60 s** sans event → arrêt du spinner + message. |

## Architecture

### État (porté par `App.tsx`)

Nouveaux états :
- `adding: boolean` — une sonde média est en cours (pilote le spinner d'`AddUrlDialog`).
- `addError: string | null` — message d'erreur **inline** affiché dans `AddUrlDialog`.

Nouvelles refs (les listeners d'events sont enregistrés une seule fois dans le
`useEffect([])`, donc ils ne voient pas l'état React courant — d'où des refs) :
- `addingRef: MutableRefObject<boolean>` — miroir de `adding`, lu dans les listeners.
- `probeTimeoutRef: MutableRefObject<number | null>` — id du timeout 60 s, pour l'annuler.

Helper interne `stopProbing()` : `setAdding(false)`, `addingRef.current = false`, et si
`probeTimeoutRef.current != null` → `clearTimeout` + remise à `null`. Centralise l'arrêt
propre du spinner (appelé depuis succès, variants, erreur, timeout, fermeture).

### Flux `handleAdd(url)` (réécriture)

```
1. setAdding(true) ; addingRef.current = true ; setAddError(null)
   (la boîte NE se ferme PAS — plus de setDialogOpen(false) ici)
2. try await api.addDownload(url, DEST_DIR)
   → succès (fichier normal) : stopProbing() ; setDialogOpen(false) ; await refresh()
3. catch (e) :
   msg = String(e)
   - msg.includes("__media_pending__") :
       armer le timeout : probeTimeoutRef.current = setTimeout(() => {
         stopProbing(); setAddError("Délai dépassé — réessaie.");
       }, 60000)
       (on garde adding=true et la boîte ouverte ; le spinner continue)
   - sinon (erreur réelle) :
       stopProbing() ; setAddError(`Impossible d'ajouter ce lien : ${msg}`)
```

### Listeners d'events (modifs dans le `useEffect([])`)

- `onMediaVariants((e) => { ... })` :
  ```
  stopProbing()
  setAddError(null)
  setDialogOpen(false)   // ferme AddUrlDialog (sans effet si déjà fermée — cas extension)
  setMediaEvent(e)       // ouvre HlsVariantDialog, déjà rempli
  ```
- `onMediaError((e) => { ... })` :
  ```
  stopProbing()
  if (addingRef.current était vrai au moment de la sonde) ... 
  ```
  Routage : si on sondait depuis la boîte → inline, sinon bandeau global. Comme
  `stopProbing()` remet `addingRef` à `false`, **lire `addingRef.current` AVANT
  `stopProbing()`** :
  ```
  const fromDialog = addingRef.current;
  stopProbing();
  if (fromDialog) setAddError(`Vidéo : ${e.message}`);
  else setError(`Vidéo : ${e.message}`);   // repli : sonde déclenchée hors UI (extension)
  ```

### `AddUrlDialog` (composant) — `src/components/AddUrlDialog.tsx`

Props ajoutés : `busy: boolean` et `error: string | null`.

- **Reset à l'ouverture fraîche** : un `useEffect([open])` qui, quand `open` passe à
  `true`, remet `url` à `""` (le reset de `addError`/`adding` est géré côté App à
  l'ouverture ; voir ci-dessous). On conserve l'URL pendant un réessai après erreur
  (ne pas vider `url` sur erreur).
- **Soumission** : `onAdd(url.trim())` — **ne plus vider `url` immédiatement** (on le
  garde au cas où une erreur inline demande un réessai). Bloquer la double soumission
  quand `busy` (Enter et clic désactivés).
- **`busy`** : afficher un spinner `Loader2` (import `lucide-react`, `className="animate-spin"`)
  + texte « Recherche des formats disponibles… » ; `input` `disabled`, bouton « Ajouter »
  `disabled` (et idéalement libellé remplacé par le spinner). Bouton « Annuler » reste
  actif (permet d'abandonner).
- **`error`** : afficher le message en rouge (classes existantes type `text-err`) sous le
  champ ; n'efface pas l'URL.
- **Fermeture** (`onClose`, clic overlay ou « Annuler ») : appelle le `onClose` fourni.

### Câblage `App.tsx`

```tsx
<AddUrlDialog
  open={dialogOpen}
  busy={adding}
  error={addError}
  onClose={() => { stopProbing(); setAddError(null); setDialogOpen(false); }}
  onAdd={handleAdd}
/>
```

À l'ouverture (`onAdd` du Toolbar, [App.tsx:113](../../../src/App.tsx)) :
`() => { setAddError(null); setAdding(false); addingRef.current = false; setDialogOpen(true); }`.

## Gestion des cas limites

- **Fichier normal** : le spinner s'affiche pendant l'`await addDownload` (~1–3 s, qui
  était déjà silencieux avant), puis la boîte se ferme et le fichier apparaît.
- **Fermeture manuelle pendant la sonde** : `stopProbing()` (annule le timeout). Si
  `media://variants` arrive ensuite, le sélecteur s'ouvre quand même (comportement
  acceptable). Le spinner ne réapparaît pas.
- **Sonde déclenchée hors UI (extension/bridge)** : `addingRef.current` est `false`,
  donc une `media://error` part dans le bandeau global ; `media://variants` ouvre le
  sélecteur comme aujourd'hui.
- **Timeout 60 s** : couvre le cas où ni `variants` ni `error` n'arrivent.

## Stratégie de test

- **Pas de nouveau test unitaire** : la logique est de la glue React événementielle
  (états + refs + timeout + events Tauri), sans unité pure isolable proprement —
  cohérent avec la posture des features UI précédentes du projet (logique pure testée,
  glue smoke-testée).
- **Automatique** : `npx tsc --noEmit` (0 erreur) + `npx vitest run` (52 tests verts,
  aucune régression).
- **Smoke-test manuel (utilisateur)** : 
  1. Lien média (ex. page YouTube) → spinner **immédiat** dans la boîte → le sélecteur
     de formats s'ouvre rempli (plus de « trou »).
  2. Lien invalide / non supporté → erreur **inline** dans la boîte, URL conservée.
  3. Fichier direct (.exe, .zip) → spinner bref → la boîte se ferme, fichier dans la liste.

## Hors périmètre (YAGNI)

- Annulation réelle de la sonde backend (le bouton « Annuler » arrête seulement le
  spinner côté UI ; la tâche de fond se termine seule).
- Barre de progression de la sonde (indéterminé : un simple spinner suffit).
- Fusion d'`AddUrlDialog` et `HlsVariantDialog` en une seule boîte qui se métamorphose.
- Loader pour l'ajout en lot (`AddBatchDialog`) — flux différent, déjà synchrone.

## Découpage d'implémentation (pour le plan)

1. **`AddUrlDialog`** : props `busy`/`error`, spinner `Loader2`, reset `url` à
   l'ouverture, non-vidage sur erreur, anti-double-soumission. (Composant isolé.)
2. **`App.tsx`** : états `adding`/`addError` + refs, `stopProbing()`, réécriture de
   `handleAdd` (timeout inclus), modifs `onMediaVariants`/`onMediaError`, câblage des
   props + ouverture.

Tâche 1 indépendante (le composant accepte les nouveaux props sans casser l'appel
actuel si on leur donne des défauts) ; tâche 2 utilise les nouveaux props.
