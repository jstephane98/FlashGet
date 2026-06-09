# Nom « FlashGet » + icône ambre — Plan d'implémentation

> Exécution **inline** (contrôleur) — tâche visuelle/config. Steps en cases à cocher.

**Goal:** Nom visible de l'app = « FlashGet » + icône en ambre, installateur reconstruit. Identifiant et données conservés.

**Branche :** `phase-brand-name-icon` (depuis `main`).
**Référence spec :** `docs/superpowers/specs/2026-06-09-brand-name-icon-design.md`

---

## Task 1 : Icône ambre

- [ ] **Step 1 :** Dans `assets/icon/make_icon.py`, remplacer `TOP = (99, 102, 241)` par `TOP = (251, 191, 36)` et `BOT = (67, 56, 202)` par `BOT = (245, 158, 11)` ; mettre à jour le commentaire d'en-tête (« dégradé ambre »).
- [ ] **Step 2 :** `python assets/icon/make_icon.py` → régénère le master + les PNG extension. Vérification visuelle du master.
- [ ] **Step 3 :** `npx tauri icon assets/icon/flashget-1024.png` ; supprimer `src-tauri/icons/android src-tauri/icons/ios src-tauri/icons/64x64.png` s'ils apparaissent.
- [ ] **Step 4 :** Vérif visuelle d'un PNG régénéré (`src-tauri/icons/128x128.png`).
- [ ] **Step 5 : Commit**
```bash
git checkout phase-brand-name-icon
git add assets/icon/make_icon.py assets/icon/flashget-1024.png extension/icons src-tauri/icons
git commit -m "feat(brand): icône FlashGet en ambre (cohérence cockpit)"
```

## Task 2 : Nom visible « FlashGet »

- [ ] **Step 1 :** Dans `src-tauri/tauri.conf.json` : `"productName": "FlashGet"` et la fenêtre `"title": "FlashGet"`. Ne PAS toucher `identifier`.
- [ ] **Step 2 :** `npx tauri info` ou relecture du fichier pour confirmer les 2 changements et l'identifiant inchangé.
- [ ] **Step 3 : Commit**
```bash
git checkout phase-brand-name-icon
git add src-tauri/tauri.conf.json
git commit -m "feat(brand): nom visible de l'app = FlashGet (productName + titre fenêtre)"
```

## Task 3 : Rebuild + vérif + merge

- [ ] **Step 1 :** `npm test` (52 verts) + `cargo build --manifest-path src-tauri/Cargo.toml` (sanity).
- [ ] **Step 2 :** `npm run tauri build` → confirmer `…/bundle/nsis/FlashGet_0.1.0_x64-setup.exe` + `…/bundle/msi/FlashGet_0.1.0_x64_en-US.msi` + icône ambre.
- [ ] **Step 3 :** Merge `--no-ff` dans `main`.

---

## Auto-revue (couverture spec)
- Icône ambre (générateur + master + extension + app via tauri icon) → Task 1. ✓
- productName + titre fenêtre = FlashGet, identifiant inchangé → Task 2. ✓
- Rebuild installateur nommé FlashGet → Task 3. ✓
- Crate Rust / package.json / identifiant inchangés → respecté (hors périmètre). ✓
