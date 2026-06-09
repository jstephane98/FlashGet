# Clic sur la notif → focus de l'app (Windows) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Sur Windows, cliquer la toast de fin de téléchargement ramène la fenêtre `main` de FlashGet au premier plan.

**Architecture:** L'émission de `notify_terminal` est scindée par OS : Windows → `tauri-winrt-notification` natif avec `on_activated` qui appelle un helper `focus_main_window` ; autres OS → `tauri-plugin-notification` conservé. Le helper de focus est extrait et partagé avec le handler `single_instance`.

**Tech Stack:** Rust / Tauri v2, `tauri-winrt-notification` 0.7 (Windows-only), `tauri-plugin-notification` (autres OS).

**Branche :** `phase-notify-click` (déjà créée depuis `main` ; la spec y est committée).

**NB :** Cette feature n'est vérifiable qu'en **build installé** (l'AUMID/raccourci n'existe pas en `tauri dev`). Pas de test unitaire (colle Tauri/Windows). Critère automatique par tâche = `cargo check`.

---

## File Structure

- **Modify** `src-tauri/src/commands.rs` — ajouter `focus_main_window` (helper cross-platform) ; scinder l'émission de `notify_terminal` par `cfg`.
- **Modify** `src-tauri/src/lib.rs` — `single_instance` appelle `commands::focus_main_window`.
- **Modify** `src-tauri/Cargo.toml` — dépendance Windows-only `tauri-winrt-notification`.

---

## Task 1: Helper `focus_main_window` partagé

**Files:**
- Modify: `src-tauri/src/commands.rs` (ajouter le helper juste avant `fn notify_terminal`, ~ligne 845)
- Modify: `src-tauri/src/lib.rs` (handler `single_instance`, lignes 15-21)

- [ ] **Step 1 : Ajouter le helper dans commands.rs**

Insérer juste **avant** le doc-comment de `notify_terminal` (avant la ligne 845 `/// Émet une notif...`) :

```rust
/// Ramène la fenêtre principale au premier plan (montre + désiconifie + focus).
pub(crate) fn focus_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
```

(`AppHandle` et le trait `Manager` — qui fournit `get_webview_window` — sont déjà importés en haut de commands.rs : `use tauri::{AppHandle, Emitter, Manager, State};`.)

- [ ] **Step 2 : Refactorer `single_instance` dans lib.rs**

Remplacer le handler actuel (lib.rs lignes 15-21) :

```rust
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
```

par :

```rust
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            commands::focus_main_window(app);
        }))
```

(`app` est un `&AppHandle` dans ce closure. Le module `commands` est déjà accessible — lib.rs y référence `commands::...` dans le `generate_handler`.)

- [ ] **Step 3 : Vérifier que ça compile**

Run (depuis `src-tauri/`) : `cargo check`
Expected : compile, 0 erreur. (Si `get_webview_window` n'était pas résolu, c'est que `Manager` manque — mais il est déjà importé.)

- [ ] **Step 4 : Commit**

Vérifier `git branch --show-current` == `phase-notify-click` (sinon `git checkout phase-notify-click`), puis :
```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "refactor(tauri): helper focus_main_window partagé (single_instance)"
```

---

## Task 2: Dépendance `tauri-winrt-notification` (Windows-only)

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1 : Ajouter la dépendance Windows-only**

À la fin de `src-tauri/Cargo.toml`, ajouter une section cible Windows (si une section `[target.'cfg(windows)'.dependencies]` existe déjà, y ajouter la ligne au lieu de recréer la section) :

```toml
[target.'cfg(windows)'.dependencies]
tauri-winrt-notification = "0.7"
```

- [ ] **Step 2 : Vérifier que ça compile**

Run (depuis `src-tauri/`) : `cargo check`
Expected : compile, 0 erreur. Cargo résout `tauri-winrt-notification` 0.7.x (déjà présent en transitif à 0.7.2 → pas de nouveau téléchargement majeur ; Cargo.lock peut être inchangé ou légèrement ajusté).

- [ ] **Step 3 : Commit**

Vérifier la branche `phase-notify-click`, puis :
```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "build(tauri): dépendance Windows tauri-winrt-notification"
```

---

## Task 3: Émission scindée par OS dans `notify_terminal`

**Files:**
- Modify: `src-tauri/src/commands.rs` (corps de `notify_terminal`, lignes 859-864 actuelles)

Dépend de : Task 1 (`focus_main_window`) + Task 2 (dépendance).

- [ ] **Step 1 : Remplacer le bloc d'émission**

Dans `notify_terminal`, le bloc actuel (lignes 859-864) :

```rust
    let (title, body) = match status {
        DownloadStatus::Completed => ("Téléchargement terminé", file_name),
        _ => ("Échec du téléchargement", file_name),
    };
    use tauri_plugin_notification::NotificationExt;
    let _ = app.notification().builder().title(title).body(&body).show();
}
```

devient :

```rust
    let (title, body) = match status {
        DownloadStatus::Completed => ("Téléchargement terminé", file_name),
        _ => ("Échec du téléchargement", file_name),
    };

    #[cfg(target_os = "windows")]
    {
        // Émission native : permet de gérer le clic (on_activated) — absent du plugin desktop.
        // app_id = identifiant du config = AUMID du raccourci créé par l'installateur.
        let app_id = app.config().identifier.clone();
        let handle = app.clone();
        let _ = tauri_winrt_notification::Toast::new(&app_id)
            .title(title)
            .text1(&body)
            .on_activated(move |_action| {
                focus_main_window(&handle);
                Ok(())
            })
            .show();
    }

    #[cfg(not(target_os = "windows"))]
    {
        use tauri_plugin_notification::NotificationExt;
        let _ = app.notification().builder().title(title).body(&body).show();
    }
}
```

Notes pour l'implémenteur :
- `app.config()` renvoie le `&Config` ; `.identifier` est une `String` → `.clone()`.
- `Toast::new(app_id: &str)`, `.title(&str)`, `.text1(&str)`, `.show() -> Result<()>`.
- `.on_activated<F>(F)` avec `F: FnMut(Option<String>) -> Result<()> + Send + 'static`. Le closure DOIT renvoyer `Ok(())` (le `Result` du crate). `AppHandle` est `Clone + Send + 'static`, donc capturer `handle` est OK.
- Le `_action: Option<String>` est ignoré (clic sur le corps, pas de boutons).
- Ne PAS retirer `tauri-plugin-notification` ni sa permission : la branche `#[cfg(not(windows))]` les utilise.

- [ ] **Step 2 : Vérifier que ça compile (Windows)**

Run (depuis `src-tauri/`) : `cargo check`
Expected : compile, 0 erreur, 0 warning lié à un import inutilisé (l'ancien `use tauri_plugin_notification::NotificationExt;` global a bien été déplacé dans la branche non-Windows).

- [ ] **Step 3 : Vérifier que la branche non-Windows compile aussi (relecture)**

La compilation croisée n'est pas requise ici (CI non-Windows hors périmètre). Relire la branche `#[cfg(not(target_os = "windows"))]` : `title` (`&str`) et `body` (`String`) sont en scope, `app.notification().builder().title(title).body(&body).show()` est identique à l'ancien code → OK.

- [ ] **Step 4 : Vérifier que rien d'autre n'a cassé**

Run (depuis `src-tauri/`) : `cargo test -p download-core`
Expected : suite verte (cette tâche ne touche pas le cœur ; confirme l'absence de régression). ~186 tests passés.

- [ ] **Step 5 : Commit**

Vérifier la branche `phase-notify-click`, puis :
```bash
git add src-tauri/src/commands.rs
git commit -m "feat(tauri): notif Windows native cliquable -> focus app (on_activated)"
```

---

## Task 4: Vérification finale

- [ ] **Step 1 : Compilation Tauri**

Run (depuis `src-tauri/`) : `cargo check`
Expected : 0 erreur, 0 warning.

- [ ] **Step 2 : Cœur**

Run (depuis `src-tauri/`) : `cargo test -p download-core`
Expected : tout passe (~186).

- [ ] **Step 3 : Front (inchangé mais on confirme)**

Run (racine projet) : `npx tsc --noEmit`
Expected : 0 erreur. (Le front n'est pas modifié ; sanity check.)

- [ ] **Step 4 : Smoke-test sur build installé (manuel, utilisateur)**

`npm run tauri build` → installer le binaire généré → lancer un téléchargement → cliquer la toast « Téléchargement terminé » → la fenêtre FlashGet revient au premier plan (montrée + focus). Idem pour un échec (URL invalide). Vérifier aussi que l'icône/nom de la notif sont bien « FlashGet » (et non PowerShell).

---

## Self-Review

- **Couverture spec :** helper partagé + refactor single_instance (T1), dépendance Windows-only (T2), émission scindée cfg avec on_activated → focus (T3), vérif (T4). ✅ Toutes les sections de la spec ont une tâche. L'icône PowerShell = aucune tâche de code (décision : réglée par l'installation) — couvert par le smoke-test T4 step 4.
- **Placeholders :** aucun — chaque step montre le code/commande réels.
- **Cohérence des noms :** `focus_main_window` (défini T1, utilisé T1 lib.rs + T3 closure), `notify_terminal`, `tauri_winrt_notification::Toast` — identiques partout. ✅
- **Dépendances :** T1 et T2 indépendantes ; T3 dépend de T1+T2 ; T4 = vérif globale. Ordre du plan respecté.
