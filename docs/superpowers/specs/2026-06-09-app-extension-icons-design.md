# Icônes app + extension — Design

**Date :** 2026-06-09
**Statut :** validé (design approuvé)
**Module :** FlashGet (Tauri v2 + extension Chromium MV3)

## Intention

Remplacer les icônes placeholder (logo Tauri par défaut pour l'app ; pièce de puzzle générique
pour l'extension) par une vraie icône de marque FlashGet, déclinée dans toutes les tailles
nécessaires, et la cuire dans un installateur frais.

## Décision visuelle

Concept **B** : **flèche de téléchargement blanche + barre de base**, sur une tuile à coins
arrondis remplie d'un **dégradé indigo vertical** (indigo-500 `#6366F1` → indigo-700 `#4338CA`,
la couleur d'accent déjà utilisée dans l'app). Coins transparents (tuile arrondie, rayon ~22 %).
Glyphe centré, lisible jusqu'à 16px (validé sur planche de comparaison fond clair/sombre).

## Architecture (asset pipeline, reproductible)

### 1. Source maîtresse
- `assets/icon/make_icon.py` — générateur Pillow **déterministe** (variante B). Produit :
  - `assets/icon/flashget-1024.png` (master 1024×1024, RGBA, coins transparents) ;
  - `extension/icons/16.png`, `32.png`, `48.png`, `128.png` (redimensionnés LANCZOS depuis le master).
- Le script et le master PNG sont commités → l'icône est régénérable à l'identique.

### 2. Icônes de l'app
- `npx tauri icon assets/icon/flashget-1024.png` régénère tout `src-tauri/icons/` :
  `icon.ico`, `icon.icns`, `32x32.png`, `128x128.png`, `128x128@2x.png`, `icon.png`,
  et les `Square*Logo.png` / `StoreLogo.png`. Remplace les placeholders.
- **Aucun** changement de `tauri.conf.json` (les chemins `bundle.icon` sont déjà ceux-là).

### 3. Icônes de l'extension
- `extension/icons/{16,32,48,128}.png` (générés en 1).
- `extension/manifest.json` :
  - ajouter `"icons": { "16": "icons/16.png", "32": "icons/32.png", "48": "icons/48.png", "128": "icons/128.png" }`
  - dans `"action"`, ajouter `"default_icon": { "16": "icons/16.png", "32": "icons/32.png", "48": "icons/48.png", "128": "icons/128.png" }`

### 4. Rebuild
- Re-lancer `npm run tauri build` → exe + MSI + NSIS portant la nouvelle icône.

## Branche & exécution

- Travail sur **`phase-tray-autostart`** (livrable unique « finitions v0.1 » : tray + autostart + icônes).
- **Exécution inline** (tâche d'asset visuelle, vérification à l'œil), pas en subagents.

## Tests / vérification

Visuelle (rendu de l'icône appliquée remontré) + `manifest.json` reste un JSON valide + le build
réussit. Pas de test unitaire (asset).

## Hors périmètre (YAGNI)

- Renommer `productName` `tauri-app` → « FlashGet » (change aussi identifiant/chemins d'install) — passage dédié si voulu.
- Écran de démarrage / bannières store / captures.
- Variantes d'icône clair/sombre adaptatives.
