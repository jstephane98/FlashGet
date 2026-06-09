# Loader pour les sondes déclenchées par l'extension — Design

**Date :** 2026-06-09
**Branche prévue :** `phase-ext-probe-loader` (depuis `main`)
**Statut :** validé par l'utilisateur (overlay loader dans l'app ; focus de la fenêtre au début)
**Suite de :** [[2026-06-09-add-link-loader-design]] (loader de l'ajout in-app, déjà mergé)

## Problème

L'ajout in-app affiche désormais un spinner pendant la sonde. Mais quand la sonde est
**déclenchée depuis l'extension** (bouton vidéo du popup → bridge WebSocket →
`bridge/server.rs:handle_conn` → `probe_smart`/`probe_hls`/`probe_dash`/`probe_ytdlp`),
il n'y a **aucun `AddUrlDialog` ouvert**, donc aucun feedback : 5–30 s (yt-dlp) sans rien,
puis `HlsVariantDialog` surgit. De plus, la fenêtre n'est ramenée au premier plan qu'à la
**fin** de la sonde (au moment du picker). Il manque (a) un event « sonde démarrée » que
l'app puisse écouter, (b) un loader visible, (c) le focus de la fenêtre dès le début.

## Décisions validées

| Sujet | Choix |
|-------|-------|
| Emplacement du feedback | **Overlay loader dans l'app** (nouvel event `media://probing` → overlay spinner, masqué à l'arrivée du picker/erreur). |
| Focus fenêtre | **Au début** de la sonde extension (l'app passe devant dès le clic, pour que le loader soit vu). |
| Périmètre de l'event | `media://probing` émis **uniquement depuis le bridge** (chemin extension). L'ajout in-app garde son spinner dans `AddUrlDialog` → pas de double-loader. |

## Architecture

### Backend — `src-tauri/src/bridge/server.rs` (`handle_conn`)

Dans le bras `ServerReply::CaptureRequested`, **avant** la chaîne `if kind == ...`, ajouter
un préambule pour les *kinds* qui sondent (`smart`/`hls`/`dash`/`page`) :

```rust
let is_probe = matches!(
    kind.as_deref(),
    Some("smart") | Some("hls") | Some("dash") | Some("page")
);
if is_probe {
    let _ = app.emit("media://probing", ());
    crate::commands::focus_main_window(&app);   // ramène la fenêtre au premier plan dès le début
}
```

- `media://probing` : payload vide (`()`). La sémantique « une sonde extension a démarré »
  suffit ; l'overlay affiche un texte générique.
- `focus_main_window` existe déjà (`pub(crate) fn focus_main_window(app: &AppHandle)` dans
  `commands.rs`, ajouté à la feature notif). Réutilisé ici → DRY.
- **Refactor DRY (inclus)** : le bras `else` (capture directe, `add_url`) focuse la fenêtre
  en inline (server.rs:114-118) ; le remplacer par `crate::commands::focus_main_window(&app);`.
- Les sondes (`probe_*` dans commands.rs) focusent déjà à la fin (succès/erreur) : inchangé,
  redondant mais inoffensif maintenant que le focus a lieu aussi au début.

### Frontend

**`src/api.ts`** — nouveau listener :
```ts
export function onMediaProbing(cb: () => void): Promise<UnlistenFn> {
  return listen("media://probing", () => cb());
}
```

**`src/components/ProbingOverlay.tsx`** — nouveau composant (petit overlay centré, même
langage visuel que `AddUrlDialog`) :
```tsx
import { Loader2 } from "lucide-react";

export function ProbingOverlay({ open, onClose }: { open: boolean; onClose: () => void }) {
  if (!open) return null;
  return (
    <div className="fixed inset-0 bg-deep/80 flex items-center justify-center" onClick={onClose}>
      <div className="bg-bg border border-border rounded-xl p-5 w-[360px] text-ink shadow-[0_20px_50px_rgba(0,0,0,.5)] flex items-center gap-3" onClick={(e) => e.stopPropagation()}>
        <Loader2 size={20} className="animate-spin text-accent-bright" aria-hidden="true" />
        <span className="text-sm text-ink-soft">Recherche des formats disponibles…</span>
      </div>
    </div>
  );
}
```
(Cliquer en dehors ferme l'overlay ; la sonde de fond continue — si `media://variants`
arrive ensuite, le picker s'ouvre quand même.)

**`src/App.tsx`** :
- État `const [extProbing, setExtProbing] = useState(false);` + ref
  `const extProbeTimeoutRef = useRef<number | null>(null);`.
- Helper local `stopExtProbing()` : `setExtProbing(false)` + clear `extProbeTimeoutRef`.
- Dans le `useEffect([])` :
  ```ts
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
  Ajouter `unProbing.then((f) => f());` au cleanup, + `clearTimeout(extProbeTimeoutRef.current)`.
- Dans les listeners existants `onMediaVariants` **et** `onMediaError`, ajouter `stopExtProbing();`
  (masque l'overlay quand le résultat arrive ; ces events sont émis aussi par les sondes extension).
- Rendu : `<ProbingOverlay open={extProbing} onClose={stopExtProbing} />`.

## Flux résultant (extension)

```
clic popup extension → background → WS → bridge handle_conn
  → emit media://probing + focus_main_window (fenêtre devant)
  → frontend : extProbing=true → overlay « Recherche des formats… »
  → probe_* (5–30 s) → media://variants (picker, overlay masqué)  | media://error (overlay masqué, bandeau)
  → filet : timeout 60 s → overlay masqué + bandeau
```

## Cas limites

- **In-app vs extension** : `media://probing` n'est émis que par le bridge → l'overlay ne
  s'affiche jamais pour un ajout in-app (qui a son propre spinner). Pas de conflit.
- **Fermeture manuelle de l'overlay** : `stopExtProbing()` ; une arrivée tardive de variants
  ouvre quand même le picker.
- **Démontage** : le timeout est nettoyé dans le cleanup du `useEffect`.
- **Limite connue (héritée, non corrigée)** : events `media://*` globaux non corrélés par
  `probe_id` → reste le cas de mauvaise attribution déjà documenté dans
  [[feature-add-link-loader]]. Hors périmètre.

## Stratégie de test

- **Pas de nouveau test unitaire** : glue Tauri/React événementielle (cohérent avec les
  features UI précédentes).
- **Automatique** : `cargo check` (depuis `src-tauri/`, 0 erreur) + `npx tsc --noEmit` (0) +
  `npx vitest run` (52 verts, aucune régression).
- **Smoke-test manuel (utilisateur)** : depuis le navigateur, cliquer le bouton vidéo de
  l'extension → l'app passe **au premier plan** avec l'overlay « Recherche des formats… » →
  le sélecteur de formats s'ouvre (overlay masqué). Tester aussi un lien non supporté →
  overlay masqué + bandeau d'erreur.

## Hors périmètre (YAGNI)

- Spinner côté popup de l'extension (le bridge renvoie déjà un `ack` ignoré — on ne l'exploite pas).
- Texte d'overlay dynamique (nom de fichier / type de flux) : générique suffit.
- Corrélation `probe_id` des events (limite héritée, backlog).
- Bouton « Annuler » qui stoppe réellement la sonde backend (l'overlay ne masque que l'UI).

## Découpage d'implémentation (pour le plan)

1. **Backend** : `media://probing` + `focus_main_window` au début des sondes dans
   `bridge/server.rs` (+ refactor DRY du focus inline du bras `add_url`). (`cargo check`)
2. **Frontend composant + api** : `ProbingOverlay.tsx` + `api.onMediaProbing`. (`tsc`)
3. **Frontend câblage** : état/ref/`stopExtProbing`/listener/timeout/rendu dans `App.tsx`
   + `stopExtProbing()` dans `onMediaVariants`/`onMediaError`. (`tsc` + `vitest`)

Tâche 1 indépendante ; tâche 2 indépendante ; tâche 3 dépend de 2 (consomme l'overlay + l'api).
