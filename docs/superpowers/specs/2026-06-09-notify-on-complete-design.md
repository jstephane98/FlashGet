# Notification système de fin de téléchargement — Design

**Date :** 2026-06-09
**Branche prévue :** `phase-notify-complete` (depuis `main`)
**Statut :** validé par l'utilisateur (approche A, completion + erreurs, ON par défaut)

## Objectif

Émettre une notification système native (toast Windows) quand un téléchargement
se **termine** (succès) ou **échoue** (erreur), même lorsque l'application est
réduite dans le system tray. Un réglage permet de couper la fonctionnalité.

## Décisions validées

| Question | Choix retenu |
|----------|--------------|
| Émission | **Approche A** : côté Rust, dans le relais de progression de `launch()`. Marche app réduite/tray. |
| Périmètre | **Completion + erreurs** (deux titres distincts). Pas de notif pour pause/reprise. |
| Réglage par défaut | **ON** (activé sauf si l'utilisateur le coupe). |
| Action au clic | Aucune (YAGNI). |

## Architecture

### 1. Plugin Tauri `tauri-plugin-notification` (nouveau)

Absent aujourd'hui. À câbler :

- **`src-tauri/Cargo.toml`** : `tauri-plugin-notification = "2"` (à côté des autres `tauri-plugin-*`).
- **`src-tauri/src/lib.rs`** : `.plugin(tauri_plugin_notification::init())` dans la chaîne de builder (après les autres `.plugin(...)`).
- **`src-tauri/capabilities/default.json`** : ajouter la permission `"notification:default"` au tableau `permissions`.

### 2. Persistance du réglage (cœur `download-core`)

Suivre **exactement** le pattern existant `cookies_from_browser`
(`src-tauri/crates/download-core/src/storage.rs:294-311`), table `settings` clé/valeur.

- Clé : `notify_on_complete`.
- `pub fn get_notify_on_complete(&self) -> Result<bool, CoreError>` :
  lit la clé ; **défaut ON** → renvoie `true` sauf si la valeur stockée vaut `"0"` ou `"off"`.
- `pub fn set_notify_on_complete(&self, value: bool) -> Result<(), CoreError>` :
  écrit `"1"` (on) / `"0"` (off) en upsert.

Ne **pas** ajouter le champ à la struct `Settings` (`model.rs:47`) : on reste sur
le même modèle clé/valeur que `cookies_from_browser`, qui est exposé hors de `Settings`.

### 3. Commandes Tauri (relais)

Sur le modèle de `youtube::get/set_cookies_from_browser`
(`src-tauri/src/youtube.rs:134-141`, enregistrées dans `lib.rs:124-125`).
Emplacement proposé : un petit module ou à côté des commandes de réglages.

- `get_notify_on_complete(app) -> Result<bool, String>`
- `set_notify_on_complete(app, value: bool) -> Result<(), String>`

Enregistrer les deux dans `invoke_handler![...]` de `lib.rs`.

### 4. Déclenchement (cœur de la feature)

Dans `launch()`, relais de progression
(`src-tauri/src/commands.rs:846-879`), bloc `if persist`.

État actuel du bloc :

```rust
let persist = matches!(p.status, DownloadStatus::Completed | DownloadStatus::Paused | DownloadStatus::Error)
    || (p.status == DownloadStatus::Recording && last_persisted != Some(DownloadStatus::Recording));
if persist {
    // ... update_state ...
    last_persisted = Some(p.status);
}
```

Logique de notif, **à l'intérieur du `if persist`, AVANT `last_persisted = Some(p.status)`**
(pour pouvoir comparer à l'ancienne valeur et ne notifier qu'**une seule fois** par
transition vers un état terminal) :

```rust
let is_terminal_notify = matches!(p.status, DownloadStatus::Completed | DownloadStatus::Error)
    && last_persisted != Some(p.status);
if is_terminal_notify {
    let st = app_for_rx.state::<AppState>();
    if st.storage.get_notify_on_complete().unwrap_or(true) {
        let file_name = st.storage.get_by_id(&id_for_rx).ok().flatten()
            .map(|d| d.file_name)
            .unwrap_or_else(|| id_for_rx.clone());
        let (title, body) = match p.status {
            DownloadStatus::Completed => ("Téléchargement terminé", file_name),
            _ /* Error */            => ("Échec du téléchargement", file_name),
        };
        use tauri_plugin_notification::NotificationExt;
        let _ = app_for_rx.notification().builder().title(title).body(&body).show();
    }
}
```

**Garde anti-doublon :** `last_persisted != Some(p.status)`. `Completed`/`Error`
persistent à chaque message reçu (pas de garde `last_persisted` sur eux dans le code
actuel), donc sans cette comparaison la notif pourrait se répéter si le moteur émet
plusieurs `Progress` avec le même statut terminal. La comparaison se fait **avant**
la mise à jour de `last_persisted` déjà présente à la fin du bloc.

**Récupération du nom :** `storage.get_by_id(&id_for_rx)` → `Download.file_name`.
Repli sur l'id si introuvable (ne devrait pas arriver à ce stade).

**Périmètre couvert :** tout ce qui atteint `Completed`/`Error` via ce relais —
regular, vidéo HLS/DASH VOD, yt-dlp, fin d'enregistrement live. `Recording` et
`Paused` ne notifient pas.

### 5. UI Réglages (`src/components/SettingsDialog.tsx`)

- `src/api.ts` : `getNotifyOnComplete(): Promise<boolean>` et
  `setNotifyOnComplete(v: boolean): Promise<void>` (sur le modèle de
  `getCookiesFromBrowser`/`setCookiesFromBrowser`).
- `SettingsDialog.tsx` :
  - état `const [notify, setNotify] = useState(true);`
  - chargement dans le `useEffect` d'ouverture : `api.getNotifyOnComplete().then(setNotify);`
  - une **case à cocher** « Notifier à la fin d'un téléchargement », icône Lucide
    `Bell` (ajouter à l'import depuis `lucide-react`), `aria-label`.
  - au changement : `setNotify(v); api.setNotifyOnComplete(v);`

## Stratégie de test

- **`download-core` (cargo test)** : roundtrip + défaut de `notify_on_complete`
  (sur le modèle de `cookies_from_browser_roundtrip_and_default_none`,
  `storage.rs:733-741`) — vérifie défaut `true`, set `false` → `false`, set `true` → `true`,
  et que la valeur stockée `"0"`/`"off"` → `false`.
- **Tauri** : l'émission de la notif est de la colle Tauri (non testée unitairement).
- **Frontend (Vitest)** : pas de logique pure nouvelle ; la case est du câblage UI.
- **Smoke-test GUI (manuel)** : lancer un vrai téléchargement court → toast Windows
  « Téléchargement terminé » ; forcer une URL invalide → toast « Échec du téléchargement » ;
  décocher la case → plus de toast.

## Hors périmètre (YAGNI)

- Action au clic sur la notif (focus app, ouvrir fichier).
- Notif pour pause/reprise/mise en file.
- Son personnalisé, icône custom dans la notif, regroupement.
- Rebuild de l'installeur (`npm run tauri build`) — à refaire séparément quand voulu.

## Découpage d'implémentation (pour le plan)

1. **Cœur** : `get/set_notify_on_complete` + test roundtrip (`download-core`).
2. **Câblage plugin** : Cargo.toml + lib.rs `.plugin(...)` + capabilities.
3. **Commandes Tauri** : `get/set_notify_on_complete` + enregistrement `invoke_handler`.
4. **Déclenchement** : bloc notif dans le relais de `launch()`.
5. **UI** : api.ts + case à cocher dans SettingsDialog.

Tâches 1 et 2 sont indépendantes ; 3 dépend de 1+2 ; 4 dépend de 1+2+3 ; 5 dépend de 3.
