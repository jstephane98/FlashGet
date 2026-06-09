# Refonte UI « Cockpit sombre » — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans. L'implémentation applique la skill `frontend-design` via les tokens figés ci-dessous. Steps en cases à cocher.

**Goal:** Re-habiller l'app (React/Tailwind) et l'extension (HTML/CSS) dans la direction « Cockpit sombre » (fond ardoise, accent ambre, Archivo + JetBrains Mono, densité confortable), sans toucher au comportement.

**Architecture:** Poser un design system (polices auto-hébergées + thème Tailwind + variables) puis ré-habiller chaque surface en mappant les classes claires actuelles vers les tokens sombres. Aucune logique modifiée ; libellés/`aria-label` conservés pour ne pas casser les tests Vitest.

**Tech Stack :** React 19, Tailwind v3, Vite, extension MV3, Vitest. Polices Archivo + JetBrains Mono (woff2 locaux).

**Branche :** `phase-redesign` (déjà créée depuis `main`). Garde `git checkout phase-redesign` avant chaque commit.

**Référence spec :** `docs/superpowers/specs/2026-06-09-ui-redesign-cockpit-design.md`

**Tokens (rappel, source de vérité = spec) :** fond `#0f172a` / profond `#020617` / surface `#1e293b` / bordure `#334155` ; texte `#e2e8f0` / `#94a3b8` / `#64748b` ; accent `#f59e0b` (soft `#fbbf24`, bright `#fcd34d`, texte-sur-accent `#3b1d00`, dégradé `linear-gradient(160deg,#fbbf24,#f59e0b)`) ; statuts : en cours = accent, terminé/connecté = `#34d399`, en file = `#475569`, pause = `#fbbf24`, erreur = `#f87171`.

**Mapping de classes (règle générale appliquée partout) :**
- `bg-white`→`bg-surface` (panneaux) ou `bg-bg` (fond) ; `bg-gray-100`→`bg-surface` ; `bg-gray-50`→`bg-bg`.
- `text-gray-900/800/700`→`text-ink` ; `text-gray-600/500`→`text-ink-soft` ; `text-gray-400`→`text-ink-mute`.
- `border-gray-200/100`→`border-border`.
- `bg-indigo-600 text-white` (CTA primaire)→`bg-accent text-accent-ink` (+ dégradé pour les gros CTA, cf. `.cta-grad`).
- Valeurs numériques (débit, taille, %, port, jeton)→ ajouter `font-mono`.
- bannière erreur : fonds rouges→`bg-err/10 border-err/40 text-err` ; bannière info : `bg-accent/10 border-accent/40 text-accent-bright`.

**Note non-régression :** ne PAS modifier les textes visibles ni les `aria-label` (tests `DownloadRow.test`, `AddBatchDialog.test`, `HlsVariantDialog.test`). Si un changement de markup casse un test de présentation, mettre à jour le test dans la même tâche.

---

## Task 1 : Fondation — polices auto-hébergées + thème Tailwind + base sombre

**Files:**
- Create: `src/assets/fonts/*.woff2`, `extension/fonts/*.woff2`
- Modify: `tailwind.config.js`, `src/index.css`, `package.json` (devDep fonts)

- [ ] **Step 1 : Récupérer les woff2 (Archivo 500/700/800, JetBrains Mono 500/700)**

```bash
npm i -D @fontsource/archivo @fontsource/jetbrains-mono
mkdir -p src/assets/fonts extension/fonts
# Repérer les fichiers latin normal puis copier+renommer (les noms fontsource sont du type archivo-latin-500-normal.woff2)
cp node_modules/@fontsource/archivo/files/archivo-latin-500-normal.woff2 src/assets/fonts/archivo-500.woff2
cp node_modules/@fontsource/archivo/files/archivo-latin-700-normal.woff2 src/assets/fonts/archivo-700.woff2
cp node_modules/@fontsource/archivo/files/archivo-latin-800-normal.woff2 src/assets/fonts/archivo-800.woff2
cp node_modules/@fontsource/jetbrains-mono/files/jetbrains-mono-latin-500-normal.woff2 src/assets/fonts/jetbrains-mono-500.woff2
cp node_modules/@fontsource/jetbrains-mono/files/jetbrains-mono-latin-700-normal.woff2 src/assets/fonts/jetbrains-mono-700.woff2
cp src/assets/fonts/*.woff2 extension/fonts/
```
Si un nom de fichier diffère, lister `ls node_modules/@fontsource/archivo/files/ | grep latin-500` et adapter. Vérifier : `ls src/assets/fonts extension/fonts` → 5 woff2 dans chaque.

- [ ] **Step 2 : `tailwind.config.js` — thème**

Remplacer le contenu par :
```js
/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bg: "#0f172a",
        deep: "#020617",
        surface: "#1e293b",
        border: "#334155",
        ink: "#e2e8f0",
        "ink-soft": "#94a3b8",
        "ink-mute": "#64748b",
        accent: {
          DEFAULT: "#f59e0b",
          soft: "#fbbf24",
          bright: "#fcd34d",
          ink: "#3b1d00",
        },
        ok: "#34d399",
        okdim: "#10b981",
        err: "#f87171",
        queued: "#475569",
      },
      fontFamily: {
        sans: ["Archivo", "ui-sans-serif", "system-ui", "sans-serif"],
        mono: ['"JetBrains Mono"', "ui-monospace", "monospace"],
      },
    },
  },
  plugins: [],
};
```

- [ ] **Step 3 : `src/index.css` — @font-face + base sombre + util CTA**

Remplacer le contenu par :
```css
@tailwind base;
@tailwind components;
@tailwind utilities;

@font-face { font-family: "Archivo"; font-weight: 500; font-style: normal; font-display: swap; src: url("./assets/fonts/archivo-500.woff2") format("woff2"); }
@font-face { font-family: "Archivo"; font-weight: 700; font-style: normal; font-display: swap; src: url("./assets/fonts/archivo-700.woff2") format("woff2"); }
@font-face { font-family: "Archivo"; font-weight: 800; font-style: normal; font-display: swap; src: url("./assets/fonts/archivo-800.woff2") format("woff2"); }
@font-face { font-family: "JetBrains Mono"; font-weight: 500; font-style: normal; font-display: swap; src: url("./assets/fonts/jetbrains-mono-500.woff2") format("woff2"); }
@font-face { font-family: "JetBrains Mono"; font-weight: 700; font-style: normal; font-display: swap; src: url("./assets/fonts/jetbrains-mono-700.woff2") format("woff2"); }

:root { color-scheme: dark; }
html, body, #root { height: 100%; }
body { margin: 0; font-family: "Archivo", ui-sans-serif, system-ui, sans-serif; background: #0f172a; color: #e2e8f0; }

@layer components {
  .cta-grad { background-image: linear-gradient(160deg, #fbbf24, #f59e0b); color: #3b1d00; }
}
```

- [ ] **Step 4 : Vérifier build + tests inchangés**

Run: `npx tsc --noEmit` → aucune erreur.
Run: `npm test` → 45 tests toujours verts (aucun composant modifié ; seuls thème/polices changent).
Run: `npm run build` → `tsc && vite build` OK (les woff2 sont bien résolus par Vite).

- [ ] **Step 5 : Commit**
```bash
git checkout phase-redesign
git add src/assets/fonts extension/fonts tailwind.config.js src/index.css package.json package-lock.json
git commit -m "feat(ui): fondation cockpit — polices Archivo/JetBrains Mono + thème Tailwind sombre + accent ambre"
```

---

## Task 2 : Coquille app — App.tsx + Toolbar + CategoryTabs/CategoryPill

**Files:** Modify: `src/App.tsx`, `src/components/Toolbar.tsx`, `src/components/CategoryTabs.tsx`, `src/components/CategoryPill.tsx`

- [ ] **Step 1 :** Lire les 4 fichiers en entier. Appliquer le **mapping de classes** (en-tête du plan) à chaque `className`.
- [ ] **Step 2 — App.tsx :** conteneur racine `bg-bg text-ink` ; bandeau erreur → `bg-err/10 border-b border-err/40 text-err` (bouton ✕ `bg-err/20 text-err`) ; bandeau info → `bg-accent/10 border-b border-accent/40 text-accent-bright` (✕ `bg-accent/20 text-accent-bright`).
- [ ] **Step 3 — Toolbar.tsx :** barre `bg-surface border-b border-border` ; bouton « + Ajouter » → `cta-grad font-semibold` (rayon `rounded-md`) ; « + Lot » → `bg-bg text-ink border border-border` ; « ⏸ Tout »/« ▶ Tout » → `bg-bg text-ink-soft border border-border` ; recherche `bg-bg text-ink placeholder:text-ink-mute border border-border` ; `<select>` `bg-bg text-ink border border-border` ; boutons 🧹/⚙ `bg-bg text-ink-soft border border-border`.
- [ ] **Step 4 — CategoryTabs.tsx / CategoryPill.tsx :** fond `bg-bg` ; onglet inactif `text-ink-soft` ; onglet actif `text-accent-bright` + soulignement/bordure `border-accent` ; pills `bg-surface text-ink-soft border border-border`.
- [ ] **Step 5 :** `npx tsc --noEmit` OK ; `npm test` OK (libellés inchangés). Vérif visuelle dev (barre + onglets sombres, CTA ambre).
- [ ] **Step 6 : Commit**
```bash
git checkout phase-redesign
git add src/App.tsx src/components/Toolbar.tsx src/components/CategoryTabs.tsx src/components/CategoryPill.tsx
git commit -m "feat(ui): coquille app cockpit (layout, toolbar CTA ambre, onglets)"
```

---

## Task 3 : Liste des téléchargements — DownloadTable + DownloadRow + PriorityPill + RowMenu (cœur)

**Files:** Modify: `src/components/DownloadTable.tsx`, `src/components/DownloadRow.tsx`, `src/components/PriorityPill.tsx`, `src/components/RowMenu.tsx`, `src/components/DownloadRow.test.tsx` (si markup change)

- [ ] **Step 1 :** Lire les 4 composants + `DownloadRow.test.tsx` en entier. Repérer les libellés assertés par le test (à conserver tels quels).
- [ ] **Step 2 — DownloadTable.tsx :** en-tête de table/colonnes `text-ink-mute` ; fond `bg-bg` ; séparateurs `divide-border`/`border-border`.
- [ ] **Step 3 — DownloadRow.tsx (densité confortable + statuts + mono) :**
  - Ligne : `px-4 py-3` (confortable), hover `hover:bg-surface/60`, bordure basse `border-b border-border`.
  - Nom de fichier : `text-ink` (Archivo par défaut). **Valeurs numériques** (vitesse, taille téléchargée/totale, %) : ajouter `font-mono text-ink-soft`, et la vitesse active en `text-accent-bright`.
  - Pastille de statut (point) selon statut : en cours `bg-accent shadow-[0_0_8px_#f59e0b]` ; terminé `bg-ok` ; en file `bg-queued` ; pause `bg-accent-soft` ; erreur `bg-err` ; enregistrement `bg-err animate-pulse`.
  - Barre de progression : piste `bg-surface` ; remplissage selon statut (en cours `bg-accent`, terminé `bg-okdim`, pause `bg-accent-soft`, file `bg-queued`), transition `transition-[width] duration-300`.
  - Conserver les **libellés de statut** et textes existants (ex. « Terminé », « En pause », « En file », « Erreur ») exactement.
- [ ] **Step 4 — PriorityPill.tsx / RowMenu.tsx :** pills `bg-surface text-ink-soft border border-border` (accent pour « Haute » : `text-accent-bright border-accent/50`) ; menu `bg-surface border border-border text-ink shadow-[0_20px_50px_rgba(0,0,0,.5)]`, items hover `hover:bg-bg`.
- [ ] **Step 5 :** `npm test`. Si `DownloadRow.test.tsx` casse parce qu'il ciblait une classe/structure changée, le mettre à jour pour cibler le **libellé/`aria-label`** (comportement identique). Re-run jusqu'au vert. `npx tsc --noEmit` OK.
- [ ] **Step 6 :** Vérif visuelle dev (lignes confortables, chiffres mono, barres colorées par statut).
- [ ] **Step 7 : Commit**
```bash
git checkout phase-redesign
git add src/components/DownloadTable.tsx src/components/DownloadRow.tsx src/components/PriorityPill.tsx src/components/RowMenu.tsx src/components/DownloadRow.test.tsx
git commit -m "feat(ui): liste de téléchargements cockpit (densité confortable, chiffres mono, statuts colorés)"
```

---

## Task 4 : Dialogues — AddUrlDialog + AddBatchDialog + SettingsDialog + HlsVariantDialog

**Files:** Modify: `src/components/AddUrlDialog.tsx`, `src/components/AddBatchDialog.tsx`, `src/components/SettingsDialog.tsx`, `src/components/HlsVariantDialog.tsx` (+ leurs `.test.tsx` si markup change)

- [ ] **Step 1 :** Lire les 4 dialogues + `AddBatchDialog.test.tsx` + `HlsVariantDialog.test.tsx`. Conserver tous les textes/`aria-label` (« Liens à télécharger », « Nom du fichier », « Ajouter », « Télécharger », « Enregistrer »…).
- [ ] **Step 2 — règle commune dialogues :** backdrop `bg-deep/80` ; panneau `bg-bg border border-border rounded-xl shadow-[0_20px_50px_rgba(0,0,0,.5)] text-ink` ; titres `text-ink font-bold` ; champs/textarea `bg-surface text-ink placeholder:text-ink-mute border border-border rounded-md` ; bouton « Annuler » `bg-surface text-ink-soft border border-border` ; bouton primaire (Ajouter/Télécharger/Enregistrer) `cta-grad font-semibold`.
- [ ] **Step 3 — SettingsDialog.tsx :** sections `border-border` ; libellés `text-ink-soft` ; la case « Démarrer avec Windows » et la section « 🧩 Extension navigateur » suivent le thème ; `code`/jeton en `font-mono bg-surface text-ink`.
- [ ] **Step 4 — HlsVariantDialog.tsx :** liste de variantes en `bg-surface`/`border-border`, variante sélectionnée `border-accent`/`text-accent-bright` ; tailles estimées en `font-mono`.
- [ ] **Step 5 :** `npm test` (mettre à jour un test seulement s'il ciblait une classe/structure ; garder les assertions de libellé). `npx tsc --noEmit` OK. Vérif visuelle des 4 dialogues.
- [ ] **Step 6 : Commit**
```bash
git checkout phase-redesign
git add src/components/AddUrlDialog.tsx src/components/AddBatchDialog.tsx src/components/SettingsDialog.tsx src/components/HlsVariantDialog.tsx src/components/AddBatchDialog.test.tsx src/components/HlsVariantDialog.test.tsx
git commit -m "feat(ui): dialogues cockpit (panneaux sombres, champs, CTA ambre)"
```

---

## Task 5 : Extension — CSS partagé + popup + options

**Files:** Create: `extension/ui.css` ; Modify: `extension/popup.html`, `extension/options.html`

**IMPORTANT :** ne PAS changer les `id` des éléments ni les `<script src="popup.js"/"options.js">` (référencés par le JS). On change uniquement le `<style>`/markup de présentation.

- [ ] **Step 1 — `extension/ui.css` :** tokens cockpit + `@font-face` pointant `fonts/*.woff2` :
```css
@font-face { font-family:"Archivo"; font-weight:500; src:url("fonts/archivo-500.woff2") format("woff2"); }
@font-face { font-family:"Archivo"; font-weight:700; src:url("fonts/archivo-700.woff2") format("woff2"); }
@font-face { font-family:"Archivo"; font-weight:800; src:url("fonts/archivo-800.woff2") format("woff2"); }
@font-face { font-family:"JetBrains Mono"; font-weight:500; src:url("fonts/jetbrains-mono-500.woff2") format("woff2"); }
:root{ --bg:#0f172a; --surface:#1e293b; --border:#334155; --ink:#e2e8f0; --ink-soft:#94a3b8; --ink-mute:#64748b; --accent:#f59e0b; --accent-soft:#fbbf24; --accent-ink:#3b1d00; --ok:#34d399; }
body{ font-family:"Archivo",system-ui,sans-serif; background:var(--bg); color:var(--ink); }
.fg-cta{ background-image:linear-gradient(160deg,var(--accent-soft),var(--accent)); color:var(--accent-ink); font-weight:800; border:none; border-radius:11px; padding:13px; width:100%; cursor:pointer; box-shadow:0 6px 18px rgba(245,158,11,.35); }
.fg-input{ width:100%; box-sizing:border-box; background:var(--surface); border:1px solid var(--border); border-radius:9px; padding:10px 11px; color:var(--ink); }
.fg-btn{ background:var(--surface); border:1px solid var(--border); color:var(--ink); border-radius:9px; padding:8px 12px; cursor:pointer; }
.fg-mono{ font-family:"JetBrains Mono",monospace; }
.fg-muted{ color:var(--ink-soft); font-size:12px; }
hr{ border:none; border-top:1px solid var(--border); margin:14px 0; }
.dot{ width:8px; height:8px; border-radius:50%; } .on{ background:var(--ok); box-shadow:0 0 7px var(--ok);} .off{ background:var(--ink-mute);}
```
- [ ] **Step 2 — `popup.html` :** remplacer le `<style>` inline par `<link rel="stylesheet" href="ui.css">`, garder TOUS les `id` (`dot`, `statusText`, `connectedView`, `downloadVideo`, `toast`, `url`, `sendUrl`, `disconnectedView`, `openOptions`, `retry`) et le `<script>`. Habillage : `body{width:320px;padding:0}` (via ui.css ou style local popup) ; en-tête logo (carré dégradé ambre avec « ↓ ») + wordmark + statut ; `#downloadVideo`→classe `fg-cta` ; `#url`→`fg-input` ; boutons secondaires→`fg-btn` ; pied `fg-mono fg-muted` (host:port + jeton).
- [ ] **Step 3 — `options.html` :** `<link rel="stylesheet" href="ui.css">`, garder les `id` (`token`, `port`, `autoCapture`, `captureTypes`, `save`, `saved`) et le `<script>`. Champs→`fg-input`, bouton Enregistrer→`fg-cta` (ou `fg-btn` accent), titres en Archivo.
- [ ] **Step 4 :** Charger l'extension non empaquetée dans Chrome (`chrome://extensions` → mode dev → charger `extension/`) et vérifier visuellement popup + options (l'app doit tourner pour le statut « Connecté »). Confirmer qu'aucun `id` n'a bougé (le JS fonctionne : statut, bouton vidéo, enregistrement options).
- [ ] **Step 5 : Commit**
```bash
git checkout phase-redesign
git add extension/ui.css extension/popup.html extension/options.html
git commit -m "feat(ui): extension cockpit (popup + options, CSS partagé + polices locales)"
```

---

## Task 6 : Vérification finale + rebuild + terminaison

**Files:** aucun (vérif + build).

- [ ] **Step 1 :** `npx tsc --noEmit` ; `npm test` (toute la suite verte) ; `cargo test --manifest-path src-tauri/Cargo.toml -p download-core` + `... ` (inchangés, sanity).
- [ ] **Step 2 :** `npm run tauri dev` — smoke visuel complet : fond sombre, toolbar/CTA ambre, lignes confortables + chiffres mono + statuts colorés, 4 dialogues sombres, tray inchangé.
- [ ] **Step 3 :** `npm run tauri build` → exe + MSI + NSIS ré-habillés. Confirmer les artefacts.
- [ ] **Step 4 :** Terminer via `superpowers:finishing-a-development-branch` (merge `--no-ff` dans `main`).

---

## Auto-revue (couverture spec)

- Thème sombre + accent ambre + Archivo/JetBrains Mono + densité confortable → Task 1 (tokens/polices) + Task 2/3/4 (app) + Task 5 (extension). ✓
- Polices **auto-hébergées** (offline/CSP) → Task 1 (woff2 locaux) + Task 5 (woff2 extension). ✓
- Couleurs de statut (en cours/terminé/file/pause/erreur/enregistrement) → Task 3 (DownloadRow). ✓
- Chiffres en mono → Task 3 (+ règle de mapping globale). ✓
- Toutes les surfaces app (App, Toolbar, tabs, table/row, pills, menu, 4 dialogues) → Tasks 2-4. ✓
- Extension popup + options + CSS partagé → Task 5. ✓
- Non-régression comportement + libellés conservés + tests mis à jour si markup change → règle d'en-tête + Steps « npm test » de chaque task. ✓
- Sombre uniquement, pas de lib d'animation, pas de renommage productName → respecté (hors périmètre). ✓
- Rebuild installateur → Task 6. ✓
- Cohérence avec l'icône ambre (logo popup réutilise la flèche) → Task 5 Step 2. ✓
