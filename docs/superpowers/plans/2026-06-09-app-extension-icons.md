# Icônes app + extension — Plan d'implémentation

> **For agentic workers:** exécution **inline** par le contrôleur (tâche d'asset visuelle). Steps en cases à cocher.

**Goal:** Remplacer les icônes placeholder par l'icône FlashGet (variante B) sur l'app et l'extension, et rebuilder l'installateur.

**Architecture:** Générateur Pillow déterministe → master 1024 + PNG extension ; `tauri icon` pour les tailles app ; `manifest.json` pour l'extension ; rebuild Tauri.

**Tech Stack :** Python Pillow, `npx tauri icon`, Tauri v2, extension MV3.

**Branche :** `phase-tray-autostart` (garde `git checkout phase-tray-autostart` avant commit).

**Référence spec :** `docs/superpowers/specs/2026-06-09-app-extension-icons-design.md`

---

## Task 1 : Générateur + master + PNG extension

**Files:**
- Create: `assets/icon/make_icon.py`
- Create (générés): `assets/icon/flashget-1024.png`, `extension/icons/{16,32,48,128}.png`

- [ ] **Step 1 :** Écrire `assets/icon/make_icon.py` (variante B déterministe) qui produit le master 1024 et les 4 PNG extension.
- [ ] **Step 2 :** `python assets/icon/make_icon.py` ; vérifier que `assets/icon/flashget-1024.png` et `extension/icons/{16,32,48,128}.png` existent.
- [ ] **Step 3 :** Vérification visuelle (afficher le master + un petit format) — cohérent avec la variante B validée.
- [ ] **Step 4 : Commit**
```bash
git checkout phase-tray-autostart
git add assets/icon/make_icon.py assets/icon/flashget-1024.png extension/icons
git commit -m "feat(brand): icône FlashGet (générateur + master 1024 + PNG extension)"
```

## Task 2 : Icônes de l'app (tauri icon)

**Files:** Modify: `src-tauri/icons/*` (régénérés)

- [ ] **Step 1 :** `npx tauri icon assets/icon/flashget-1024.png` (régénère `src-tauri/icons/`).
- [ ] **Step 2 :** `ls src-tauri/icons/` — confirmer `icon.ico`, `icon.icns`, `32x32.png`, `128x128.png`, `128x128@2x.png` régénérés (dates récentes).
- [ ] **Step 3 :** Vérification visuelle d'un PNG régénéré (ex. `128x128.png`).
- [ ] **Step 4 : Commit**
```bash
git checkout phase-tray-autostart
git add src-tauri/icons
git commit -m "feat(brand): icônes de l'app régénérées depuis le master FlashGet"
```

## Task 3 : Icônes de l'extension (manifest)

**Files:** Modify: `extension/manifest.json`

- [ ] **Step 1 :** Ajouter le champ `"icons"` (16/32/48/128) et `"default_icon"` dans `"action"`.
- [ ] **Step 2 :** Valider le JSON : `python -c "import json;json.load(open('extension/manifest.json'))"` → pas d'erreur.
- [ ] **Step 3 : Commit**
```bash
git checkout phase-tray-autostart
git add extension/manifest.json
git commit -m "feat(brand): icône de l'extension (manifest icons + action.default_icon)"
```

## Task 4 : Rebuild de l'installateur

**Files:** aucun (build).

- [ ] **Step 1 :** `npm run tauri build` → exe + MSI + NSIS avec la nouvelle icône.
- [ ] **Step 2 :** Confirmer les artefacts ; vérification visuelle de l'icône de `tauri-app.exe` (Explorateur) et, après chargement, de l'extension.
- [ ] **Step 3 :** Terminer la branche via `superpowers:finishing-a-development-branch` (merge `--no-ff` dans `main`).

---

## Auto-revue (couverture spec)

- Variante B, dégradé indigo, coins transparents → Task 1 (générateur). ✓
- App toutes tailles (.ico/.icns/PNG/Square*) → Task 2 (`tauri icon`). ✓
- Extension 16/32/48/128 + manifest `icons` + `action.default_icon` → Task 1 (PNG) + Task 3 (manifest). ✓
- Rebuild installateur → Task 4. ✓
- Pas de changement `tauri.conf.json` (chemins inchangés) → confirmé, aucun fichier conf modifié. ✓
- Reproductibilité (script + master commités) → Task 1. ✓
