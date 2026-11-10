# FlashGet v2 — Phase 5 : page Paramètres — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal :** remplacer la boîte de dialogue « Réglages » par la page **Paramètres** de la maquette 03, avec un sous-menu de 8 sections. Ajouter au moteur les réglages qui manquent :
- connexions par fichier ;
- nouvelles tentatives automatiques ;
- reprise au démarrage ;
- conduite si le fichier existe ;
- fermeture vers la zone de notification ;
- notification d'erreur ;
- qualité vidéo par défaut ;
- format et débit audio ;
- langues de sous-titres.

**Architecture :**
- La structure Rust `Settings` grandit. Tous ses champs ont une valeur par défaut, elle est enregistrée clé par clé comme aujourd'hui et validée côté Rust.
- Une commande `save_settings(s)` remplace `update_settings` : elle enregistre tout, applique la limite effective et relance l'ordonnanceur.
- Les réglages sont lus au moment où ils servent (démarrage d'un téléchargement, analyse d'un lien, fermeture de la fenêtre, notification). Ils s'appliquent donc à la prochaine utilisation, sans redémarrer l'application.
- Côté interface, la page enregistre **à chaque changement** (les champs numériques à la sortie du champ), comme la page Planificateur.

**Tech Stack :** Tauri 2 (sidecar yt-dlp, `@tauri-apps/api/app` `getVersion`), Rust, React 19, TypeScript, Tailwind, Vitest.

**Spec :** [`../specs/2026-11-10-flashget-v2-refonte-design.md`](../specs/2026-11-10-flashget-v2-refonte-design.md), §4.8, §5, §8.3, §8.4. Maquette : [`../specs/2026-11-10-flashget-v2-maquettes/03-parametres.html`](../specs/2026-11-10-flashget-v2-maquettes/03-parametres.html).

**Branche :** `flashget-v2-phase-5`, **créée depuis `flashget-v2-phase-4`** (pas encore fusionnée), pour réutiliser l'action de fin de file et le planificateur.

## Global Constraints

- Aucun code repris de NeoDLP, aucune mention d'un tiers dans l'interface (spec §2). La section « À propos » peut citer les composants embarqués (yt-dlp, FFmpeg), puisque leur licence l'exige.
- Aucune capacité actuelle ne doit disparaître ; tout ce que fait la boîte de dialogue « Réglages » doit se retrouver dans la page :
  - thème ;
  - téléchargements simultanés et limite de vitesse ;
  - démarrage avec Windows ;
  - notification de fin ;
  - dossiers par catégorie ;
  - extension (état, port, jeton, copier, régénérer) ;
  - compte YouTube (connexion, déconnexion) ;
  - cookies d'un navigateur.
- L'action de fin de file reste **un seul réglage**, partagé avec le Planificateur (`get_end_action` / `set_end_action`) — spec §8.4.
- Langue : le sélecteur reste **masqué** tant qu'il n'y a qu'une langue (spec §8.3).
- Valeurs par défaut identiques au comportement actuel, pour qu'une mise à jour ne change rien sans action de l'utilisateur. Il y a deux exceptions voulues :
  - les **3 nouvelles tentatives** (il n'y en a aucune aujourd'hui) ;
  - la **reprise au démarrage** des lignes restées « en cours » (aujourd'hui, elles bloquent une place jusqu'au prochain démarrage manuel).

  Voici les valeurs par défaut :

| Réglage | Défaut |
|---|---|
| Connexions par fichier | 8 |
| Nouvelles tentatives | 3 |
| Reprise au démarrage | oui |
| Conflit de nom | renommer |
| Fermer = réduire | oui |
| Notification de fin | oui |
| Notification d'erreur | oui |
| Qualité vidéo préférée | 1080 |
| Format audio | MP3, qualité maximale |
| Sous-titres | `["fr", "en"]` |

- Tout texte visible en français via `t()` ; chiffres en JetBrains Mono ; jetons Tailwind uniquement.
- `cargo` depuis PowerShell, avec `-j 1` (machine à court de mémoire) ; `vitest --maxWorkers=2`.

## Review Focus

1. **Base existante sans les nouvelles clés** (mise à jour depuis la phase 4) : `get_settings` renvoie les valeurs par défaut ci-dessus. Aucun comportement ne change tant que l'utilisateur n'a rien modifié. Test : Task 1 `settings_defaults_for_missing_keys`.
2. **Nouvelles tentatives** : jamais après une pause, une annulation, une erreur disque (fichier impossible à écrire) ni une erreur HTTP 4xx. On réessaie seulement sur une erreur réseau ou une erreur 5xx, avec un délai croissant (2, 4, 8… s, plafonné à 30 s). Le téléchargement reprend là où il s'était arrêté. Test : Task 2 `retry_policy`.
3. **Reprise au démarrage** : les lignes restées « en cours » après un plantage ne doivent plus occuper de place pour toujours. Avec la reprise activée, elles repartent ; désactivée, elles passent en pause. Une pause demandée par l'utilisateur n'est jamais relancée. Test : Task 2 `startup_recovery`.
4. **« Remplacer » si le fichier existe** : l'ancien fichier part à la **Corbeille** au moment de l'ajout, jamais effacé définitivement. Le moteur ne doit jamais prendre le fichier existant pour un téléchargement partiel à reprendre. Un autre téléchargement prévu au même chemin reste protégé : on renomme dans ce cas. Test : Task 3 `conflict_policy`.
5. **Valeurs hors limites** (0 connexion, 99 tentatives, débit audio inconnu, liste de sous-titres vide) : refusées par le Rust avec un message en français, que la page affiche. Le réglage précédent reste en place. Tests : Task 1 `validate_settings`, Task 5 `erreur de validation`.

---

## Décisions techniques

- **`Settings` (Rust, `download-core/src/model.rs`)**, tous les champs en `#[serde(default)]` :

| Champ | Type | Défaut | Contrainte |
|---|---|---|---|
| `max_concurrent` | `u32` | 3 | 1 à 10 |
| `max_speed_bps` | `u64` | 0 | 0 = aucune limite |
| `connections` | `u32` | 8 | 1 à 32 |
| `retries` | `u32` | 3 | 0 à 10 |
| `auto_resume` | `bool` | `true` | |
| `conflict` | `ConflictPolicy` (`rename` / `overwrite`) | `rename` | |
| `close_to_tray` | `bool` | `true` | |
| `notify_complete` | `bool` | `true` | |
| `notify_error` | `bool` | `true` | |
| `video_max_height` | `u32` | 1080 | 2160, 1440, 1080, 720, 480 ou 360 |
| `audio_format` | `AudioFormat` (`mp3` / `m4a` / `opus`) | `mp3` | |
| `audio_kbps` | `u32` | 0 | 0 = qualité maximale, sinon 128, 192, 256 ou 320 |
| `sub_langs` | `Vec<String>` | `["fr", "en"]` | 1 à 6 codes de 2 lettres |

  - **Enregistrement** : une clé par champ dans la table `settings`, en reprenant les clés `max_concurrent` et `max_speed_bps` existantes. `notify_complete` reprend la clé existante `notify_on_complete`, pour ne pas perdre le choix de l'utilisateur.
  - **Démarrage avec Windows** : il reste géré par le registre (commandes `get_autostart` / `set_autostart` existantes), pas par `Settings`.
- **Nouvelles tentatives** : boucle dans la tâche de `launch`.
  - Avant de déclarer l'erreur, si `should_retry(&err, attempt, settings.retries)`, on attend `retry_delay(attempt)` puis on relance le moteur, qui reprend grâce au fichier de reprise.
  - Pendant l'attente, le statut reste « en cours ». Une pause ou une annulation pendant l'attente arrête les tentatives.
  - Pour yt-dlp, une tentative relance la commande, qui reprend avec `--continue`.
- **Démarrage** : `recover_interrupted_downloads(app)`, appelée juste après `recover_interrupted_recordings`, fait passer chaque ligne `downloading` en `queued` si `auto_resume`, sinon en `paused`.
- **Conflit « remplacer »** : dans `confirm_link` et dans les deux chemins d'ajout groupé.
  - Si le nom choisi existe **sur le disque** et qu'aucun autre téléchargement ne vise ce chemin, l'ancien fichier part à la Corbeille avant l'enregistrement de la nouvelle ligne, et le fichier de reprise éventuel (`.resume`) est supprimé.
  - Sinon, on renomme comme aujourd'hui.
  - Si la Corbeille échoue, on renomme et on le signale dans le résumé ; on ne bloque pas.
- **Qualité vidéo** : `build_media_formats(formats, max_height)` recommande la plus grande hauteur ≤ `max_height`, ou la plus petite disponible si toutes la dépassent. `analyze` passe `settings.video_max_height`.
- **Audio** :
  - La ligne de conversion ne s'appelle plus `"mp3"` mais `format_id: "convert"`, avec `label` = `"MP3"`, `"M4A"` ou `"Opus"` (plus `" · {kbps} kb/s"` si `audio_kbps > 0`) et `container` = le format en majuscules.
  - `YtdlpJob` gagne `audio_format: Option<String>` et `audio_kbps: Option<u32>`. `job_args` produit `-x --audio-format {fmt} --audio-quality {kbps}K` (ou `0` si `kbps = 0`).
  - Les anciennes tâches `convert_mp3` gardent `mp3` en qualité 0.
  - Côté interface, `outputExtension` donne `"." + container` en minuscules pour `format_id === "convert"`. On met à jour `selection.ts`, ses tests et `plan_download`. Le reste de la logique est inchangé.
- **Sous-titres** : `plan_download` utilise `settings.sub_langs` au lieu de `PREFERRED_SUB_LANGS`. La constante reste comme valeur par défaut.
- **Fermeture** : le gestionnaire `CloseRequested` de `lib.rs` lit `close_to_tray`. Si c'est vrai, la fenêtre est cachée ; sinon, `app.exit(0)`.
- **Notifications** : `notify_terminal` vérifie `notify_complete` pour les fins réussies et `notify_error` pour les échecs.
- **Version de yt-dlp** : commande `get_ytdlp_version()`, qui lance le sidecar avec `--version`, met le résultat en cache pour la session et renvoie `Option<String>`. Version de FlashGet : `getVersion()` de `@tauri-apps/api/app`.
- **Page** : sous-menu à gauche (8 entrées), section active à droite, sur le modèle de la maquette 03. Chaque section est un composant à part. La page lit tout au montage. Chaque contrôle enregistre aussitôt ; les champs numériques et les heures enregistrent à la sortie du champ ou sur Entrée. Un refus ramène l'affichage à la dernière valeur enregistrée et affiche l'erreur en haut de la section.
- **Mises à jour** (section 7) : en phase 5, elle affiche seulement la version de FlashGet et celle de yt-dlp, avec le texte `t("set.updatesSoon")` (« Les mises à jour automatiques arriveront dans une prochaine version. »). Le reste viendra en phase 6.
- **Extension** (section 5) : état d'écoute, port, jeton (copier, régénérer), et une courte aide d'installation. Les sites ignorés viendront en phase 7.

## Fichiers

| Fichier | Rôle |
|---|---|
| `download-core/src/model.rs`, `storage.rs` | `Settings` étendu, `validate_settings` |
| `download-core/src/engine.rs` | nombre de segments en paramètre |
| `download-core/src/ytdlp.rs` | `build_media_formats(.., max_height)`, conversion audio |
| `src-tauri/src/retry.rs` (nouveau) | `should_retry`, `retry_delay` |
| `src-tauri/src/commands.rs`, `link.rs`, `dest.rs`, `lib.rs`, `ytdlp.rs` | branchements |
| `src/api.ts`, `src/types.ts`, `src/newlink/selection.ts` | types, appels, extension de sortie |
| `src/pages/SettingsPage.tsx`, `src/components/settings/*.tsx` (nouveaux) | page et sections |
| `src/App.tsx` | page branchée, boîte de dialogue supprimée |

---

### Task 1 : Réglages étendus et validés

**Files :** `download-core/src/model.rs`, `storage.rs` (+tests) ; `src-tauri/src/commands.rs` (`save_settings` remplace `update_settings`), `src-tauri/src/lib.rs` ; `src/types.ts`, `src/api.ts`.

**Interfaces :**
- `Settings` et les deux enums, comme dans le tableau ci-dessus, en `#[serde(rename_all = "snake_case")]` ; `impl Default`.
- `pub fn validate_settings(s: &Settings) -> Result<(), String>`, avec ces messages :

| Cas refusé | Message |
|---|---|
| `max_concurrent` hors 1 à 10 | « Téléchargements simultanés : entre 1 et 10 » |
| `connections` hors 1 à 32 | « Connexions par fichier : entre 1 et 32 » |
| `retries` au-delà de 10 | « Nouvelles tentatives : entre 0 et 10 » |
| `video_max_height` hors liste | « Qualité vidéo inconnue » |
| `audio_kbps` hors liste | « Débit audio inconnu » |
| `sub_langs` vide, plus de 6 codes, ou code qui n'est pas en 2 lettres minuscules | « Langues de sous-titres invalides » |

- `get_settings()` et `set_settings(&Settings)` lisent et écrivent chaque clé. Une clé absente ou illisible prend la valeur par défaut. `notify_complete` utilise la clé `notify_on_complete`. Les listes et les enums sont stockés en JSON ou en texte simple.
- Commande `save_settings(app, s: Settings) -> Result<Settings, String>` : `validate_settings`, puis `set_settings`, puis `queue::apply_effective_rate`, puis `reevaluate`. Elle renvoie les réglages relus.
- `update_settings` est supprimée, avec son enregistrement dans `generate_handler!` et `api.updateSettings`. Seule la boîte de dialogue l'utilisait ; elle est supprimée à la Task 7 et adaptée temporairement entre-temps (cf. Step 3).
- `get_notify_on_complete` et `set_notify_on_complete` restent pour l'instant (Task 7 les retire).
- TS : `type ConflictPolicy = "rename" | "overwrite"`, `type AudioFormat = "mp3" | "m4a" | "opus"`, `interface Settings {…}` (miroir exact) et `saveSettings(s): Promise<Settings>` → `invoke("save_settings", { s })`.

- [ ] **Step 1 : Tests qui échouent**

```rust
#[test] fn settings_defaults_for_missing_keys() { let s = Storage::open_in_memory().unwrap(); let g = s.get_settings().unwrap(); assert_eq!(g, Settings::default()); assert_eq!((g.connections, g.retries, g.video_max_height), (8, 3, 1080)); assert_eq!(g.sub_langs, vec!["fr","en"]); }
#[test] fn settings_roundtrip_all_fields() { /* valeurs non par défaut partout → set puis get → égal */ }
#[test] fn legacy_notify_key_is_reused() { /* set_notify_on_complete(false) via l'ancien helper → get_settings().notify_complete == false */ }
#[test] fn validate_settings_cases() { /* chaque refus du tableau → Err avec son message ; Default → Ok */ }
```

- [ ] **Step 2 : Vérifier l'échec** — `cargo test --manifest-path src-tauri/Cargo.toml -p download-core -j 1`.
- [ ] **Step 3 : Implémenter.** `SettingsDialog.tsx` utilise encore `updateSettings` : remplace cet appel par `saveSettings({ ...reglagesLus, max_concurrent, max_speed_bps })` pour garder la boîte fonctionnelle jusqu'à la Task 7.
- [ ] **Step 4 : Vérifier** — cargo workspace `-j 1`, `npm test -- --maxWorkers=2`, `npx tsc --noEmit`.
- [ ] **Step 5 : Commit** — `feat(core): réglages étendus et validés (save_settings)`

---

### Task 2 : Connexions, nouvelles tentatives, reprise au démarrage, fermeture, notifications

**Files :** create `src-tauri/src/retry.rs` (+tests) ; modify `download-core/src/engine.rs` (+test), `src-tauri/src/commands.rs` (`launch`, `notify_terminal`, `recover_interrupted_downloads`), `src-tauri/src/lib.rs`, `src-tauri/src/ytdlp.rs` si besoin.

**Interfaces :**
- `engine::download(.., segments: u32)` remplace `DEFAULT_SEGMENTS`. La constante reste comme valeur par défaut et dans les tests. `launch` passe `settings.connections`, lu au démarrage du téléchargement.
- `retry.rs` :
  - `pub fn should_retry(err: &CoreError, attempt: u32, max: u32) -> bool` : `attempt < max` ET l'erreur est réseau, délai dépassé ou HTTP ≥ 500. Jamais pour une erreur disque (`Io`), une erreur HTTP 4xx, une annulation ou une pause. Adapter au type d'erreur réel de `CoreError` ; si la distinction n'existe pas, ajouter une méthode `CoreError::is_transient()` dans download-core, avec des tests.
  - `pub fn retry_delay(attempt: u32) -> Duration` : `min(2^(attempt+1), 30)` secondes, donc 2, 4, 8, 16, 30…
  - Pour yt-dlp : `should_retry_ytdlp(exit_ok: bool, paused_or_cancelled: bool, attempt, max) -> bool` (toute sortie en échec qui n'est ni une pause ni une annulation).
- `launch` : boucle de tentatives autour de l'appel du moteur et autour de `run_download` (yt-dlp). Entre deux tentatives, `tokio::time::sleep(retry_delay(n))`, interrompu si le handle est mis en pause ou annulé (vérification toutes les 250 ms). Le message d'erreur final mentionne le nombre de tentatives (« … (après 3 tentatives) »).
- `pub fn recover_interrupted_downloads(app)` : pour chaque ligne `downloading` sans handle, `queued` si `auto_resume`, sinon `paused`. Appelée dans `setup` juste après `recover_interrupted_recordings`, avant la boucle du planificateur. Elle s'appuie sur une fonction pure `startup_status(auto_resume: bool) -> DownloadStatus`, testée.
- `lib.rs` `CloseRequested` : lit `get_settings().close_to_tray` à chaque fermeture (vrai par défaut si la lecture échoue). Si c'est vrai, `prevent_close` puis `hide`, comme aujourd'hui ; sinon `app.exit(0)`.
- `notify_terminal` : `Completed` dépend de `notify_complete`, `Error` de `notify_error`.

- [ ] **Step 1 : Tests qui échouent** :
  - `retry_policy` : un délai réseau aux tentatives 0 et 2 sur 3 → vrai ; tentative 3 sur 3 → faux ; `Io` → faux ; HTTP 404 → faux ; HTTP 503 → vrai ; `max = 0` → toujours faux ;
  - `retry_delays` : 2, 4, 8, 16, 30, 30 ;
  - `startup_recovery` : `startup_status(true) == Queued`, `startup_status(false) == Paused` ;
  - dans engine.rs : `download(.., segments = 4)` produit 4 `parts`.
- [ ] **Step 2 à 4 :** échec, implémentation, puis cargo workspace `-j 1` et `cargo build -j 1`.
- [ ] **Step 5 : Commit** — `feat: connexions réglables, nouvelles tentatives, reprise au démarrage, fermeture et notifications`

---

### Task 3 : Qualité vidéo, audio, sous-titres et conflit de nom

**Files :** `download-core/src/ytdlp.rs` (+tests), `src-tauri/src/link.rs` (+tests), `src-tauri/src/dest.rs` (+tests), `src-tauri/src/commands.rs` (ajout groupé), `src/newlink/selection.ts` (+tests), `src/components/newlink/*` si un test dépend de `"mp3"`.

**Interfaces :**
- `build_media_formats(formats, max_height: u32)` :
  - recommande la plus grande hauteur ≤ `max_height`, ou la plus petite hauteur si toutes la dépassent (corrige au passage l'absence de format recommandé au-delà de 1080p) ;
  - la ligne de conversion devient `{ format_id: "convert", label, container }` selon `audio_format` et `audio_kbps`, toujours recommandée dans l'onglet Audio. La signature reçoit donc aussi `(audio_format, audio_kbps)` (ou une petite structure `MediaPrefs { max_height, audio_format, audio_kbps }`).
- `YtdlpJob` gagne `#[serde(default)] audio_format: Option<String>` et `audio_kbps: Option<u32>`.
  - `job_args` en Audio avec conversion : `-f bestaudio/best -x --audio-format {fmt} --audio-quality {kbps}K`, ou `0` si `kbps = 0` ou absent.
  - `convert_mp3: true` sans `audio_format` (anciennes tâches) donne `mp3`, qualité 0, comme avant.
- `plan_download` :
  - `format_id == "convert"` donne un job audio avec conversion au format des réglages ; extension `.{fmt}` (mp3, m4a ou opus) ; catégorie Musique ;
  - sous-titres : `pick_sub_langs(&info.subtitle_langs, &settings.sub_langs)`.

  Les réglages sont lus par `confirm_link` et passés à la fonction pure.
- `dest.rs` : `pub fn resolve_conflict(dir, name, taken, policy, exists_on_disk) -> ConflictResolution { Use(name), Rename(new_name), ReplaceExisting(path) }`, testée.
  - `ReplaceExisting` seulement si `policy == Overwrite`, que le fichier existe sur le disque et qu'il n'est pas dans `taken`.
  - `confirm_link` et l'ajout groupé appliquent ce résultat : `ReplaceExisting` envoie le fichier à la Corbeille (crate `trash`, déjà présente) et supprime le `.resume` ; si la Corbeille échoue, on renomme.
- TS `selection.ts` : `outputExtension(f)` donne `"." + container.toLowerCase()` pour `format_id === "convert"`. Les tests qui utilisaient `"mp3"` comme identifiant de la ligne de conversion sont adaptés.

- [ ] **Step 1 : Tests qui échouent** :
  - `recommended_respects_max_height` : 720 donne 720p recommandé ; 2160 donne 2160p ; formats tous au-dessus de 1080 donnent la plus petite hauteur ;
  - `convert_row_follows_audio_prefs` : m4a à 256 donne le libellé `"M4A · 256 kb/s"` et le conteneur `"M4A"` ;
  - `job_args_audio_formats` : opus 0 donne `-x --audio-format opus --audio-quality 0` ; ancienne tâche `convert_mp3` donne mp3 0 ;
  - `plan_convert_uses_settings` : extension `.opus` et catégorie Musique ;
  - `subs_from_settings` : réglage `["de"]` avec `["en", "de"]` disponibles donne `["de"]` ;
  - `conflict_policy` : les quatre cas de `resolve_conflict` (renommer, utiliser, remplacer, chemin déjà prévu par un autre téléchargement donc renommer même en « remplacer ») ;
  - TS : `outputExtension` d'une ligne `convert` M4A donne `.m4a`.
- [ ] **Step 2 à 4 :** échec, implémentation, puis cargo workspace `-j 1`, `npm test -- --maxWorkers=2`, `tsc`.
- [ ] **Step 5 : Commit** — `feat: qualité vidéo, format audio, sous-titres et conflit de nom réglables`

---

### Task 4 : Versions de yt-dlp et de FlashGet

**Files :** `src-tauri/src/ytdlp.rs` (commande + test de l'analyse du texte), `src-tauri/src/lib.rs`, `src/api.ts`.

**Interfaces :**
- `pub fn parse_version(stdout: &str) -> Option<String>` : première ligne non vide, nettoyée (ex. `"2026.10.30\r\n"` donne `"2026.10.30"`).
- Commande `get_ytdlp_version(app) -> Option<String>` : `app.shell().sidecar("yt-dlp").args(["--version"]).output()`, puis `parse_version`. Résultat mis en cache dans un `OnceLock`/`Mutex<Option<String>>` d'`AppState` pour la session. Une erreur donne `None`, sans plantage.
- TS : `getYtdlpVersion(): Promise<string | null>` ; `appVersion(): Promise<string>` via `getVersion()` de `@tauri-apps/api/app`.

- [ ] **Step 1 : Tests qui échouent** (`parse_version` : avec `\r\n`, vide, plusieurs lignes) **à Step 5 : Commit** — `feat: versions de yt-dlp et de FlashGet`

---

### Task 5 : Page Paramètres — structure, Général, Téléchargements, Dossiers

**Files :** create `src/pages/SettingsPage.tsx`, `src/components/settings/SettingsNav.tsx`, `GeneralSection.tsx`, `DownloadsSection.tsx`, `FoldersSection.tsx`, `controls.tsx` (contrôles partagés : `SettingRow`, `Switch`, `NumberField` qui enregistre à la sortie du champ, `Select`), `src/pages/SettingsPage.test.tsx` ; modify `src/i18n/fr.ts`.

**Interfaces :**
- `type SectionId = "general" | "downloads" | "folders" | "video" | "extension" | "notifications" | "updates" | "about"`.
- `SettingsPage({ onSaved(s: Settings): void })` :
  - à gauche, le sous-menu `SettingsNav` (titre « Paramètres », 8 entrées avec icône lucide, section active sur `surface`) ;
  - à droite, la section active, sous la forme titre, description, puis cartes ;
  - au montage, lit `getSettings()`, puis passe `settings`, `save(patch)` et `error` aux sections. `save(patch)` fusionne le patch, appelle `saveSettings`, met à jour l'état avec la réponse et appelle `onSaved`. En cas de refus, l'affichage revient au dernier état enregistré et l'erreur s'affiche en haut de la section ;
  - si le chargement échoue : message `t("set.loadError")` et bouton « Réessayer ».
- **Général** : carte Apparence (Thème Sombre / Clair / Système via `useTheme`) et carte Démarrage :
  - « Lancer FlashGet au démarrage de Windows » (`getAutostart` / `setAutostart`) ;
  - « Fermer = réduire dans la zone de notification » (`close_to_tray`).
- **Téléchargements** :
  - carte Vitesse et parallélisme : simultanés (1 à 10), connexions par fichier (curseur de 1 à 32 et sa valeur en mono), limite de vitesse (interrupteur et champ Mo/s, enregistré à la sortie du champ, virgule acceptée) ;
  - carte Fiabilité : nouvelles tentatives (0 à 10), reprise au démarrage, « Fichier déjà existant » (Renommer / Remplacer, avec la note `t("set.overwriteNote")` : « L'ancien fichier part à la Corbeille. ») ;
  - carte Fin des téléchargements : sélecteur de l'action de fin (`getEndAction` / `setEndAction`), avec les libellés `end.*` existants.
- **Dossiers** : une ligne par catégorie (Vidéos, Documents, Programmes, Archives, Musique, Autres) avec le chemin en mono, « Parcourir… » (`open({ directory: true })` puis `setCategoryDir`) et « Par défaut » (`setCategoryDir(cat, null)`). Le texte d'aide est `t("set.foldersHint")` (« Sans dossier choisi, FlashGet utilise le dossier Téléchargements de Windows. »).
- Textes : préfixe `set.*` pour toutes les étiquettes ; reprendre les textes de la maquette 03 mot pour mot quand ils existent.

- [ ] **Step 1 : Tests qui échouent** :
  - menu à 8 entrées et section Général par défaut ;
  - le thème change avec `setPref` ;
  - « Fermer = réduire » appelle `saveSettings` avec `close_to_tray: false` ;
  - connexions : déplacer le curseur sur 16 appelle `saveSettings` avec `connections: 16` ;
  - limite « 2,5 » puis sortie du champ donne `max_speed_bps: 2621440` ;
  - `erreur de validation` : `saveSettings` refuse, le message s'affiche et la valeur affichée revient ;
  - « Remplacer » affiche la note sur la Corbeille ;
  - action de fin via `setEndAction` ;
  - dossiers : « Parcourir » appelle `setCategoryDir("video", "D:/V")`, et « Par défaut » appelle `setCategoryDir(…, null)` ;
  - échec de chargement suivi de « Réessayer ».
- [ ] **Step 2 à 4 :** échec, implémentation, puis `npm test -- --maxWorkers=2` et `tsc`.
- [ ] **Step 5 : Commit** — `feat(ui): page Paramètres (structure, Général, Téléchargements, Dossiers)`

---

### Task 6 : Sections Vidéo et yt-dlp, Extension, Notifications, Mises à jour, À propos

**Files :** create `src/components/settings/VideoSection.tsx`, `ExtensionSection.tsx`, `NotificationsSection.tsx`, `UpdatesSection.tsx`, `AboutSection.tsx`, `src/components/settings/sections.test.tsx` ; modify `src/pages/SettingsPage.tsx`, `src/i18n/fr.ts`.

**Interfaces :**
- **Vidéo et yt-dlp** :
  - carte Moteur : `yt-dlp {version}` en mono, ou `t("set.ytdlpUnknown")` ;
  - carte Formats par défaut :
    - qualité préférée (2160p, 1440p, 1080p, 720p, 480p, 360p) ;
    - format audio (MP3, M4A, Opus en boutons segmentés) ;
    - débit (Maximum, 320, 256, 192, 128 kb/s) ;
    - sous-titres : cases à cocher pour fr, en, es, de, it, pt, ar, ja, avec au moins une cochée ; si on décoche la dernière, la case reste cochée et rien n'est enregistré ;
  - carte Comptes : compte YouTube (état Connecté / Non connecté, Se connecter / Se déconnecter, écoute de `onYoutubeStatus`) et « Cookies d'un navigateur » (Aucun, Chrome, Edge, Firefox → `setCookiesFromBrowser`).
- **Extension navigateur** :
  - état (`getBridgeInfo().listening` : « À l'écoute sur le port {port} » ou « Inactive ») ;
  - jeton en mono, avec « Copier » (`navigator.clipboard.writeText`, puis `t("set.copied")`) et « Régénérer » (`regenerateBridgeToken`, après une confirmation `t("set.regenConfirm")` : « L'extension devra être reconnectée avec le nouveau jeton. Continuer ? ») ;
  - aide d'installation `t("set.extHelp")` (le dossier `extension` fourni avec FlashGet se charge dans le navigateur en « mode développeur », puis le jeton se colle dans les options de l'extension).
- **Notifications** : « À la fin d'un téléchargement » (`notify_complete`) et « En cas d'échec » (`notify_error`).
- **Mises à jour** : versions de FlashGet (`appVersion()`) et de yt-dlp, plus `t("set.updatesSoon")`.
- **À propos** : logo, « FlashGet », version, une courte phrase de description, et « Composants intégrés : yt-dlp, FFmpeg, aria2 » (ne citer que ceux réellement embarqués : vérifier `tauri.conf.json` `externalBin`).

- [ ] **Step 1 : Tests qui échouent** :
  - qualité 720p dans `saveSettings` ;
  - format M4A, puis débit 256 ;
  - décocher la dernière langue n'enregistre rien ;
  - cocher « de » ajoute « de » à `sub_langs` ;
  - YouTube : connexion et déconnexion ;
  - cookies Edge ;
  - « Copier » le jeton ;
  - « Régénérer » avec confirmation (annuler n'appelle rien) ;
  - notifications d'erreur à `false` ;
  - affichage des versions dans Mises à jour et dans À propos.
- [ ] **Step 2 à 4 :** échec, implémentation, puis `npm test -- --maxWorkers=2` et `tsc`.
- [ ] **Step 5 : Commit** — `feat(ui): sections Vidéo, Extension, Notifications, Mises à jour, À propos`

---

### Task 7 : Intégration et suppression de la boîte de dialogue

**Files :** modify `src/App.tsx`, `src/api.ts`, `src-tauri/src/youtube.rs`, `src-tauri/src/lib.rs` ; delete `src/components/SettingsDialog.tsx`.

**Interfaces :**
- `go("settings")` donne `setPage("settings")`, qui rend `<SettingsPage onSaved={(s) => { setMaxSpeedBps(s.max_speed_bps); setMaxConcurrent(s.max_concurrent); }} />`. Le menu met « Paramètres » en surbrillance.
- On supprime `SettingsDialog`, son état et son rendu.
- On retire les commandes `get_notify_on_complete` et `set_notify_on_complete`, et leurs fonctions TS, si plus rien ne les utilise (vérifier avec grep).
- La barre d'état reste juste, car elle est mise à jour par `onSaved`.

- [ ] **Step 1 : Implémenter**, puis grep : `SettingsDialog`, `updateSettings`, `update_settings` et `NotifyOnComplete` ne renvoient plus rien.
- [ ] **Step 2 : Vérifier** — `npm test -- --maxWorkers=2`, `npx tsc --noEmit`, `npm run build`, cargo workspace `-j 1`, `cargo build -j 1`.
- [ ] **Step 3 : Commit** — `feat(ui): page Paramètres branchée, boîte de dialogue supprimée`
- [ ] **Step 4 : Vérification manuelle** (par le propriétaire, `npm run tauri dev`, après avoir fermé tout FlashGet et tout Vite sur le port 1420) :
  1. Chacune des 8 sections s'affiche, et chaque changement est conservé après avoir quitté puis relancé FlashGet.
  2. Connexions à 16 : un gros fichier affiche 16 segments.
  3. Couper le réseau pendant un téléchargement : il réessaie, puis reprend quand le réseau revient.
  4. « Fermer = réduire » désactivé : la croix quitte vraiment FlashGet.
  5. Qualité 720p : la page Nouveau recommande 720p. Format Opus : la ligne de conversion affiche « Opus » et le fichier final est un `.opus`.
  6. « Remplacer » : re-télécharger un fichier existant envoie l'ancien à la Corbeille.
- [ ] **Step 5 : Fin de phase** — `superpowers:finishing-a-development-branch`.
