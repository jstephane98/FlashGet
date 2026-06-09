# Nom visible « FlashGet » + icône ambre — Design

**Date :** 2026-06-09
**Statut :** validé
**Module :** FlashGet (Tauri v2)

## Intention

Finaliser l'identité : nom visible de l'app = **« FlashGet »** (au lieu du placeholder
`tauri-app`) et **icône en ambre** cohérente avec le thème « Cockpit » (au lieu de l'indigo
hérité d'avant la refonte).

## Décisions

- **Identifiant inchangé** (`com.steph.tauri-app`) → dossier de données app conservé
  (downloads.sqlite + réglages + jeton du bridge). Pas de reset, pas de re-appairage.
- **Nom visible seulement** : on ne touche pas au crate Rust (`tauri-app`/`tauri_app_lib`) ni à
  `package.json` (internes, zéro impact utilisateur). `productName` pilote l'installeur, l'exe
  installé, le menu Démarrer et le titre.
- **Icône** : même motif (flèche de téléchargement + barre, blanc), dégradé **ambre**
  `#fbbf24 → #f59e0b` (= accent cockpit) au lieu de l'indigo.

## Changements

### Icône (`assets/icon/make_icon.py`)
- `TOP = (251, 191, 36)` (#fbbf24, amber-400) ; `BOT = (245, 158, 11)` (#f59e0b, amber-500).
- Mettre à jour le commentaire d'en-tête (« dégradé ambre »).
- Régénère : `assets/icon/flashget-1024.png` + `extension/icons/{16,32,48,128}.png`.
- `npx tauri icon assets/icon/flashget-1024.png` → régénère `src-tauri/icons/*`
  (supprimer les dossiers `android/`/`ios/` créés au passage, inutiles desktop).

### Nom (`src-tauri/tauri.conf.json`)
- `productName`: `"tauri-app"` → `"FlashGet"`.
- Fenêtre `title`: `"tauri-app"` → `"FlashGet"`.
- `identifier` **inchangé** (`com.steph.tauri-app`).

### Rebuild
- `npm run tauri build` → `FlashGet_0.1.0_x64-setup.exe` (NSIS) + `FlashGet_0.1.0_x64_en-US.msi`,
  avec l'icône ambre.

## Tests / vérification

Visuelle (icône régénérée remontrée ; titre de fenêtre « FlashGet ») + build réussi
+ suite inchangée verte. Pas de test unitaire.

## Hors périmètre (YAGNI)

- Renommer le crate Rust / `package.json` / l'identifiant.
- Publication stores / signature.
