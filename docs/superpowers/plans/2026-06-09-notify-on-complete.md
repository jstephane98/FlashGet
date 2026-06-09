# Notification de fin de téléchargement — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Émettre une notification système (toast Windows) quand un téléchargement atteint `Completed` ou `Error`, même app réduite dans le tray, avec un réglage ON par défaut pour la couper.

**Architecture:** Notif émise côté Rust dans le relais de progression de `launch()` via `tauri-plugin-notification`. Réglage persisté en table `settings` (clé/valeur) sur le modèle exact de `cookies_from_browser`. Case à cocher dans `SettingsDialog` sur le modèle d'`autostart` (chargée à l'ouverture, appliquée dans `save()`).

**Tech Stack:** Rust / Tauri v2 (`tauri-plugin-notification` v2), rusqlite (table `settings`), React/TypeScript (api.ts + SettingsDialog.tsx).

**Branche :** `phase-notify-complete` (déjà créée depuis `main` ; la spec y est committée).

---

## File Structure

- **Modify** `src-tauri/crates/download-core/src/storage.rs` — `get/set_notify_on_complete` (pattern `cookies_from_browser`) + test roundtrip.
- **Modify** `src-tauri/Cargo.toml` — dépendance `tauri-plugin-notification`.
- **Modify** `src-tauri/src/lib.rs` — `.plugin(...)` + enregistrement des 2 commandes.
- **Modify** `src-tauri/capabilities/default.json` — permission `notification:default`.
- **Modify** `src-tauri/src/youtube.rs` — 2 commandes `get/set_notify_on_complete` (à côté de leurs jumelles cookies).
- **Modify** `src-tauri/src/commands.rs` — bloc notif dans le relais de `launch()`.
- **Modify** `src/api.ts` — `getNotifyOnComplete` / `setNotifyOnComplete`.
- **Modify** `src/components/SettingsDialog.tsx` — case à cocher (icône Lucide `Bell`).

---

## Task 1: Persistance `notify_on_complete` (cœur download-core)

**Files:**
- Modify: `src-tauri/crates/download-core/src/storage.rs` (après `set_cookies_from_browser`, ~ligne 311 ; test après `cookies_from_browser_roundtrip_and_default_none`, ~ligne 741)

- [ ] **Step 1: Écrire le test qui échoue**

Ajouter dans le module de tests de `storage.rs` (à côté de `cookies_from_browser_roundtrip_and_default_none`) :

```rust
    #[test]
    fn notify_on_complete_roundtrip_and_default_on() {
        let s = Storage::open_in_memory().unwrap();
        // Défaut : activé.
        assert_eq!(s.get_notify_on_complete().unwrap(), true);
        // Désactivation persistée.
        s.set_notify_on_complete(false).unwrap();
        assert_eq!(s.get_notify_on_complete().unwrap(), false);
        // Réactivation persistée.
        s.set_notify_on_complete(true).unwrap();
        assert_eq!(s.get_notify_on_complete().unwrap(), true);
    }
```

- [ ] **Step 2: Lancer le test, vérifier qu'il échoue**

Run: `cargo test -p download-core notify_on_complete_roundtrip_and_default_on`
Expected: FAIL — `no method named get_notify_on_complete` (erreur de compilation).

- [ ] **Step 3: Implémenter les méthodes**

Ajouter dans `impl Storage`, juste après `set_cookies_from_browser` :

```rust
    /// Notif système à la fin d'un téléchargement (clé `notify_on_complete`).
    /// Défaut : activé (true), sauf valeur stockée "0"/"off".
    pub fn get_notify_on_complete(&self) -> Result<bool, CoreError> {
        let conn = self.conn.lock().unwrap();
        let stored: Option<String> = conn
            .query_row("SELECT value FROM settings WHERE key=?1", ["notify_on_complete"], |r| r.get::<_, String>(0))
            .ok();
        Ok(!matches!(stored.as_deref(), Some("0") | Some("off")))
    }

    pub fn set_notify_on_complete(&self, value: bool) -> Result<(), CoreError> {
        let conn = self.conn.lock().unwrap();
        let val = if value { "1" } else { "0" };
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value=?2",
            rusqlite::params!["notify_on_complete", val],
        )
        .map_err(|e| CoreError::Storage(e.to_string()))?;
        Ok(())
    }
```

- [ ] **Step 4: Lancer le test, vérifier qu'il passe**

Run: `cargo test -p download-core notify_on_complete_roundtrip_and_default_on`
Expected: PASS (1 passed).

- [ ] **Step 5: Vérifier la suite cœur complète**

Run: `cargo test -p download-core`
Expected: tout passe (aucune régression).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/crates/download-core/src/storage.rs
git commit -m "feat(core): persistance notify_on_complete (défaut ON)"
```

---

## Task 2: Câbler le plugin `tauri-plugin-notification`

**Files:**
- Modify: `src-tauri/Cargo.toml` (bloc des `tauri-plugin-*`, ~lignes 25-29)
- Modify: `src-tauri/src/lib.rs` (chaîne `.plugin(...)`, ~lignes 26-28)
- Modify: `src-tauri/capabilities/default.json` (tableau `permissions`)

- [ ] **Step 1: Ajouter la dépendance Cargo**

Dans `src-tauri/Cargo.toml`, à la suite des autres `tauri-plugin-*` :

```toml
tauri-plugin-notification = "2"
```

- [ ] **Step 2: Enregistrer le plugin dans le builder**

Dans `src-tauri/src/lib.rs`, ajouter après `.plugin(tauri_plugin_shell::init())` (ligne 28) :

```rust
        .plugin(tauri_plugin_notification::init())
```

- [ ] **Step 3: Ajouter la permission**

Dans `src-tauri/capabilities/default.json`, ajouter `"notification:default"` au tableau `permissions` (après `"dialog:default"`) :

```json
  "permissions": [
    "core:default",
    "opener:default",
    "dialog:default",
    "notification:default",
    {
      "identifier": "shell:allow-execute",
      "allow": [{ "name": "bin/ffmpeg", "sidecar": true, "args": true }]
    }
  ]
```

- [ ] **Step 4: Vérifier que ça compile**

Run: `cargo build -p flashget` (ou `cargo check` dans `src-tauri`)
Expected: compile sans erreur (le crate télécharge `tauri-plugin-notification`).

> Note : si le nom du package binaire diffère, utiliser `cargo check` depuis `src-tauri/`. L'objectif est que le workspace Tauri compile avec le plugin enregistré.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs src-tauri/capabilities/default.json
git commit -m "feat(tauri): câble tauri-plugin-notification (dep + plugin + permission)"
```

---

## Task 3: Commandes Tauri `get/set_notify_on_complete`

**Files:**
- Modify: `src-tauri/src/youtube.rs` (après `set_cookies_from_browser`, ~ligne 141)
- Modify: `src-tauri/src/lib.rs` (`invoke_handler![...]`, après ligne 125)

Dépend de : Task 1 (méthodes storage), Task 2 (compile avec plugin).

- [ ] **Step 1: Ajouter les deux commandes**

Dans `src-tauri/src/youtube.rs`, juste après `set_cookies_from_browser` :

```rust
#[tauri::command]
pub fn get_notify_on_complete(app: AppHandle) -> Result<bool, String> {
    app.state::<AppState>().storage.get_notify_on_complete().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_notify_on_complete(app: AppHandle, value: bool) -> Result<(), String> {
    app.state::<AppState>().storage.set_notify_on_complete(value).map_err(|e| e.to_string())
}
```

- [ ] **Step 2: Enregistrer les commandes**

Dans `src-tauri/src/lib.rs`, dans le `tauri::generate_handler![...]`, après `youtube::set_cookies_from_browser,` (ligne 125) :

```rust
            youtube::get_notify_on_complete,
            youtube::set_notify_on_complete,
```

- [ ] **Step 3: Vérifier que ça compile**

Run: `cargo check` (depuis `src-tauri/`)
Expected: compile sans erreur.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/youtube.rs src-tauri/src/lib.rs
git commit -m "feat(tauri): commandes get/set_notify_on_complete"
```

---

## Task 4: Déclenchement de la notif dans le relais de `launch()`

**Files:**
- Modify: `src-tauri/src/commands.rs` (relais de progression, bloc `if persist`, ~lignes 869-877)

Dépend de : Task 1, 2, 3.

- [ ] **Step 1: Insérer le bloc de notification**

Dans `src-tauri/src/commands.rs`, dans le `if persist { ... }` du relais de `launch()`,
**avant** la ligne `last_persisted = Some(p.status);` (ligne 876), insérer :

```rust
                // Notif système une seule fois par transition vers un état terminal.
                let is_terminal_notify = matches!(p.status, DownloadStatus::Completed | DownloadStatus::Error)
                    && last_persisted != Some(p.status);
                if is_terminal_notify && st.storage.get_notify_on_complete().unwrap_or(true) {
                    let file_name = st.storage.get_by_id(&id_for_rx).ok().flatten()
                        .map(|d| d.file_name)
                        .unwrap_or_else(|| id_for_rx.clone());
                    let (title, body) = match p.status {
                        DownloadStatus::Completed => ("Téléchargement terminé", file_name),
                        _ => ("Échec du téléchargement", file_name),
                    };
                    use tauri_plugin_notification::NotificationExt;
                    let _ = app_for_rx.notification().builder().title(title).body(&body).show();
                }
```

> Le `st` (= `app_for_rx.state::<AppState>()`) est déjà lié juste au-dessus dans le bloc `if persist`
> (utilisé pour `st.storage.update_state(...)`). Réutiliser cette liaison ; ne pas la redéclarer.
> Le bloc doit être placé **après** l'appel `update_state` et **avant** `last_persisted = Some(p.status);`
> pour que la comparaison `last_persisted != Some(p.status)` reflète encore l'état précédent.

- [ ] **Step 2: Vérifier que ça compile**

Run: `cargo check` (depuis `src-tauri/`)
Expected: compile sans erreur. (Pas de test unitaire : colle Tauri.)

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "feat(tauri): notif système à la fin d'un téléchargement (Completed/Error)"
```

---

## Task 5: UI — case à cocher dans les Réglages

**Files:**
- Modify: `src/api.ts` (après `setCookiesFromBrowser`, ~ligne 151)
- Modify: `src/components/SettingsDialog.tsx` (état + useEffect + save + JSX)

Dépend de : Task 3 (commandes Tauri).

- [ ] **Step 1: Ajouter les fonctions api**

Dans `src/api.ts`, après `setCookiesFromBrowser` :

```ts
export function getNotifyOnComplete(): Promise<boolean> {
  return invoke<boolean>("get_notify_on_complete");
}
export function setNotifyOnComplete(value: boolean): Promise<void> {
  return invoke("set_notify_on_complete", { value });
}
```

- [ ] **Step 2: Ajouter l'import de l'icône Lucide**

Dans `src/components/SettingsDialog.tsx`, ligne 6, ajouter `Bell` à l'import :

```ts
import { Settings, Puzzle, X, Check, Video, Bell } from "lucide-react";
```

- [ ] **Step 3: Ajouter l'état**

Après `const [autostart, setAutostart] = useState(false);` (ligne 24) :

```ts
  const [notify, setNotify] = useState(true);
```

- [ ] **Step 4: Charger la valeur à l'ouverture**

Dans le `useEffect` d'ouverture, après `api.getAutostart().then(setAutostart);` (ligne 37) :

```ts
    api.getNotifyOnComplete().then(setNotify);
```

- [ ] **Step 5: Appliquer la valeur dans `save()`**

Dans `save()`, après `await api.setAutostart(autostart);` (ligne 78) :

```ts
    await api.setNotifyOnComplete(notify);
```

- [ ] **Step 6: Ajouter la case à cocher**

Dans le JSX, juste après le bloc `<div>` de l'autostart (le `</div>` qui suit le label « Démarrer avec Windows », ~ligne 111), ajouter un bloc sœur :

```tsx
        <div className="px-4 py-3 border-b border-border">
          <label className="text-sm flex items-center gap-2 text-ink-soft">
            <input
              type="checkbox"
              checked={notify}
              onChange={(e) => setNotify(e.target.checked)}
              aria-label="Notifier à la fin d'un téléchargement"
            />
            <Bell size={16} /> Notifier à la fin d'un téléchargement
          </label>
        </div>
```

- [ ] **Step 7: Vérifier le typage et les tests front**

Run: `npx tsc --noEmit` puis `npx vitest run`
Expected: 0 erreur TS ; tous les tests Vitest passent (aucun test cassé).

- [ ] **Step 8: Commit**

```bash
git add src/api.ts src/components/SettingsDialog.tsx
git commit -m "feat(ui): case « Notifier à la fin d'un téléchargement » (ON par défaut)"
```

---

## Task 6: Vérification finale

- [ ] **Step 1: Suite cœur**

Run: `cargo test -p download-core`
Expected: tout passe.

- [ ] **Step 2: Compilation Tauri**

Run: `cargo check` (depuis `src-tauri/`)
Expected: compile, 0 erreur.

- [ ] **Step 3: Front**

Run: `npx tsc --noEmit && npx vitest run`
Expected: 0 erreur, tests verts.

- [ ] **Step 4: Smoke-test GUI manuel (par l'utilisateur)**

`npm run tauri dev` →
  1. Lancer un téléchargement court → toast « Téléchargement terminé » (nom du fichier).
  2. Lancer une URL invalide → toast « Échec du téléchargement ».
  3. Décocher la case dans Réglages, relancer → plus de toast.

---

## Self-Review

- **Couverture spec :** plugin (T2), persistance défaut ON (T1), commandes (T3), déclenchement Completed+Error avec garde anti-doublon (T4), UI case ON par défaut (T5), tests roundtrip + smoke (T1/T6). ✅ Toutes les sections de la spec ont une tâche.
- **Placeholders :** aucun — chaque step montre le code/commande réels.
- **Cohérence des noms :** `get_notify_on_complete`/`set_notify_on_complete` (Rust), `getNotifyOnComplete`/`setNotifyOnComplete` (TS), clé `notify_on_complete`, état `notify` — identiques d'une tâche à l'autre. ✅
- **Dépendances :** T1 et T2 indépendantes ; T3 dépend de T1+T2 ; T4 de T1+T2+T3 ; T5 de T3. Ordre du plan respecté.
