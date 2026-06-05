# System tray + autostart caché + close-to-tray — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** FlashGet vit dans la zone de notification Windows : démarrage caché au login (opt-in), le bouton Fermer (X) réduit dans le tray sans quitter, menu tray Ouvrir/Quitter, instance unique — puis produire l'exécutable + installateurs.

**Architecture:** Tray natif Tauri v2 (feature `tray-icon`) + `tauri-plugin-autostart` (registre `Run`, argument `--hidden`) + `tauri-plugin-single-instance`. Tout le contrôle fenêtre/tray est côté Rust dans `setup`. L'autostart est exposé par 2 commandes fines vers une case dans les Réglages ; la source de vérité est le registre (pas de base).

**Tech Stack :** Rust / Tauri v2 (`tray-icon`, plugins autostart + single-instance), React 19 + TypeScript.

**Branche de travail :** `phase-tray-autostart`, créée depuis `main` APRÈS merge du module lot (Task 1). **Garde subagent : `git checkout phase-tray-autostart` avant chaque commit.**

**Référence spec :** `docs/superpowers/specs/2026-06-05-tray-autostart-design.md`

**Note sur les tests :** ce module est du comportement OS/runtime (tray, événements fenêtre, registre, démarrage caché) — non testable unitairement (effets de bord, besoin d'`AppHandle`). Les commandes autostart sont une colle triviale. **Vérification = compilation (`cargo build`) + smoke-test manuel.** Tâches non-TDD assumées.

**Note d'exécution :** les Task 1 (merge + branche) et Task 6 (build + smoke) sont des opérations **contrôleur** (git/build), pas des tâches de code à déléguer à un subagent. Les Task 2→5 sont déléguées.

---

## Structure des fichiers

**Modifiés :**
- `src-tauri/Cargo.toml` — feature `tray-icon` + deps `tauri-plugin-autostart`, `tauri-plugin-single-instance`.
- `src-tauri/tauri.conf.json` — fenêtre `main` `"visible": false`.
- `src-tauri/src/lib.rs` — enregistrement des 2 plugins + tray + close-to-tray + affichage conditionnel dans `setup`.
- `src-tauri/src/commands.rs` — commandes `get_autostart` / `set_autostart`.
- `src/api.ts` — wrappers `getAutostart` / `setAutostart`.
- `src/components/SettingsDialog.tsx` — case « Démarrer avec Windows ».

---

## Task 1 : (contrôleur) Merge du module lot + branche de travail + prérequis build

**Files:** aucun fichier de code (opérations git + vérification).

- [ ] **Step 1 : Merger `phase-batch-download` dans `main` (--no-ff)**

```bash
git checkout main
git merge --no-ff phase-batch-download -m "Merge phase-batch-download: téléchargement par lots (coller plusieurs liens -> file)"
```
Expected: merge réussi, fast-forward refusé (commit de merge créé).

- [ ] **Step 2 : Vérifier que la suite passe sur `main` après merge**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
npm test
```
Expected: Rust (download-core 171 + tauri 12) et Vitest (45) verts.

- [ ] **Step 3 : Créer la branche de travail**

```bash
git checkout -b phase-tray-autostart
```

- [ ] **Step 4 : Vérifier les prérequis de bundling (sidecars `externalBin`)**

Run: `ls src-tauri/bin/`
Expected: présence de `ffmpeg-x86_64-pc-windows-msvc.exe` ET `yt-dlp-x86_64-pc-windows-msvc.exe` (nommage triplet-cible). C'est déjà le cas au moment d'écrire ce plan — confirmer simplement. Si l'un manque sous ce nom exact, le renommer avant la Task 6 (sinon `tauri build` échoue avec « binary not found »).

(Pas de commit : opérations git/setup uniquement.)

---

## Task 2 : Dépendances + enregistrement des plugins (single-instance, autostart)

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/lib.rs` (chaîne du `Builder`)

- [ ] **Step 1 : Ajouter la feature `tray-icon` et les deux plugins dans `Cargo.toml`**

Dans `src-tauri/Cargo.toml`, remplacer la ligne `tauri = { version = "2", features = [] }` par :
```toml
tauri = { version = "2", features = ["tray-icon"] }
```
Et ajouter, à la suite des autres `tauri-plugin-*` (après `tauri-plugin-shell = "2"`) :
```toml
tauri-plugin-autostart = "2"
tauri-plugin-single-instance = "2"
```

- [ ] **Step 2 : Enregistrer les plugins dans `src-tauri/src/lib.rs`**

Dans `pub fn run()`, la chaîne commence par `tauri::Builder::default()`. Insérer le plugin single-instance **en tout premier** (exigence du plugin), puis autostart, AVANT les `.plugin(tauri_plugin_opener::init())` existants. Le début devient :

```rust
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
```

`get_webview_window` vient du trait `tauri::Manager`, déjà importé (`use tauri::Manager;`).

- [ ] **Step 3 : Compiler**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: compilation réussie (les crates des plugins sont téléchargées au 1er build). Aucune erreur.

- [ ] **Step 4 : Commit**

```bash
git checkout phase-tray-autostart
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs
git commit -m "feat(tauri): plugins single-instance + autostart, feature tray-icon

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 3 : Tray + close-to-tray + affichage conditionnel (cœur du module)

**Files:**
- Modify: `src-tauri/tauri.conf.json` (fenêtre `visible: false`)
- Modify: `src-tauri/src/lib.rs` (bloc `setup`)

- [ ] **Step 1 : Masquer la fenêtre au démarrage dans `tauri.conf.json`**

Dans `src-tauri/tauri.conf.json`, l'objet fenêtre est :
```json
      {
        "title": "tauri-app",
        "width": 800,
        "height": 600
      }
```
Le remplacer par (ajout de `"visible": false`) :
```json
      {
        "title": "tauri-app",
        "width": 800,
        "height": 600,
        "visible": false
      }
```

- [ ] **Step 2 : Ajouter le tray + close-to-tray + affichage conditionnel dans `setup`**

Dans `src-tauri/src/lib.rs`, le `setup` actuel se termine par :
```rust
            bridge::start(app.handle().clone());
            commands::recover_interrupted_recordings(app.handle());
            Ok(())
        })
```
Insérer le bloc suivant juste AVANT `Ok(())` (donc après `recover_interrupted_recordings`) :

```rust
            // --- System tray ---
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
            use tauri::menu::{Menu, MenuItem};

            let open_i = MenuItem::with_id(app, "open", "Ouvrir FlashGet", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_i, &quit_i])?;
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            // --- Close-to-tray + affichage initial conditionnel ---
            if let Some(win) = app.get_webview_window("main") {
                let win_for_close = win.clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win_for_close.hide();
                    }
                });
                let hidden = std::env::args().any(|a| a == "--hidden");
                if !hidden {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
```

**Note de version Tauri :** si `cargo build` rejette `show_menu_on_left_click` (méthode inconnue), remplacer cet appel par `.menu_on_left_click(false)` (nom antérieur dans certaines versions 2.x). Le comportement visé est identique : le clic gauche n'ouvre pas le menu (il est intercepté par `on_tray_icon_event` pour afficher la fenêtre).

- [ ] **Step 3 : Compiler**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: compilation réussie. Si erreur sur `show_menu_on_left_click`, appliquer la note de version ci-dessus puis recompiler.

- [ ] **Step 4 : Vérification runtime rapide (dev)**

Run: `npm run tauri dev` (lancer, observer, puis fermer le process)
Expected vérifiable : la fenêtre s'affiche (lancement manuel, pas de `--hidden`) ; une icône apparaît dans le tray ; cliquer X **cache** la fenêtre sans terminer le process ; clic gauche sur l'icône tray (ou menu « Ouvrir FlashGet ») la **réaffiche** ; menu « Quitter » termine l'app. (Le démarrage caché `--hidden` et l'autostart seront vérifiés après build en Task 6.)

- [ ] **Step 5 : Commit**

```bash
git checkout phase-tray-autostart
git add src-tauri/tauri.conf.json src-tauri/src/lib.rs
git commit -m "feat(tray): icône tray (Ouvrir/Quitter), close-to-tray, démarrage caché conditionnel

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 4 : Commandes `get_autostart` / `set_autostart`

**Files:**
- Modify: `src-tauri/src/commands.rs` (ajouter les 2 commandes)
- Modify: `src-tauri/src/lib.rs` (enregistrer dans `invoke_handler`)

- [ ] **Step 1 : Ajouter les commandes dans `commands.rs`**

En haut de `src-tauri/src/commands.rs`, ajouter l'import du trait (avec les autres `use download_core::...`/`use tauri::...`) :
```rust
use tauri_plugin_autostart::ManagerExt;
```
Puis, à la fin du fichier (ou près des autres commandes de réglages), ajouter :
```rust
/// Indique si l'app est inscrite au démarrage de Windows (lecture du registre via le plugin).
#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// Active/désactive le démarrage avec Windows. Le plugin enregistre la clé `Run`
/// avec l'argument `--hidden` (configuré à l'init), pour un démarrage caché dans le tray.
#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())
    } else {
        manager.disable().map_err(|e| e.to_string())
    }
}
```

- [ ] **Step 2 : Enregistrer les commandes dans `invoke_handler` (`lib.rs`)**

Dans `src-tauri/src/lib.rs`, ajouter ces deux lignes dans le `tauri::generate_handler![ ... ]` (p.ex. après `commands::regenerate_bridge_token,`) :
```rust
            commands::get_autostart,
            commands::set_autostart,
```

- [ ] **Step 3 : Compiler + lint du nouveau code**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: compilation réussie.

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets 2>&1 | grep -E "commands.rs|lib.rs"`
Expected: aucune ligne (le nouveau code n'introduit pas de warning). NB : 13 warnings clippy **préexistants** existent dans `download-core` (engine/hls/dash) — sans rapport, à ignorer.

- [ ] **Step 4 : Commit**

```bash
git checkout phase-tray-autostart
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(tauri): commandes get_autostart / set_autostart

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 5 : Frontend — API + case « Démarrer avec Windows » dans les Réglages

**Files:**
- Modify: `src/api.ts` (2 wrappers)
- Modify: `src/components/SettingsDialog.tsx` (état + chargement + case + save)

- [ ] **Step 1 : Ajouter les wrappers dans `src/api.ts`**

À la fin de `src/api.ts` (après `regenerateBridgeToken`), ajouter :
```ts
export function getAutostart(): Promise<boolean> {
  return invoke<boolean>("get_autostart");
}

export function setAutostart(enabled: boolean): Promise<void> {
  return invoke("set_autostart", { enabled });
}
```

- [ ] **Step 2 : Câbler la case dans `SettingsDialog.tsx`**

(a) Ajouter un état près des autres `useState` (après `const [copied, setCopied] = useState(false);`) :
```tsx
  const [autostart, setAutostart] = useState(false);
```

(b) Dans le `useEffect(() => { if (!open) return; ... }, [open])`, ajouter le chargement à la suite des autres appels (après `api.getBridgeInfo().then(setBridge);`) :
```tsx
    api.getAutostart().then(setAutostart);
```

(c) Dans la fonction `save()`, ajouter l'application de l'autostart juste avant `onClose();` :
```tsx
    await api.setAutostart(autostart);
```
La fonction `save()` devient :
```tsx
  async function save() {
    const bps = unlimited ? 0 : Math.round(speedValue * 1024 * 1024);
    await api.updateSettings(Math.max(1, Math.round(maxConcurrent)), bps);
    await api.setAutostart(autostart);
    onClose();
  }
```

(d) Ajouter le bloc de la case à cocher dans le JSX, juste après le bloc « Vitesse de téléchargement max » (après le `</div>` qui ferme ce bloc `px-4 py-3 border-b border-gray-100`, avant le bloc « Dossiers par catégorie ») :
```tsx
        <div className="px-4 py-3 border-b border-gray-100">
          <label className="text-sm flex items-center gap-2">
            <input
              type="checkbox"
              checked={autostart}
              onChange={(e) => setAutostart(e.target.checked)}
            />
            Démarrer avec Windows (lancement caché dans la zone de notification)
          </label>
        </div>
```

- [ ] **Step 3 : Vérifier compilation + suite de tests frontend**

Run: `npx tsc --noEmit`
Expected: aucune erreur de type.

Run: `npm test`
Expected: toute la suite Vitest passe (inchangée — aucun test ne couvre `SettingsDialog`).

- [ ] **Step 4 : Commit**

```bash
git checkout phase-tray-autostart
git add src/api.ts src/components/SettingsDialog.tsx
git commit -m "feat(ui): case « Démarrer avec Windows » dans les Réglages (autostart)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

## Task 6 : (contrôleur) Smoke-test runtime + production de l'exécutable

**Files:** aucun (vérification + build).

- [ ] **Step 1 : Smoke-test runtime complet (dev)**

Run: `npm run tauri dev`
Scénario à valider :
1. Lancement manuel → fenêtre **visible**, icône tray présente.
2. Démarrer un téléchargement, puis clic **X** → la fenêtre **disparaît**, l'app continue (le téléchargement progresse toujours), icône tray présente.
3. Clic gauche sur l'icône tray → la fenêtre **revient** ; idem via menu « Ouvrir FlashGet ».
4. Ouvrir ⚙ Réglages → cocher « Démarrer avec Windows » → Enregistrer. Vérifier la clé de registre :
   `reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Run"` (PowerShell) → une entrée pour l'app avec `--hidden`.
5. Décocher + Enregistrer → l'entrée `Run` disparaît.
6. Relancer une 2e instance de l'app pendant qu'elle tourne → la fenêtre existante revient au premier plan (pas de 2e fenêtre).
7. Menu tray « Quitter » → l'app se ferme réellement.

Expected: tous les points OK.

- [ ] **Step 2 : Construire l'exécutable + installateurs**

Run: `npm run tauri build`
(Build long : compilation Rust en release + bundling ; le sidecar ffmpeg ~220 Mo alourdit l'installateur. Prévoir plusieurs minutes ; WiX/NSIS sont téléchargés automatiquement au 1er build — connexion requise.)
Expected: build réussi. Artefacts :
- Exe : `src-tauri/target/release/tauri-app.exe`
- Installateur MSI : `src-tauri/target/release/bundle/msi/*.msi`
- Installateur NSIS : `src-tauri/target/release/bundle/nsis/*-setup.exe`

- [ ] **Step 3 : Vérifier les artefacts**

Run: `ls -la src-tauri/target/release/tauri-app.exe src-tauri/target/release/bundle/msi/ src-tauri/target/release/bundle/nsis/`
Expected: l'exe et au moins un installateur présents.

- [ ] **Step 4 : Smoke-test du démarrage caché (post-install, manuel)**

Installer via le NSIS/MSI (ou lancer l'exe), s'assurer que l'autostart est coché, puis **fermer la session / redémarrer Windows**. Au retour de session : l'app démarre **cachée** (pas de fenêtre), seulement présente dans le tray ; un clic sur l'icône l'affiche.

Expected: démarrage caché confirmé. (Étape manuelle réalisée par l'utilisateur.)

- [ ] **Step 5 : Terminer la branche**

Utiliser `superpowers:finishing-a-development-branch` pour merger `phase-tray-autostart` dans `main` (`--no-ff`, convention du projet).

---

## Auto-revue du plan (couverture spec)

- **Démarrage caché opt-in (`--hidden`)** → Task 2 (init autostart avec `Some(vec!["--hidden"])`) + Task 3 (lecture `std::env::args()` → show si absent) + Task 4/5 (case opt-in, défaut OFF via registre). ✓
- **Close-to-tray (X ne quitte pas)** → Task 3 (`CloseRequested` → `prevent_close` + `hide`). ✓
- **Tray Ouvrir/Quitter + clic gauche** → Task 3 (`MenuItem` open/quit, `on_menu_event`, `on_tray_icon_event`). ✓
- **Instance unique (re-focus)** → Task 2 (`single_instance::init` → show+unminimize+focus). ✓
- **Source de vérité = registre, défaut OFF** → Task 4 (`is_enabled`/`enable`/`disable`, pas de base) + Task 5 (case lit l'état réel). ✓
- **Case dans les Réglages appliquée à l'Enregistrement** → Task 5 (chargement dans `useEffect`, `setAutostart` dans `save`). ✓
- **Pas de changement de capabilities** → confirmé : aucun fichier `capabilities/*` modifié dans le plan ; contrôle fenêtre/tray 100 % Rust. ✓
- **Livrable (exe + installateurs)** → Task 6 (`tauri build`, artefacts MSI/NSIS). ✓
- **Inclure le module lot** → Task 1 (merge `phase-batch-download` → `main` avant de brancher). ✓
- **Vigilance externalBin** → Task 1 Step 4 (vérif nommage triplet — déjà correct). ✓
- **Tests** : aucun test unitaire (comportement OS/runtime) ; vérification compilation + smoke manuel détaillé (Task 3 Step 4, Task 6). Écart au modèle TDD **assumé et documenté** (en-tête du plan). ✓
- **Bouton Réduire (–) inchangé / pas de réglage X-quitte** → hors périmètre, non implémenté (conforme YAGNI de la spec). ✓
