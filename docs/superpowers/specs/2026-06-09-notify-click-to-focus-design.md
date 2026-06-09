# Clic sur la notif → focus de l'app (Windows) — Design

**Date :** 2026-06-09
**Branche prévue :** `phase-notify-click` (depuis `main`)
**Statut :** validé par l'utilisateur (cfg Windows + plugin ailleurs ; helper focus partagé)
**Suite de :** [[2026-06-09-notify-on-complete-design]] (la notif de fin de téléchargement, déjà mergée)

## Contexte / diagnostic

Deux remarques utilisateur sur la notif de fin de téléchargement :

1. **Icône = PowerShell au lieu de FlashGet.** Comportement **documenté** de `tauri-plugin-notification` sur Windows : *« Only works for installed apps. Shows powershell name & icon in development. »* Sans AppUserModelID (AUMID) enregistré (= raccourci menu Démarrer créé par l'installateur), le plugin retombe sur `Toast::POWERSHELL_APP_ID`. → **Réglé par l'installation**, l'identifiant `com.steph.tauri-app` et les icônes sont déjà configurés. **Aucun code à changer** ; décision utilisateur = on ne build pas maintenant, vérif au prochain build.

2. **Clic sur la notif → devrait ouvrir/focus l'app.** `tauri-plugin-notification` **n'expose pas** la gestion du clic sur desktop (API d'actions `onAction`/`registerActionTypes` = mobile uniquement). → objet de ce design.

## Décision validée

| Question | Choix |
|----------|-------|
| Multi-OS | **cfg Windows** : émission native via `tauri-winrt-notification` (avec clic). **Autres OS** : on garde `tauri-plugin-notification` (sans clic). |
| Helper focus | **Extraire `focus_main_window(app)`** et refactorer le handler `single_instance` pour l'utiliser (DRY). |
| Icône | Pas de gestion explicite ; l'AUMID fournit l'icône de l'app une fois installée. |

## Architecture

### Point d'émission unique scindé par OS — `notify_terminal` (`src-tauri/src/commands.rs`)

La fonction `notify_terminal(app: &AppHandle, status: DownloadStatus, id: &str)` (déjà existante, appelée depuis le relais pour `Completed` et depuis le bras `Err` du moteur pour `Error`) garde son préambule inchangé (vérif `get_notify_on_complete`, résolution du `file_name`, choix `title`/`body`). Seule l'**émission** est scindée :

```rust
    // ... préambule inchangé : early-return si réglage OFF, file_name, (title, body) ...

    #[cfg(target_os = "windows")]
    {
        let app_id = app.config().identifier.clone(); // = AUMID du raccourci installé
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
```

- `Toast::new(app_id)`, `.title(&str)`, `.text1(&str)`, `.show() -> Result<()>` ; `.on_activated<F>(F)` où `F: FnMut(Option<String>) -> Result<()> + Send + 'static`.
- `AppHandle` est `Clone + Send + 'static` → capturable dans le closure.
- `app.config().identifier` (String) = identifiant `tauri.conf.json` = AUMID utilisé par l'installateur NSIS pour le raccourci. Le passer à `Toast::new` rend l'attribution (icône/nom) et le routage de l'activation corrects.

### Helper `focus_main_window` (`src-tauri/src/commands.rs`)

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

Cross-platform (pas de `cfg`) : utilisé par le closure `on_activated` (Windows) ET par le handler `single_instance` (tous OS). Nécessite le trait `tauri::Manager` en scope (déjà utilisé dans commands.rs via `app.state()`).

### Refactor `single_instance` (`src-tauri/src/lib.rs`)

Le handler actuel inline les 3 lignes :
```rust
.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}))
```
devient :
```rust
.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
    commands::focus_main_window(app);
}))
```
(`app` y est un `&AppHandle`.)

### Dépendance (`src-tauri/Cargo.toml`)

Ajouter, en **Windows-only** pour ne pas alourdir les autres cibles :
```toml
[target.'cfg(windows)'.dependencies]
tauri-winrt-notification = "0.7"
```
Déjà résolu en transitif à 0.7.2 (via le plugin) → cohérent. `tauri-plugin-notification` et sa permission `notification:default` **restent** (utilisés sur les autres OS et inoffensifs sur Windows).

## Pourquoi ça marche (et ses limites)

- L'app **tourne toujours** quand elle émet la notif (elle vient de la créer) → le callback `on_activated` se déclenche **in-process** : pas besoin d'un serveur COM d'activation enregistré.
- L'`app_id` = AUMID du raccourci installé → attribution icône/nom + routage de l'activation corrects.
- **Limite (acceptée)** : comme l'icône, ne fonctionne **qu'en build installé**, pas en `tauri dev`. Le `_action: Option<String>` n'est pas utilisé (clic sur le corps, pas de boutons d'action).

## Stratégie de test

- **Pas de test unitaire** : émission et focus = colle Tauri/Windows à effets de bord.
- **`cargo check`** (Windows) doit rester vert ; vérifier aussi que le code `#[cfg(not(windows))]` compile (au moins relecture ; CI non-Windows hors périmètre ici).
- **Smoke-test sur build installé** (manuel, utilisateur) : installer le build → lancer un téléchargement → cliquer la toast « Téléchargement terminé » → la fenêtre FlashGet revient au premier plan ; idem échec.

## Hors périmètre (YAGNI)

- Boutons d'action dans la notif (Ouvrir le fichier / le dossier).
- Action différenciée succès vs échec au clic (les deux ne font que focus).
- Notif cliquable sur Mac/Linux (non build aujourd'hui ; le plugin ne le permet pas).
- Build/installeur (séparé ; à refaire quand voulu).

## Découpage d'implémentation (pour le plan)

1. **Helper** : ajouter `focus_main_window` dans commands.rs + refactorer `single_instance` dans lib.rs (cargo check).
2. **Dépendance** : ajouter `tauri-winrt-notification` Windows-only dans Cargo.toml (cargo check).
3. **Émission scindée** : remplacer le corps d'émission de `notify_terminal` par les deux branches `cfg` (cargo check).

Tâche 1 indépendante ; 3 dépend de 1 (utilise le helper) + 2 (dépendance).
