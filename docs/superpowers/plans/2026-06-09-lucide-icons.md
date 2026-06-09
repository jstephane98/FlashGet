# Remplacement des emoji par icônes Lucide — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. Implémentation guidée par `frontend-design`. Steps en cases à cocher.

**Goal:** Remplacer tous les emoji de l'UI (app + extension) par des icônes Lucide cohérentes, sans toucher au comportement ni aux libellés texte.

**Architecture:** `lucide-react` pour l'app (icônes héritant de `currentColor`), SVG inline pour l'extension. Un helper partagé `CategoryIcon` pour la correspondance catégorie→icône (DRY). Boutons icône seule dotés d'`aria-label`.

**Tech Stack :** React 19, Tailwind v3, lucide-react, extension MV3, Vitest.

**Branche :** `phase-icons-lucide` (déjà créée depuis `main`). Garde `git checkout phase-icons-lucide` avant chaque commit.

**Référence spec :** `docs/superpowers/specs/2026-06-09-lucide-icons-design.md`

**Règle commune :** un bouton/élément qui gagne une icône **devant du texte** reçoit `inline-flex items-center gap-1.5` (si pas déjà flex). Les icônes : `size={16}` (14 px pour les boutons de ligne compacts), couleur héritée (`currentColor`). Ne PAS changer les libellés texte, handlers, props. Conserver les `title` existants ; ajouter un `aria-label` aux boutons icône seule.

---

## Task 1 : Dépendance + helper CategoryIcon + Toolbar

**Files:**
- Modify: `package.json` (dep `lucide-react`)
- Create: `src/components/categoryIcons.tsx`
- Modify: `src/components/Toolbar.tsx`

- [ ] **Step 1 : Installer lucide-react**
```bash
npm i lucide-react
```

- [ ] **Step 2 : Créer le helper `src/components/categoryIcons.tsx`**
```tsx
import { LayoutGrid, Film, FileText, MonitorCog, FileArchive, Music, Folder, type LucideIcon } from "lucide-react";
import type { Category } from "../types";

const MAP: Record<Category | "all", LucideIcon> = {
  all: LayoutGrid,
  video: Film,
  document: FileText,
  program: MonitorCog,
  archive: FileArchive,
  music: Music,
  other: Folder,
};

export function CategoryIcon({ cat, size = 16, className }: { cat: Category | "all"; size?: number; className?: string }) {
  const Icon = MAP[cat];
  return <Icon size={size} className={className} />;
}
```
(Vérifier que `Category` dans `src/types.ts` couvre `video|document|program|archive|music|other` ; sinon adapter les clés de `MAP`.)

- [ ] **Step 3 : `Toolbar.tsx` — remplacer les emoji par des icônes**

Lire le fichier. Ajouter l'import : `import { Plus, ListPlus, Pause, Play, Search, Trash2, Settings } from "lucide-react";`. Puis :
- Bouton « Ajouter » : retirer le `+` du texte, mettre `inline-flex items-center gap-1.5`, contenu `<Plus size={16} /> Ajouter`.
- Bouton « Lot » : idem avec `<ListPlus size={16} /> Lot`.
- « Tout » (pause tout) : `inline-flex items-center gap-1.5`, `<Pause size={16} /> Tout`.
- « Tout » (reprendre tout) : `<Play size={16} /> Tout`.
- Champ recherche : retirer 🔍 du `placeholder` (→ `"Rechercher par nom…"`), envelopper l'input dans un conteneur `relative flex-1` et ajouter l'icône :
```tsx
<div className="relative flex-1">
  <Search size={15} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-ink-mute pointer-events-none" />
  <input
    className="w-full bg-bg text-ink placeholder:text-ink-mute border border-border rounded-md pl-8 pr-3 py-1.5 text-sm"
    placeholder="Rechercher par nom…"
    value={query}
    onChange={(e) => onQuery(e.target.value)}
  />
</div>
```
(adapter en conservant les classes existantes du champ + `pl-8` pour la place de l'icône ; l'input perd `flex-1` qui passe sur le conteneur.)
- Bouton vider terminés (🧹) : contenu `<Trash2 size={16} />`, ajouter `aria-label="Vider les terminés"` (garder `title="Vider les terminés"`).
- Bouton réglages (⚙) : contenu `<Settings size={16} />`, ajouter `aria-label="Réglages"` (garder `title="Réglages"`).

- [ ] **Step 4 : Vérifier**
Run: `npx tsc --noEmit` → aucune erreur.
Run: `npm test` → 52 tests verts.

- [ ] **Step 5 : Commit**
```bash
git checkout phase-icons-lucide
git add package.json package-lock.json src/components/categoryIcons.tsx src/components/Toolbar.tsx
git commit -m "feat(ui): icônes Lucide — toolbar + helper CategoryIcon (remplace les emoji)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 2 : Catégories — CategoryTabs + CategoryPill + liste Réglages

**Files:** Modify: `src/components/CategoryTabs.tsx`, `src/components/CategoryPill.tsx`, `src/components/SettingsDialog.tsx` (uniquement le tableau `CATS` + son rendu)

- [ ] **Step 1 — CategoryTabs.tsx :** importer `import { CategoryIcon } from "./categoryIcons";`. Retirer les emoji des labels (`"🎬 Vidéos"`→`"Vidéos"`, etc. ; « Tous » et « Autre » déjà sans emoji). Dans le rendu de chaque onglet, mettre l'icône devant le label : le bouton d'onglet devient `inline-flex items-center gap-1.5` avec `<CategoryIcon cat={t.key} size={15} />` puis `{t.label}`.

- [ ] **Step 2 — CategoryPill.tsx :** importer `CategoryIcon`. Le `LABELS` actuel (`video: "🎬 Vidéos"`, `document: "📄 Docs"`, `program: "⚙ Prog."`, …) : retirer les emoji (→ `"Vidéos"`, `"Docs"`, `"Prog."`, `"Archives"`, `"Musique"`). Rendre `<span class="inline-flex items-center gap-1"><CategoryIcon cat={cat} size={13} /> {label}</span>` (cette pill est compacte → size 13).

- [ ] **Step 3 — SettingsDialog.tsx (liste catégories seulement) :** le tableau `CATS` en tête (`{ key: "video", label: "🎬 Vidéos" }`, …) : retirer les emoji des labels. Dans la boucle qui rend chaque ligne de catégorie (`{CATS.map((c) => ...)}`), remplacer l'affichage du label par `<span className="inline-flex items-center gap-1.5"><CategoryIcon cat={c.key} size={15} /> {c.label}</span>`. (Le reste du SettingsDialog est traité en Task 4.)

- [ ] **Step 4 : Vérifier** — `npx tsc --noEmit` + `npm test` verts.

- [ ] **Step 5 : Commit**
```bash
git checkout phase-icons-lucide
git add src/components/CategoryTabs.tsx src/components/CategoryPill.tsx src/components/SettingsDialog.tsx
git commit -m "feat(ui): icônes de catégorie Lucide (onglets, pills, réglages)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3 : Ligne de téléchargement + menu

**Files:** Modify: `src/components/DownloadRow.tsx`, `src/components/RowMenu.tsx`

- [ ] **Step 1 — DownloadRow.tsx :** importer `import { Square, Pause, Play, X } from "lucide-react";`. Dans la zone actions (boutons) :
  - stop (⏹, `onStop`) : contenu `<Square size={14} />`, garder `title="Arrêter (finalise l'enregistrement)"`, ajouter `aria-label="Arrêter l'enregistrement"`.
  - pause (⏸, `onPause`) : `<Pause size={14} />`, ajouter `aria-label="Mettre en pause"` + `title="Mettre en pause"`.
  - reprendre (▶, `onResume`) : `<Play size={14} />`, ajouter `aria-label="Reprendre"` + `title="Reprendre"`.
  - annuler (✕, `onCancel`) : `<X size={14} />`, garder `title="Annuler (efface le fichier)"`, ajouter `aria-label="Annuler"`.
  Garder les classes `px-2 py-1 rounded bg-surface text-ink-soft hover:text-ink` (ajouter `inline-flex items-center justify-center` si besoin de centrer l'icône).

- [ ] **Step 2 — RowMenu.tsx :** importer `import { MoreHorizontal, File, FolderOpen, Trash2 } from "lucide-react";`.
  - Bouton déclencheur (⋯) : contenu `<MoreHorizontal size={16} />`, garder `title="Actions"`, ajouter `aria-label="Actions"`.
  - Item « 📂 Ouvrir le fichier » : `<span class="inline-flex items-center gap-2"><File size={15} /> Ouvrir le fichier</span>`.
  - Item « 🗂 Ouvrir le dossier » : `<FolderOpen size={15} /> Ouvrir le dossier`.
  - Item « 🗑 Retirer de la liste » : `<Trash2 size={15} /> Retirer de la liste`.

- [ ] **Step 3 : Vérifier** — `npx tsc --noEmit` + `npm test`. Le test `DownloadRow.test.tsx` cible le bouton stop par son `title` (« Arrêter (finalise l'enregistrement) ») conservé → doit rester vert. Si un test casse, cibler le `aria-label`/`title`. Re-run jusqu'au vert.

- [ ] **Step 4 : Commit**
```bash
git checkout phase-icons-lucide
git add src/components/DownloadRow.tsx src/components/RowMenu.tsx
git commit -m "feat(ui): icônes Lucide sur les actions de ligne + menu (a11y aria-label)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4 : Réglages (reste) + bannières App + HlsVariantDialog

**Files:** Modify: `src/components/SettingsDialog.tsx`, `src/App.tsx`, `src/components/HlsVariantDialog.tsx` (+ `HlsVariantDialog.test.tsx` si markup change)

- [ ] **Step 1 — SettingsDialog.tsx :** importer `import { Settings, Puzzle, X } from "lucide-react";`.
  - Titre « ⚙ Réglages » → `<h3 ...><span className="inline-flex items-center gap-2"><Settings size={18} /> Réglages</span></h3>` (garder les classes du `h3`).
  - « 🧩 Extension navigateur » → `<div ...><span className="inline-flex items-center gap-2"><Puzzle size={16} /> Extension navigateur</span></div>`.
  - Bouton ✕ d'effacement de dossier de catégorie → `<X size={14} />`, ajouter `aria-label="Retirer le dossier"`.

- [ ] **Step 2 — App.tsx :** importer `import { X } from "lucide-react";`. Les deux bannières (erreur + info) ont un bouton ✕ : remplacer le texte `✕` par `<X size={14} />` et ajouter `aria-label="Fermer"` à chacun (garder leurs classes et `onClick`).

- [ ] **Step 3 — HlsVariantDialog.tsx :** lire le fichier ; remplacer tout emoji présent par l'icône Lucide adéquate (`X` pour une fermeture, `Check` pour une sélection/coche). Importer ce qui est utilisé. Conserver les libellés/`aria-label` (« Nom du fichier », « Télécharger »).

- [ ] **Step 4 : Vérifier** — `npx tsc --noEmit` + `npm test` (mettre à jour `HlsVariantDialog.test.tsx` seulement s'il ciblait un emoji/markup changé ; conserver les assertions de libellé).

- [ ] **Step 5 : Commit**
```bash
git checkout phase-icons-lucide
git add src/components/SettingsDialog.tsx src/App.tsx src/components/HlsVariantDialog.tsx src/components/HlsVariantDialog.test.tsx
git commit -m "feat(ui): icônes Lucide réglages + bannières + dialogue HLS

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5 : Extension — SVG inline dans le popup

**Files:** Modify: `extension/popup.html` (+ `extension/ui.css` pour le dimensionnement si besoin)

**IMPORTANT :** ne pas changer les `id` ni les `<script>`. On remplace seulement les glyphes ⬇/↓.

- [ ] **Step 1 — popup.html :** définir l'icône Lucide `Download` en SVG inline :
```html
<svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><path d="m7 10 5 5 5-5"/><path d="M12 15V3"/></svg>
```
  - Dans la pastille logo (`.logo`) : remplacer le texte `↓` par ce SVG (couleur héritée = `--accent-ink`, ok).
  - Dans le bouton `#downloadVideo` : retirer le `⬇` du texte et placer le SVG avant « Télécharger la vidéo de cette page » (le bouton `.fg-cta` est déjà `display:flex; gap:8px`).

- [ ] **Step 2 — ui.css :** ajouter une règle de taille pour les SVG inline :
```css
.ic { width: 18px; height: 18px; flex: none; }
.logo .ic { width: 16px; height: 16px; }
```

- [ ] **Step 3 : Vérifier** — recharger l'extension non empaquetée dans Chrome (`chrome://extensions`), ouvrir le popup : logo + bouton vidéo affichent l'icône Lucide (et plus l'emoji) ; confirmer que les `id` n'ont pas bougé (le JS fonctionne).

- [ ] **Step 4 : Commit**
```bash
git checkout phase-icons-lucide
git add extension/popup.html extension/ui.css
git commit -m "feat(ui): icône Lucide (SVG inline) dans le popup de l'extension

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6 : Vérification finale + terminaison

**Files:** aucun (vérif).

- [ ] **Step 1 :** `npx tsc --noEmit` ; `npm test` (toute la suite verte) ; grep anti-emoji résiduel :
```bash
grep -rnE "⏸|▶|✕|🧹|⚙|🎬|📄|🗜|🎵|📂|🗂|🗑|⬇|↓|🔍|🧩|⏹|⋯" src/components src/App.tsx extension/popup.html | grep -v ".test." || echo "AUCUN emoji résiduel"
```
Expected : aucun emoji résiduel (hors tests éventuels).
- [ ] **Step 2 :** `npm run tauri dev` — smoke visuel : toolbar, onglets, lignes, menu, réglages, dialogues affichent des icônes Lucide cohérentes (couleur ink/ambre, pas d'emoji). Recharger l'extension → popup OK.
- [ ] **Step 3 (optionnel) :** `npm run tauri build` pour un installateur à jour.
- [ ] **Step 4 :** Terminer via `superpowers:finishing-a-development-branch` (merge `--no-ff` dans `main`).

---

## Auto-revue (couverture spec)

- lucide-react (app) + SVG inline (extension) → Task 1 / Task 5. ✓
- Toolbar (Plus, ListPlus, Pause, Play, Search, Trash2, Settings) → Task 1. ✓
- Catégories (LayoutGrid/Film/FileText/MonitorCog/FileArchive/Music/Folder) via helper partagé → Task 1 (helper) + Task 2 (consommateurs). ✓
- Ligne (Square/Pause/Play/X) + menu (MoreHorizontal/File/FolderOpen/Trash2) → Task 3. ✓
- Réglages (Settings/Puzzle/X) + bannières (X) + HlsVariantDialog → Task 4. ✓
- Extension popup (Download SVG, logo + CTA) → Task 5. ✓
- `aria-label` sur boutons icône seule → Tasks 1,3,4. ✓
- Comportement/libellés inchangés ; tests conservés/mis à jour si markup → Steps « npm test » + grep. ✓
- Hors périmètre (icône app, libellés, animations) → respecté. ✓
