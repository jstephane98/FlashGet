# FlashGet v2 — Phase 1 : coque et page Téléchargements — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal :** remplacer l'écran unique actuel par la nouvelle coque de FlashGet : fenêtre sans bordure avec barre de titre, menu latéral, barre d'état, thèmes sombre/clair/système, et la nouvelle page Téléchargements avec une barre de progression découpée par connexion.

**Architecture :** le front passe d'un écran unique à une coque (`AppShell`) qui affiche une page selon l'entrée active du menu. La navigation se fait par un simple état React, sans bibliothèque de routage. Les couleurs passent par des variables CSS que Tailwind consomme : changer de thème, c'est changer `data-theme` sur `<html>`. Le moteur Rust ajoute une seule information à l'événement de progression : la fraction terminée de chaque segment.

**Tech Stack :** Tauri 2, React 19, TypeScript 5.8, Tailwind 3, Vitest + Testing Library, Rust (`download-core`, `rusqlite`), lucide-react.

**Spec :** [`../specs/2026-11-10-flashget-v2-refonte-design.md`](../specs/2026-11-10-flashget-v2-refonte-design.md). Maquette de référence : [`../specs/2026-11-10-flashget-v2-maquettes/01-fenetre-principale.html`](../specs/2026-11-10-flashget-v2-maquettes/01-fenetre-principale.html) (direction A = sombre, direction B = clair).

**Feuille de route :** [`2026-11-10-flashget-v2-feuille-de-route.md`](2026-11-10-flashget-v2-feuille-de-route.md)

## Global Constraints

- Aucun code, composant ou ressource repris de NeoDLP ; aucune mention d'un tiers (spec §2, anti-objectifs).
- Aucune capacité actuelle ne doit disparaître : pause, reprise, annulation, arrêt d'un direct, priorité, ouvrir le fichier ou le dossier, retirer de la liste, ajout groupé, choix de variante HLS/DASH/yt-dlp, découpe, connexion YouTube, réglages existants (spec §2).
- Tout texte visible est en français et passe par `t()` (`src/i18n`) — spec §8.3.
- Polices : Archivo (texte), JetBrains Mono (chiffres, tailles, vitesses) — spec §3.
- Thème par défaut : **sombre** ; choix possibles : `dark`, `light`, `system` (spec §4.1).
- Ordre du menu : Nouveau (mis en avant), Téléchargements, Terminés, File d'attente, Planificateur, puis Paramètres en bas (spec §4.2). En phase 1, File d'attente et Planificateur sont masqués (pages livrées en phase 4).
- Un téléchargement terminé quitte la page Téléchargements et apparaît dans Terminés (spec §8.1).
- Tout le travail se fait sur la branche `flashget-v2-phase-1`, un commit par tâche.

## Review Focus

1. **Reprise d'un téléchargement multi-segments à moitié fini** : les segments déjà terminés doivent apparaître pleins dès la première mise à jour, pas vides. Test : Task 2, `parts_reflect_resumed_segments`.
2. **Téléchargement sans taille connue** (pas de `Content-Length`, ou yt-dlp sur des flux séparés) : pas de segments, barre animée, et ni `NaN` ni `Infinity` dans le temps restant ou le pourcentage. Tests : Task 2 `no_parts_for_unknown_size`, Task 7 `formatEta` et Task 8 `barre indéterminée sans taille`.
3. **Thème « Système » quand Windows change de mode pendant que FlashGet est ouvert** : l'application doit suivre tout de suite. Test : Task 4, `suit le changement du système en mode system`.
4. **Nom de fichier très long, ou lien sans hôte lisible** (`file:`, URL invalide) : le nom est tronqué sur une ligne et l'hôte est vide, sans planter. Tests : Task 7 `hostOf`, Task 8 `nom long tronqué`.
5. **Bouton de fermeture de la nouvelle barre de titre** : il doit réduire FlashGet dans la zone de notification (comportement actuel), pas le quitter, et les téléchargements continuent. Vérification manuelle : Task 10, étape 6.

---

## Fichiers

| Fichier | Rôle |
|---|---|
| `src-tauri/crates/download-core/src/model.rs` | `Progress.parts` |
| `src-tauri/crates/download-core/src/engine.rs` | calcule et émet `parts` |
| `src-tauri/crates/download-core/src/storage.rs` | `get_pref` / `set_pref` |
| `src-tauri/src/prefs.rs` (nouveau) | commandes `get_ui_pref` / `set_ui_pref` + liste blanche |
| `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json` | fenêtre sans bordure, permissions de fenêtre |
| `src/i18n/fr.ts`, `src/i18n/index.ts` (nouveaux) | textes FR et `t()` |
| `src/theme/theme.ts`, `src/theme/ThemeProvider.tsx` (nouveaux) | résolution et application du thème |
| `src/index.css`, `tailwind.config.js`, `index.html` | variables de couleur des deux thèmes |
| `src/components/shell/TitleBar.tsx`, `Sidebar.tsx`, `StatusBar.tsx` (nouveaux) | coque |
| `src/navigation.ts` (nouveau) | identifiants de page et entrées du menu |
| `src/format.ts` | `formatEta`, `hostOf` |
| `src/downloads/tabs.ts` (nouveau) | onglets Tous / Actifs / En pause / Erreurs |
| `src/components/downloads/SegmentBar.tsx`, `DownloadCard.tsx`, `RowMenu.tsx` (déplacé) | ligne de téléchargement |
| `src/pages/DownloadsPage.tsx`, `src/pages/CompletedPage.tsx` (nouveaux) | pages |
| `src/App.tsx` | devient la coque |
| supprimés | `Toolbar.tsx`, `CategoryTabs.tsx`, `DownloadTable.tsx`, `DownloadRow.tsx`, `CategoryPill.tsx`, `PriorityPill.tsx`, `filter.ts` (+ tests) |

---

### Task 1 : Base verte

Un test front échoue déjà sur `main` : `HlsVariantDialog.test.tsx › transmet le nom édité à onConfirm`. Le composant transmet désormais un 4ᵉ argument `timeRange` (fonction de découpe), `undefined` quand on ne découpe pas.

**Files :**
- Modify : `src/components/HlsVariantDialog.test.tsx:33`

- [ ] **Step 1 : Créer la branche**

Run : `git checkout -b flashget-v2-phase-1`

- [ ] **Step 2 : Corriger l'assertion**

```ts
expect(onConfirm).toHaveBeenCalledWith("p1", 0, "Mon clip", undefined);
```

- [ ] **Step 3 : Vérifier que tout passe**

Run : `npm test` → Expected : `Tests  52 passed`.
Run : `cargo test --manifest-path src-tauri/Cargo.toml --workspace` → Expected : `test result: ok` pour chaque crate.

- [ ] **Step 4 : Commit**

```bash
git add src/components/HlsVariantDialog.test.tsx
git commit -m "test: aligne HlsVariantDialog sur l'argument timeRange"
```

---

### Task 2 : Progression par segment dans le moteur

**Files :**
- Modify : `src-tauri/crates/download-core/src/model.rs:128-138`
- Modify : `src-tauri/crates/download-core/src/engine.rs` (fonction `download` et `emit`)
- Modify : chaque littéral `Progress { … }` (11 occurrences : `dash/engine.rs`, `dash/live.rs`, `hls/engine.rs`, `hls/live.rs`, `media_io.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/ytdlp.rs`) → ajouter `parts: None`
- Modify : `src/types.ts` (`Progress`)
- Test : module `tests` de `engine.rs`

**Interfaces :**
- Produces : `Progress.parts: Option<Vec<f32>>` (Rust, `#[serde(default)]`), une fraction dans `[0.0, 1.0]` par segment, dans l'ordre des segments. `None` pour un flux non segmenté (taille inconnue, ou un seul segment), et pour HLS/DASH/yt-dlp. Côté TS : `parts?: number[] | null`.

- [ ] **Step 1 : Écrire les tests qui échouent** (dans `engine.rs`, à côté de `emits_progress`)

```rust
#[tokio::test]
async fn parts_reported_per_segment() {
    let data: Vec<u8> = vec![7u8; 800];
    let fetcher = FakeFetcher::new(data, true);
    let dir = tempdir().unwrap();
    let dest = collect_dest(&dir);
    let (tx, mut rx) = mpsc::unbounded_channel();
    download(&fetcher, "p1", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default()).await.unwrap();
    let all: Vec<Progress> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
    let last = all.last().unwrap();
    assert_eq!(last.status, DownloadStatus::Completed);
    let parts = last.parts.as_ref().expect("parts attendus");
    assert_eq!(parts.len(), DEFAULT_SEGMENTS as usize);
    assert!(parts.iter().all(|f| (*f - 1.0).abs() < f32::EPSILON));
    assert!(all.iter().filter_map(|p| p.parts.as_ref()).flatten().all(|f| (0.0..=1.0).contains(f)));
}

#[tokio::test]
async fn no_parts_for_unknown_size() {
    let fetcher = FakeFetcher::new(vec![1u8; 300], false); // pas de Range
    // … même schéma ; assert!(all.iter().all(|p| p.parts.is_none()));
}

#[tokio::test]
async fn parts_reflect_resumed_segments() {
    // 1er run mis en pause après le premier chunk (cf. resume_after_pause_completes_file),
    // puis on réécrit le sidecar avec le segment 0 entièrement terminé :
    //   let mut st = resume::load(&dest).unwrap(); let (s, e, _) = st.segments[0];
    //   st.segments[0] = (s, e, e - s + 1); resume::save(&dest, &st).unwrap();
    // 2e run : le PREMIER message Downloading reçu a parts[0] == 1.0.
}
```

Si `FakeFetcher::new(_, false)` renvoie quand même une taille, adapter `no_parts_for_unknown_size` pour qu'il produise un seul segment : l'assertion `parts.is_none()` reste la même, puisqu'un seul segment donne aussi `None`.

- [ ] **Step 2 : Lancer les tests et vérifier qu'ils échouent**

Run : `cargo test --manifest-path src-tauri/Cargo.toml -p download-core parts` → Expected : erreur de compilation `no field 'parts'`.

- [ ] **Step 3 : Implémenter**

- `model.rs` : ajouter `#[serde(default)] pub parts: Option<Vec<f32>>` à `Progress`.
- `engine.rs` : avant la boucle des segments, construire un `Arc<Vec<(u64, Arc<AtomicU64>)>>` contenant **tous** les segments de `state.segments`, sous la forme (longueur, octets faits), initialisés avec le `done` lu dans le sidecar. Le compteur d'un segment qui tourne est celui-là même qui sert de `seg_progress` (le remplacer, ne pas en créer un second). Changer la signature en `fn emit(tx, id, handle, total, status, parts: Option<&[(u64, Arc<AtomicU64>)]>)`. `parts` vaut `None` si la liste a moins de 2 segments ; sinon `Some(fraction par segment)`, avec la fraction `min(done, len) / len`.
- Ajouter `parts: None` à tous les autres littéraux `Progress`.
- `src/types.ts` : `parts?: number[] | null;` dans `Progress`.

- [ ] **Step 4 : Lancer les tests et vérifier qu'ils passent**

Run : `cargo test --manifest-path src-tauri/Cargo.toml --workspace` → Expected : tout `ok`, y compris les 3 nouveaux tests.

- [ ] **Step 5 : Commit**

```bash
git add src-tauri src/types.ts
git commit -m "feat(core): progression par segment (Progress.parts)"
```

---

### Task 3 : Préférences d'interface persistées

**Files :**
- Modify : `src-tauri/crates/download-core/src/storage.rs` (méthodes + tests)
- Create : `src-tauri/src/prefs.rs`
- Modify : `src-tauri/src/lib.rs` (`mod prefs;` + enregistrement des 2 commandes)
- Modify : `src/api.ts`

**Interfaces :**
- Produces (Rust) : `Storage::get_pref(&self, key: &str) -> Result<Option<String>, CoreError>`, `Storage::set_pref(&self, key: &str, value: &str) -> Result<(), CoreError>` (table `settings` existante, même `INSERT … ON CONFLICT` que les autres clés).
- Produces (commandes) : `get_ui_pref(key: String) -> Result<Option<String>, String>`, `set_ui_pref(key: String, value: String) -> Result<(), String>`. Clés autorisées : `const UI_PREFS: &[(&str, &[&str])] = &[("ui.theme", &["dark", "light", "system"])];`. Toute autre clé, ou valeur hors liste, renvoie `Err`. La validation vit dans `fn validate(key: &str, value: Option<&str>) -> Result<(), String>`.
- Produces (TS) : `getUiPref(key: string): Promise<string | null>`, `setUiPref(key: string, value: string): Promise<void>`.

- [ ] **Step 1 : Écrire les tests qui échouent**

Dans `storage.rs` :
```rust
#[test]
fn pref_roundtrip_and_absent() {
    let s = Storage::open_in_memory().unwrap();
    assert_eq!(s.get_pref("ui.theme").unwrap(), None);
    s.set_pref("ui.theme", "light").unwrap();
    s.set_pref("ui.theme", "system").unwrap();
    assert_eq!(s.get_pref("ui.theme").unwrap().as_deref(), Some("system"));
}
```

Dans `prefs.rs` :
```rust
#[test] fn accepts_known_key_and_value() { assert!(validate("ui.theme", Some("light")).is_ok()); assert!(validate("ui.theme", None).is_ok()); }
#[test] fn rejects_unknown_key() { assert!(validate("bridge.token", None).is_err()); }
#[test] fn rejects_unknown_value() { assert!(validate("ui.theme", Some("rose")).is_err()); }
```

- [ ] **Step 2 : Vérifier l'échec**

Run : `cargo test --manifest-path src-tauri/Cargo.toml --workspace pref` → Expected : erreurs de compilation (méthodes et `validate` absentes).

- [ ] **Step 3 : Implémenter** les méthodes, `validate`, les deux commandes (`State<AppState>` → `state.storage`), leur enregistrement dans `generate_handler!`, et les deux fonctions de `api.ts` (`invoke("get_ui_pref", { key })`, `invoke("set_ui_pref", { key, value })`).

- [ ] **Step 4 : Vérifier**

Run : `cargo test --manifest-path src-tauri/Cargo.toml --workspace` → Expected : tout `ok`.

- [ ] **Step 5 : Commit**

```bash
git add src-tauri src/api.ts
git commit -m "feat: préférences d'interface persistées (ui.theme)"
```

---

### Task 4 : Thèmes sombre / clair / système

**Files :**
- Create : `src/theme/theme.ts`, `src/theme/ThemeProvider.tsx`, `src/theme/theme.test.tsx`
- Modify : `src/index.css`, `tailwind.config.js`, `index.html`, `src/main.tsx`, `src/components/SettingsDialog.tsx`

**Interfaces :**
- Consumes : `getUiPref` / `setUiPref` (Task 3).
- Produces :
  - `type ThemePref = "dark" | "light" | "system"`, `type Theme = "dark" | "light"`
  - `resolveTheme(pref: ThemePref, systemDark: boolean): Theme`
  - `applyTheme(theme: Theme): void` : `document.documentElement.dataset.theme = theme` et `style.colorScheme = theme`.
  - `<ThemeProvider>` et `useTheme(): { pref: ThemePref; theme: Theme; setPref(p: ThemePref): void }`.
  - Clé de cache local : `"flashget.theme"`.
  - Couleurs Tailwind (noms conservés pour que les boîtes de dialogue existantes suivent le thème) : `deep`, `bg`, `panel`, `field`, `surface`, `border`, `line`, `ink`, `ink-soft`, `ink-mute`, `accent` (+ `soft`, `bright`, `ink`), `accent-fg`, `cta`, `cta-ink`, `ok`, `okdim`, `err`, `queued`, sous la forme `rgb(var(--c-<nom>) / <alpha-value>)`.

Valeurs des variables (canaux RVB décimaux), reprises des maquettes A et B :

| Variable | Sombre (A) | Clair (B) |
|---|---|---|
| `--c-deep` (barre de titre, barre d'état) | 6 10 22 | 247 247 245 |
| `--c-bg` (fond) | 11 17 32 | 247 247 245 |
| `--c-panel` (menu, lignes) | 15 23 42 | 255 255 255 |
| `--c-field` (champs, boutons secondaires) | 17 27 46 | 255 255 255 |
| `--c-surface` (pistes, onglet actif) | 30 41 59 | 245 245 244 |
| `--c-border` | 30 41 59 | 231 229 228 |
| `--c-line` (bord des lignes) | 26 36 56 | 236 235 232 |
| `--c-ink` | 226 232 240 | 28 25 23 |
| `--c-ink-soft` | 148 163 184 | 87 83 78 |
| `--c-ink-mute` | 100 116 139 | 168 162 158 |
| `--c-accent` | 245 158 11 | 245 158 11 |
| `--c-accent-fg` (texte accentué : vitesse, menu actif) | 251 191 36 | 180 83 9 |
| `--c-cta` / `--c-cta-ink` (bouton principal) | 245 158 11 / 59 29 0 | 28 25 23 / 255 255 255 |
| `--c-ok` / `--c-err` | 52 211 153 / 248 113 113 | 34 197 94 / 239 68 68 |
| `--c-okdim` / `--c-queued` | 16 185 129 / 71 85 105 | 22 163 74 / 168 162 158 |
| `--c-accent-soft` / `--c-accent-bright` / `--c-accent-ink` | 251 191 36 / 252 211 77 / 59 29 0 | 251 191 36 / 217 119 6 / 59 29 0 |

- [ ] **Step 1 : Écrire les tests qui échouent** (`src/theme/theme.test.tsx`, avec `vi.mock("../api")` et un `matchMedia` simulé)

```ts
it("resolveTheme", () => {
  expect(resolveTheme("dark", false)).toBe("dark");
  expect(resolveTheme("light", true)).toBe("light");
  expect(resolveTheme("system", true)).toBe("dark");
  expect(resolveTheme("system", false)).toBe("light");
});
it("sombre par défaut", () => { render(<ThemeProvider><span /></ThemeProvider>); expect(document.documentElement.dataset.theme).toBe("dark"); });
it("setPref('light') applique et persiste", async () => { /* via un composant qui appelle useTheme().setPref("light") */
  expect(document.documentElement.dataset.theme).toBe("light");
  expect(api.setUiPref).toHaveBeenCalledWith("ui.theme", "light");
  expect(localStorage.getItem("flashget.theme")).toBe("light");
});
it("suit le changement du système en mode system", () => { /* pref system, matchMedia dark=false → light ; déclencher l'événement 'change' avec matches=true → dark */ });
it("la valeur persistée côté Rust l'emporte au démarrage", async () => { /* getUiPref résout "light" → data-theme passe à light */ });
```

- [ ] **Step 2 : Vérifier l'échec** — Run : `npx vitest run src/theme` → Expected : FAIL (module introuvable).

- [ ] **Step 3 : Implémenter**
  - `theme.ts` et `ThemeProvider.tsx` selon les interfaces. Au montage, lire le cache local (`dark` par défaut), puis `getUiPref("ui.theme")` ; si la valeur est valide et différente, l'appliquer. Écouter `matchMedia("(prefers-color-scheme: dark)")` seulement quand `pref === "system"`.
  - `index.css` : définir les variables sous `:root[data-theme="dark"]` et `:root[data-theme="light"]`. `body` utilise `rgb(var(--c-bg))` et `rgb(var(--c-ink))`. Supprimer la règle `:root { color-scheme: dark; }`. `.cta-grad` utilise `--c-cta` et `--c-cta-ink`.
  - `tailwind.config.js` : remplacer chaque couleur codée en dur par sa variable.
  - `index.html` : `<html lang="fr" data-theme="dark">`, `<title>FlashGet</title>`, et un petit script en ligne qui lit `localStorage["flashget.theme"]` et pose `data-theme` avant le premier affichage (`"system"` → via `matchMedia`).
  - `main.tsx` : envelopper `<App />` dans `<ThemeProvider>`.
  - `SettingsDialog.tsx` : en tête, un sélecteur à 3 boutons Sombre / Clair / Système relié à `useTheme()`. Ce sélecteur est provisoire : il déménagera dans Paramètres › Général en phase 5.

- [ ] **Step 4 : Vérifier** — Run : `npm test` → Expected : tout passe.

- [ ] **Step 5 : Commit**

```bash
git add src index.html tailwind.config.js
git commit -m "feat(ui): thèmes sombre, clair et système"
```

---

### Task 5 : Textes de l'interface

**Files :**
- Create : `src/i18n/fr.ts`, `src/i18n/index.ts`, `src/i18n/i18n.test.ts`

**Interfaces :**
- Produces : `export const fr = { … } as const` (dictionnaire plat, clés en `domaine.nom`), `export type MsgKey = keyof typeof fr`, `export function t(key: MsgKey, vars?: Record<string, string | number>): string`, qui remplace chaque `{nom}` par `vars.nom`.
- Les tâches 6 à 9 ajoutent leurs clés dans `fr.ts`. Le texte exact est donné dans chaque tâche.

- [ ] **Step 1 : Test qui échoue**

```ts
it("interpole les variables", () => { expect(t("status.counts", { active: 3, queued: 1 })).toBe("3 actifs · 1 en file"); });
it("renvoie le texte tel quel sans variable", () => { expect(t("nav.downloads")).toBe("Téléchargements"); });
```

- [ ] **Step 2 : Vérifier l'échec** — Run : `npx vitest run src/i18n` → FAIL.
- [ ] **Step 3 : Implémenter** `t()`, avec `fr` qui contient pour l'instant `"nav.downloads": "Téléchargements"` et `"status.counts": "{active} actifs · {queued} en file"`.
- [ ] **Step 4 : Vérifier** — Run : `npx vitest run src/i18n` → PASS.
- [ ] **Step 5 : Commit** — `git add src/i18n && git commit -m "feat(ui): fichier de textes FR et t()"`

---

### Task 6 : Fenêtre sans bordure et barre de titre

**Files :**
- Modify : `src-tauri/tauri.conf.json` (`app.windows[0]`), `src-tauri/capabilities/default.json`
- Create : `src/components/shell/TitleBar.tsx`, `src/components/shell/TitleBar.test.tsx`
- Modify : `src/i18n/fr.ts`

**Interfaces :**
- Produces : `TitleBar(): JSX.Element`, une barre de 36 px avec `data-tauri-drag-region`, le logo (carré ambre avec la flèche de téléchargement, comme l'icône actuelle), « FlashGet » et trois boutons.
- Textes : `"window.minimize": "Réduire"`, `"window.maximize": "Agrandir"`, `"window.close": "Fermer"` (utilisés comme `aria-label` et `title`).

- [ ] **Step 1 : Test qui échoue** — mocker `@tauri-apps/api/window` (`getCurrentWindow` renvoie `{ minimize, toggleMaximize, close }`, des `vi.fn()`). Un clic sur chaque bouton (trouvé par `getByLabelText`) appelle la bonne méthode. `getByText("FlashGet")` est présent.
- [ ] **Step 2 : Vérifier l'échec** — `npx vitest run src/components/shell` → FAIL.
- [ ] **Step 3 : Implémenter**
  - `tauri.conf.json` : `"label": "main"`, `"decorations": false`, `"width": 1200`, `"height": 780`, `"minWidth": 960`, `"minHeight": 640`, `"visible": false`. Le code Rust montre déjà la fenêtre au démarrage, sauf avec `--hidden`, ce qui évite un flash blanc.
  - `capabilities/default.json` : ajouter `"core:window:allow-start-dragging"`, `"core:window:allow-minimize"`, `"core:window:allow-toggle-maximize"`, `"core:window:allow-close"`.
  - `close()` déclenche `CloseRequested`, que `lib.rs` intercepte déjà pour cacher la fenêtre : ne rien changer côté Rust.
- [ ] **Step 4 : Vérifier** — `npm test` → PASS.
- [ ] **Step 5 : Commit** — `git commit -am "feat(ui): fenêtre sans bordure et barre de titre"` (ajouter les nouveaux fichiers avec `git add` avant).

---

### Task 7 : Menu latéral, barre d'état, formats

**Files :**
- Create : `src/navigation.ts`, `src/components/shell/Sidebar.tsx`, `src/components/shell/StatusBar.tsx`, `src/components/shell/shell.test.tsx`
- Modify : `src/format.ts`, `src/format.test.ts`, `src/i18n/fr.ts`

**Interfaces :**
- Produces :
  - `type PageId = "new" | "downloads" | "completed" | "queue" | "scheduler" | "settings"`
  - `interface NavItem { id: PageId; labelKey: MsgKey; icon: LucideIcon; ready: boolean; bottom?: boolean }` et `const NAV_ITEMS: NavItem[]` dans l'ordre du spec ; `ready: false` pour `queue` et `scheduler`.
  - `Sidebar(props: { active: PageId; counts: Partial<Record<PageId, number>>; onNavigate(id: PageId): void })` : n'affiche que les entrées `ready`. « Nouveau » est un bouton plein (couleur `cta`). « Paramètres » est en bas. L'entrée active est sur fond accent pâle, en texte `accent-fg`. Un compteur s'affiche à droite quand `counts[id]` est défini.
  - `StatusBar(props: { active: number; queued: number; maxSpeedBps: number; bridgeListening: boolean; totalSpeedBps: number })`
  - `formatEta(remainingBytes: number, bps: number): string` : `"—"` si `bps <= 0` ou si `remainingBytes` n'est pas un nombre fini ≥ 0 ; `"{s} s"` sous 60 s ; `"{m} min"` sous 1 h ; `"{h} h {mm}"` au-delà (minutes sur 2 chiffres).
  - `hostOf(url: string): string` : hôte sans `www.`, `""` si l'URL est invalide ou n'a pas d'hôte.
- Textes : `"nav.new": "Nouveau"`, `"nav.completed": "Terminés"`, `"nav.queue": "File d'attente"`, `"nav.scheduler": "Planificateur"`, `"nav.settings": "Paramètres"`, `"status.limit.none": "Limite : aucune"`, `"status.limit": "Limite : {speed}"`, `"status.bridge.on": "Extension : prête"`, `"status.bridge.off": "Extension : inactive"`, `"status.speed": "↓ {speed}"`.

- [ ] **Step 1 : Tests qui échouent**

```ts
// format.test.ts
it("formatEta", () => {
  expect(formatEta(1000, 0)).toBe("—");
  expect(formatEta(Number.NaN, 100)).toBe("—");
  expect(formatEta(45 * 100, 100)).toBe("45 s");
  expect(formatEta(17 * 60 * 100, 100)).toBe("17 min");
  expect(formatEta(3600 * 100 + 5 * 60 * 100, 100)).toBe("1 h 05");
});
it("hostOf", () => {
  expect(hostOf("https://www.youtube.com/watch?v=x")).toBe("youtube.com");
  expect(hostOf("https://releases.ubuntu.com/a.iso")).toBe("releases.ubuntu.com");
  expect(hostOf("file:///C:/a.mp4")).toBe("");
  expect(hostOf("pas une url")).toBe("");
});
// shell.test.tsx
it("le menu suit l'ordre du spec et masque les pages non prêtes", () => { /* textes visibles dans l'ordre : Nouveau, Téléchargements, Terminés, Paramètres ; pas de "File d'attente" */ });
it("cliquer une entrée appelle onNavigate", () => { /* clic "Terminés" → onNavigate("completed") */ });
it("affiche les compteurs", () => { /* counts={{downloads: 6, completed: 128}} → "6" et "128" visibles */ });
it("barre d'état", () => { /* active=3 queued=1 maxSpeedBps=0 bridgeListening totalSpeedBps=14260633 → "3 actifs · 1 en file", "Limite : aucune", "Extension : prête", "↓ 13,6 Mo/s" */ });
```

- [ ] **Step 2 : Vérifier l'échec** — `npx vitest run src/format.test.ts src/components/shell` → FAIL.
- [ ] **Step 3 : Implémenter** les fonctions et composants, avec les icônes lucide déjà présentes dans le projet : `Plus`, `List`, `CircleCheck`, `Clock`, `CalendarDays`, `Settings`.
- [ ] **Step 4 : Vérifier** — `npm test` → PASS.
- [ ] **Step 5 : Commit** — `git add src && git commit -m "feat(ui): menu latéral, barre d'état, formatEta/hostOf"`

---

### Task 8 : Ligne de téléchargement

**Files :**
- Create : `src/components/downloads/SegmentBar.tsx`, `src/components/downloads/DownloadCard.tsx`, `src/components/downloads/DownloadCard.test.tsx`
- Move : `src/components/RowMenu.tsx` → `src/components/downloads/RowMenu.tsx` (étendu)
- Modify : `src/i18n/fr.ts`

**Interfaces :**
- Consumes : `Progress.parts` (Task 2), `formatEta` / `hostOf` / `formatBytes` / `formatSpeed` (Task 7), `CategoryIcon` (existant).
- Produces :
  - `type BarTone = "active" | "paused" | "error" | "done" | "queued" | "live"`
  - `SegmentBar(props: { parts?: number[] | null; percent: number; tone: BarTone; indeterminate?: boolean })` : si `parts` contient au moins 2 éléments, une cellule par segment (`data-testid="segment"`), remplie à `part × 100 %`. Sinon une barre unique à `percent`. Si `indeterminate`, une barre animée.
  - `DownloadCard(props: { d: Download; speed: number; parts?: number[] | null; seg?: { done: number; total: number }; queuePosition: number | null; onPause(): void; onResume(): void; onCancel(): void; onStop(): void; onPriority(p: Priority): void; onOpenFile(): void; onOpenFolder(): void; onRemove(): void })`
  - `RowMenu(props: { canOpen: boolean; priority: Priority; onOpenFile(): void; onOpenFolder(): void; onPriority(p: Priority): void; onCancel(): void; onRemove(): void })`
- Structure de `DownloadCard` (maquette 01) : grille `vignette 76×46 | nom + méta | progression 210 px | actions`.
  - Vignette : tuile avec `CategoryIcon` (les vraies vignettes arrivent en phase 2), avec l'étiquette `LIVE` si l'élément est en `recording`.
  - Nom : une ligne, tronqué (`truncate`).
  - Méta : un badge (`HLS`, `DASH`, `yt-dlp`, ou la catégorie), `hostOf(d.url)`, puis selon l'état : `en file d'attente (n° {n})`, `{n} connexions` (si `parts`), `Enregistrement en direct`, ou le message d'erreur.
  - Progression : `SegmentBar` + ligne en police mono, à gauche `{fait} / {total}` (ou `{durée} · {taille}` pour un direct), à droite `{vitesse} · {eta}` en couleur `accent-fg` pendant le téléchargement, sinon le libellé d'état.
  - Actions : bouton principal (Pause si `downloading`, Arrêter si `recording`, Reprendre si `paused`/`error`/`queued`), dossier, `RowMenu`.
- Le minuteur d'enregistrement (durée d'un direct) et la règle « terminé = 100 % même sans taille » de l'ancien `DownloadRow` sont conservés tels quels.
- Textes : `"dl.queuePos": "en file d'attente (n° {n})"`, `"dl.connections": "{n} connexions"`, `"dl.live": "Enregistrement en direct"`, `"dl.waiting": "en attente"`, `"dl.paused": "en pause"`, `"dl.interrupted": "interrompu"`, `"dl.done": "terminé"`, `"dl.pause": "Mettre en pause"`, `"dl.resume": "Reprendre"`, `"dl.stop": "Arrêter (finalise l'enregistrement)"`, `"dl.openFolder": "Ouvrir le dossier"`, `"dl.openFile": "Ouvrir le fichier"`, `"dl.cancel": "Annuler (efface le fichier)"`, `"dl.remove": "Retirer de la liste"`, `"dl.actions": "Actions"`, `"prio.high": "Priorité haute"`, `"prio.normal": "Priorité normale"`, `"prio.low": "Priorité basse"`.

- [ ] **Step 1 : Tests qui échouent** (`DownloadCard.test.tsx`). Reprendre les 6 cas de `DownloadRow.test.tsx` (LIVE, Arrêter → `onStop`, pas de LIVE une fois terminé, badge `yt-dlp`, terminé sans taille = barre pleine), adaptés aux nouveaux libellés, et ajouter :

```ts
it("une cellule par segment quand parts est fourni", () => {
  renderCard({ status: "downloading", total_bytes: 800, downloaded_bytes: 400, kind: "regular" }, { parts: [1, 0.5, 0, 0.25] });
  const cells = screen.getAllByTestId("segment");
  expect(cells).toHaveLength(4);
  expect((cells[1].firstChild as HTMLElement).style.width).toBe("50%");
  expect(screen.getByText("4 connexions")).toBeTruthy();
});
it("barre indéterminée sans taille", () => {
  renderCard({ status: "downloading", total_bytes: null, kind: "regular" }, { parts: null, speed: 1024 });
  expect(screen.queryAllByTestId("segment")).toHaveLength(0);
  expect(document.body.textContent).not.toMatch(/NaN|Infinity/);
});
it("position dans la file", () => { renderCard({ status: "queued" }, { queuePosition: 2 }); expect(screen.getByText(/n° 2/)).toBeTruthy(); });
it("nom long tronqué", () => { renderCard({ file_name: "x".repeat(300) }); expect(screen.getByText("x".repeat(300)).className).toMatch(/truncate/); });
it("le menu change la priorité", () => { /* ouvrir "Actions", clic "Priorité haute" → onPriority("high") */ });
```

- [ ] **Step 2 : Vérifier l'échec** — `npx vitest run src/components/downloads` → FAIL.
- [ ] **Step 3 : Implémenter** `SegmentBar`, `DownloadCard`, `RowMenu`. Couleurs de la barre : `active` = `accent`, `paused` = `accent-soft`, `error` = `err`, `done` = `ok`, `queued` = `queued`, `live` = `err` pulsé.
- [ ] **Step 4 : Vérifier** — `npm test` → PASS.
- [ ] **Step 5 : Commit** — `git add src && git commit -m "feat(ui): ligne de téléchargement avec barre segmentée"`

---

### Task 9 : Pages Téléchargements et Terminés

**Files :**
- Create : `src/downloads/tabs.ts`, `src/downloads/tabs.test.ts`, `src/pages/DownloadsPage.tsx`, `src/pages/CompletedPage.tsx`, `src/pages/pages.test.tsx`
- Modify : `src/i18n/fr.ts`

**Interfaces :**
- Consumes : `DownloadCard` (Task 8).
- Produces :
  - `type DownloadTab = "all" | "active" | "paused" | "error"`
  - `inTab(d: Download, tab: DownloadTab): boolean` : `active` = `downloading` ou `recording` ; `paused` = `paused` ; `error` = `error` ; `all` = tout ce qui n'est pas `completed`.
  - `tabCounts(list: Download[]): Record<DownloadTab, number>`
  - `visibleDownloads(list: Download[], tab: DownloadTab, query: string): Download[]` : exclut `completed`, filtre par onglet et par nom (sans tenir compte de la casse), et garde l'ordre reçu.
  - `queuePositions(list: Download[]): Record<string, number>` : même règle que l'ancien `DownloadTable` (priorité puis ordre d'arrivée, parmi les `queued`).
  - `DownloadsPage(props: { downloads: Download[]; speeds: Record<string, number>; parts: Record<string, number[] | null>; segments: Record<string, { done: number; total: number }>; onAddLink(): void; onAddBatch(): void; onPauseAll(): void; onResumeAll(): void; actions: CardActions })`, avec `type CardActions = { pause(id: string): void; resume(id: string): void; cancel(id: string): void; stop(id: string): void; priority(id: string, p: Priority): void; openFile(id: string): void; openFolder(id: string): void; remove(id: string): void }`.
  - `CompletedPage(props: { downloads: Download[]; onClear(): void; actions: CardActions })` : page **provisoire** jusqu'à la phase 3. Titre « Terminés », recherche, liste de `DownloadCard` des `completed` (du plus récent au plus ancien, dans l'ordre inverse de la liste), et bouton « Vider la liste » (`clear_completed`, qui retire de la liste sans toucher aux fichiers).
- En-tête de `DownloadsPage` : titre, recherche, bouton « Plusieurs liens » (ouvre l'ajout groupé existant), « Tout suspendre » s'il y a au moins un élément actif (sinon « Tout reprendre »), et « Ajouter un lien » (bouton `cta`).
- Textes : `"dl.title": "Téléchargements"`, `"dl.search": "Rechercher…"`, `"dl.pauseAll": "Tout suspendre"`, `"dl.resumeAll": "Tout reprendre"`, `"dl.addLink": "Ajouter un lien"`, `"dl.addBatch": "Plusieurs liens"`, `"tab.all": "Tous · {n}"`, `"tab.active": "Actifs · {n}"`, `"tab.paused": "En pause · {n}"`, `"tab.error": "Erreurs · {n}"`, `"dl.empty": "Aucun téléchargement en cours."`, `"done.title": "Terminés"`, `"done.clear": "Vider la liste"`, `"done.empty": "Aucun téléchargement terminé."`.

- [ ] **Step 1 : Tests qui échouent**

```ts
// tabs.test.ts — liste (statut : nom) : downloading "Clip A.mp4", recording "Direct.ts", paused "clip b.mp4",
//                error "photos.zip", queued "rapport.pdf", completed "Clip C.mp4"
expect(tabCounts(list)).toEqual({ all: 5, active: 2, paused: 1, error: 1 });
expect(visibleDownloads(list, "all", "").map(d => d.status)).not.toContain("completed");
expect(visibleDownloads(list, "all", "CLIP").map(d => d.file_name)).toEqual(["Clip A.mp4", "clip b.mp4"]);
expect(queuePositions([q("a","low"), q("b","high"), q("c","normal")])).toEqual({ b: 1, c: 2, a: 3 });
// pages.test.tsx
it("onglets avec compteurs, filtrage au clic", () => { /* "Actifs · 2" ; clic → seules 2 lignes */ });
it("Tout suspendre s'il y a un actif, sinon Tout reprendre", () => { /* … */ });
it("Ajouter un lien appelle onAddLink", () => { /* … */ });
it("Terminés liste les terminés, le plus récent en premier, et Vider appelle onClear", () => { /* … */ });
```

- [ ] **Step 2 : Vérifier l'échec** — `npx vitest run src/downloads src/pages` → FAIL.
- [ ] **Step 3 : Implémenter** selon les interfaces et la maquette 01 : onglets en pilules (l'onglet actif sur `surface`), lignes espacées de 8 px.
- [ ] **Step 4 : Vérifier** — `npm test` → PASS.
- [ ] **Step 5 : Commit** — `git add src && git commit -m "feat(ui): pages Téléchargements et Terminés (provisoire)"`

---

### Task 10 : Assemblage de la coque et nettoyage

**Files :**
- Modify : `src/App.tsx`
- Delete : `src/components/Toolbar.tsx`, `CategoryTabs.tsx`, `DownloadTable.tsx`, `DownloadRow.tsx`, `DownloadRow.test.tsx`, `CategoryPill.tsx`, `PriorityPill.tsx`, `src/filter.ts`, `src/filter.test.ts`

**Interfaces :**
- Consumes : tout ce qui précède.
- `App` garde **toute** la logique d'événements actuelle (sondes, `media://variants`, `media://error`, `media://probing`, ajout groupé, minuteries de 60 s, bandeaux d'erreur et d'info) et les boîtes de dialogue existantes (`AddUrlDialog`, `AddBatchDialog`, `HlsVariantDialog`, `SettingsDialog`, `ProbingOverlay`), en ne changeant que la mise en page :

```
<div class="h-screen flex flex-col bg-bg text-ink">
  <TitleBar/>
  <div class="flex flex-1 min-h-0">
    <Sidebar active={page} counts={{downloads: nb non terminés, completed: nb terminés}} onNavigate={go}/>
    <main class="flex-1 flex flex-col min-w-0">{bandeaux}{page courante}<StatusBar …/></main>
  </div>
  {boîtes de dialogue}
</div>
```

- `go("new")` ouvre `AddUrlDialog`, qui sera remplacée par la page Nouveau en phase 2. `go("settings")` ouvre `SettingsDialog`, qui sera remplacée en phase 5. Ces deux entrées ne changent pas `page`. Les autres entrées changent `page` (`"downloads"` par défaut).
- `onProgress` stocke aussi `p.parts` dans un état `parts: Record<string, number[] | null>`.
- `StatusBar` : `active` et `queued` sont calculés sur `downloads`, `totalSpeedBps` est la somme des vitesses des éléments `downloading`/`recording`. `maxSpeedBps` vient de `getSettings()` (au montage et à la fermeture de `SettingsDialog`), et `bridgeListening` de `getBridgeInfo().listening`.

- [ ] **Step 1 : Réécrire `App.tsx`** selon la structure ci-dessus, puis supprimer les fichiers listés.

- [ ] **Step 2 : Vérifier les tests et le typage**

Run : `npm test` → Expected : tout passe.
Run : `npx tsc --noEmit` → Expected : aucune erreur (en particulier aucun import vers un fichier supprimé).
Run : `cargo test --manifest-path src-tauri/Cargo.toml --workspace` → Expected : tout `ok`.

- [ ] **Step 3 : Lancer l'application** — Run : `npm run tauri dev` → la fenêtre s'ouvre sans bordure Windows, avec la nouvelle coque, en thème sombre.

- [ ] **Step 4 : Vérifier un téléchargement multi-connexions** — ajouter un gros fichier qui accepte les plages (un ISO, par exemple) : la ligne montre 8 cellules qui se remplissent à des vitesses différentes, `8 connexions`, puis la vitesse et le temps restant. Mettre en pause puis reprendre : les cellules déjà pleines restent pleines.

- [ ] **Step 5 : Vérifier les capacités conservées** — choix de variante sur un lien YouTube (la boîte existante s'ouvre), arrêt d'un direct HLS, priorité depuis le menu « … », ouvrir le dossier, ajout groupé, réglages existants. Une fois terminé, l'élément disparaît de Téléchargements et apparaît dans Terminés.

- [ ] **Step 6 : Vérifier la fenêtre** — glisser la barre de titre déplace la fenêtre, et un double-clic l'agrandit. Le bouton Fermer **cache** FlashGet dans la zone de notification : les téléchargements continuent, et un clic sur l'icône la rouvre. Réduire et Agrandir fonctionnent.

- [ ] **Step 7 : Vérifier les thèmes** — dans les réglages, passer en Clair : toute la coque **et** les anciennes boîtes de dialogue restent lisibles. Passer en Système, changer le mode de Windows (Paramètres › Personnalisation › Couleurs) : FlashGet suit. Fermer puis rouvrir FlashGet : le choix est conservé, sans flash du mauvais thème.

- [ ] **Step 8 : Commit**

```bash
git add -A src
git commit -m "feat(ui): nouvelle coque FlashGet v2 (phase 1)"
```

- [ ] **Step 9 : Fin de phase** — suivre `superpowers:finishing-a-development-branch` pour fusionner `flashget-v2-phase-1` dans `main`.
