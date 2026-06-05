# System tray + autostart caché + close-to-tray — Design

**Date :** 2026-06-05
**Statut :** validé (design approuvé par l'utilisateur)
**Module :** FlashGet (clone d'IDM, Tauri v2 — Rust + React/TS)

## Intention

Faire de FlashGet une application « qui vit dans la zone de notification Windows (system tray) » :

1. **Démarrage avec Windows, caché** : si l'app est lancée par Windows au démarrage, elle se
   lance **sans fenêtre visible**, seulement présente dans le tray. Réglage **opt-in** (OFF par défaut).
2. **Close-to-tray** : cliquer sur **Fermer (X)** ne quitte PAS l'application — la fenêtre est
   **cachée dans le tray** et les téléchargements continuent. Comportement **toujours actif**.
3. **Tray** : une icône dans le tray avec un menu **« Ouvrir FlashGet » / « Quitter »** ; clic
   gauche = restaurer la fenêtre. Seul « Quitter » ferme réellement l'app.
4. **Instance unique** : relancer l'app pendant qu'elle tourne ramène la fenêtre existante au
   premier plan (pas de seconde copie).

## Décisions (issues du brainstorm)

- **Démarrage caché uniquement quand lancé par Windows** (argument `--hidden`). Lancement
  manuel (double-clic) = fenêtre **visible**.
- **X cache toujours vers le tray** ; quitter seulement via le menu tray « Quitter ».
- **Menu tray** : « Ouvrir FlashGet » + « Quitter ». Clic gauche sur l'icône = ouvrir.
- **Instance unique** activée (re-focus l'existante).
- **Autostart OFF par défaut** ; source de vérité = le registre Windows (via le plugin),
  **pas de colonne en base**.
- Bouton **Réduire (–)** : comportement Windows normal (inchangé).

## Mécanismes

- **Tray** : API native Tauri v2 (`tauri::tray::TrayIconBuilder`, `tauri::menu`), via la feature
  `tray-icon` du crate `tauri`. Pas de plugin tiers.
- **Autostart** : `tauri-plugin-autostart` (Windows = clé de registre `HKCU\…\Run`), enregistré
  avec l'argument `--hidden`.
- **Instance unique** : `tauri-plugin-single-instance`.

## Matrice de comportement

| Déclencheur | Résultat attendu |
|---|---|
| Lancement manuel (sans `--hidden`) | Fenêtre visible + icône tray |
| Lancement au démarrage Windows (`--hidden`) | Fenêtre cachée, app active, icône tray |
| Clic Fermer (X) | Fenêtre cachée dans le tray, app continue |
| Clic gauche tray / menu « Ouvrir FlashGet » | `show()` + `set_focus()` de la fenêtre |
| Menu tray « Quitter » | `app.exit(0)` (fermeture réelle) |
| Relance pendant exécution | Single-instance : `show()` + `set_focus()` + `unminimize()` |
| Bouton Réduire (–) | Réduction Windows normale (non modifié) |

## Architecture

### 1. `src-tauri/Cargo.toml`

- `tauri = { version = "2", features = ["tray-icon"] }` (ajouter la feature).
- `tauri-plugin-autostart = "2"`
- `tauri-plugin-single-instance = "2"`

### 2. `src-tauri/tauri.conf.json`

- Fenêtre principale : ajouter `"visible": false`. On contrôle l'affichage dans `setup`
  (évite le flash d'une fenêtre qui apparaît puis se cache).
- La fenêtre garde son label par défaut `main`.

### 3. `src-tauri/src/lib.rs`

Ordre des plugins dans le `Builder` (single-instance **en premier**, exigence du plugin) :

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
    // ... plugins existants (opener, dialog, shell) ...
```

Dans `setup` (après l'init storage/state existante) :

```rust
// Tray
use tauri::tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState};
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
            if let Some(w) = app.get_webview_window("main") { let _ = w.show(); let _ = w.set_focus(); }
        }
        "quit" => { app.exit(0); }
        _ => {}
    })
    .on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
            let app = tray.app_handle();
            if let Some(w) = app.get_webview_window("main") { let _ = w.show(); let _ = w.set_focus(); }
        }
    })
    .build(app)?;

// Close-to-tray + affichage initial
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

**Note de version Tauri :** si le compilateur rejette `show_menu_on_left_click`, utiliser le nom
antérieur `menu_on_left_click` (selon la version exacte de `tauri` 2.x). Comportement identique :
le clic gauche n'ouvre pas le menu (il est géré par `on_tray_icon_event` pour ouvrir la fenêtre).

### 4. `src-tauri/src/commands.rs`

```rust
use tauri_plugin_autostart::ManagerExt;

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

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

Enregistrer `commands::get_autostart` et `commands::set_autostart` dans le `invoke_handler`.

### 5. Frontend

**`src/api.ts`** :
```ts
export function getAutostart(): Promise<boolean> {
  return invoke<boolean>("get_autostart");
}
export function setAutostart(enabled: boolean): Promise<void> {
  return invoke("set_autostart", { enabled });
}
```

**`src/components/SettingsDialog.tsx`** :
- Nouvel état `const [autostart, setAutostart] = useState(false);`.
- Dans le `useEffect` d'ouverture : `api.getAutostart().then(setAutostart);`.
- Une case à cocher dans le dialogue, intitulée
  **« Démarrer avec Windows (lancement caché dans la zone de notification) »**.
- Dans `save()` : `await api.setAutostart(autostart);` (à côté de `updateSettings`).

## Source de vérité & défaut

Le registre Windows est la source de vérité (lecture via `is_enabled()`). Pas de persistance en
base. Défaut OFF « gratuit » : le plugin n'inscrit rien tant qu'on n'appelle pas `enable()`.

## Capabilities / sécurité

Aucun changement de capabilities : tout le contrôle fenêtre/tray se fait **côté Rust** ; les
commandes autostart sont des commandes maison (non soumises aux permissions IPC des plugins).
Le frontend n'appelle aucune API JS de plugin.

## Tests

Le comportement (tray, événements fenêtre, registre, démarrage caché) est OS/runtime et ne se
prête pas à des tests unitaires (effets de bord, besoin d'`AppHandle`). Les commandes autostart
sont une colle triviale. **Vérification = compilation + smoke-test manuel.** Pas de test du
`SettingsDialog` (aucun n'existe, et son `useEffect` multiplie les appels à mocker).

Smoke-test manuel :
1. Lancement manuel → fenêtre visible, icône tray présente.
2. Clic X → fenêtre disparaît, app toujours active (icône tray), un téléchargement en cours continue.
3. Clic gauche sur l'icône tray → fenêtre revient ; menu « Ouvrir FlashGet » idem.
4. Cocher « Démarrer avec Windows » + Enregistrer → l'entrée `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` est créée avec `--hidden`.
5. Relancer l'exe pendant que l'app tourne → la fenêtre existante revient au premier plan (pas de 2e instance).
6. Menu tray « Quitter » → l'app se ferme réellement.
7. (Après build/install) redémarrage de session Windows → l'app démarre cachée dans le tray.

## Séquencement du livrable

1. **Merger `phase-batch-download` dans `main`** (`--no-ff`) — tests automatiques verts
   (le smoke GUI du lot est recommandé mais accepté sur la foi des tests).
2. Implémenter cette feature sur **`phase-tray-autostart`** (depuis `main`), puis merger.
3. **`npm run tauri build`** → `src-tauri/target/release/tauri-app.exe` + installateurs
   `bundle/msi/*.msi` et `bundle/nsis/*-setup.exe`.

⚠️ **Vigilance build (à vérifier en premier) :** le bundling exige que les binaires `externalBin`
soient nommés avec le triplet cible, p.ex. `src-tauri/bin/ffmpeg-x86_64-pc-windows-msvc.exe` et
`yt-dlp-x86_64-pc-windows-msvc.exe`. Si le nommage actuel diffère, corriger avant `tauri build`
(sinon échec « binary not found »).

## Hors périmètre (YAGNI)

- Router le bouton Réduire (–) vers le tray (seul X est demandé).
- Un réglage « X quitte vs X réduit » (le comportement close-to-tray est fixe).
- Notifications de bureau, badge de progression sur l'icône tray.
- Lancement caché configurable autrement que par l'autostart.
- Signature de l'installateur / auto-update (phase P6 ultérieure).

## Réutilisation de l'existant

- `SettingsDialog` (modèle Enregistrer), `api.ts` (wrappers `invoke`), `commands.rs`,
  `invoke_handler` — patterns réutilisés.
- Icône tray = `app.default_window_icon()` (icônes déjà présentes dans `src-tauri/icons/`).
