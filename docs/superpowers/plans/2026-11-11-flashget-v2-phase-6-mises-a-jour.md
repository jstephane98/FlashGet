# FlashGet v2 — Phase 6 : mises à jour et installeur — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal :** FlashGet se met à jour seul. Cela couvre :
- yt-dlp, mis à jour depuis l'application, à la main ou automatiquement ;
- FlashGet lui-même, via l'updater Tauri et GitHub Releases ;
- un installeur NSIS par utilisateur, avec son nouvel identifiant et la migration des données ;
- un script local qui fabrique une version publiable.

**Architecture :**
- Toute la logique réseau et fichiers est en Rust. On ajoute deux modules :
  - `ytdlp_update.rs` : vérifier la dernière version, télécharger, contrôler la somme SHA-256, puis remplacer ;
  - `app_update.rs` : vérifier, télécharger et installer avec `tauri-plugin-updater`.
- Le yt-dlp mis à jour est rangé dans `{app_data}/bin/yt-dlp.exe`. Il est utilisé **en priorité** sur le sidecar fourni. Tous les lancements de yt-dlp passent par un seul assistant, `ytdlp_command`.
- Au démarrage, une tâche de fond fait deux vérifications :
  - yt-dlp, au plus une fois par 24 h ;
  - FlashGet, à chaque démarrage, si le réglage est actif.

  Elle prévient l'interface par des événements. L'interface affiche alors un point ambre sur « Paramètres » et sur la section concernée.
- Installer une mise à jour de FlashGet se fait ainsi : on met en pause les téléchargements actifs, on note leurs identifiants, puis on lance l'installeur. FlashGet se ferme, se rouvre, et remet ces téléchargements dans la file.
- La publication se fait **en local** : `npm run release` construit l'installeur signé et produit `latest.json`. Le propriétaire dépose les fichiers dans une Release GitHub.

**Tech Stack :** Tauri 2.11, `tauri-plugin-updater` 2, `reqwest` (déjà utilisé par download-core), `sha2`, React 19, TypeScript, Tailwind, Vitest, Node (script de publication).

**Spec :** [`../specs/2026-11-10-flashget-v2-refonte-design.md`](../specs/2026-11-10-flashget-v2-refonte-design.md), §4.8 (sections 4 et 7), §5 (mise à jour yt-dlp, mise à jour FlashGet, identifiant), §7.3, §8.7, §8.8. Maquettes : [`03-parametres.html`](../specs/2026-11-10-flashget-v2-maquettes/03-parametres.html) (ligne yt-dlp) et [`05-mises-a-jour.html`](../specs/2026-11-10-flashget-v2-maquettes/05-mises-a-jour.html) (section Mises à jour et états de la ligne yt-dlp ; **à valider par le propriétaire avant les tâches 5 et 6**).

**Branche :** `flashget-v2-phase-6`, **créée depuis `flashget-v2-phase-5`** (pas encore fusionnée).

## Décisions du propriétaire (2026-11-11)

| Sujet | Décision |
|---|---|
| Identifiant | `com.jstephane98.flashget` (ancien : `com.steph.tauri-app`) |
| Publication | Script local `npm run release` ; dépôt manuel dans une Release GitHub de `jstephane98/FlashGet` |
| Version | `2.0.0` |
| Nom du programme | `FlashGet.exe` (au lieu de `tauri-app.exe`) |

## Décisions techniques

| Sujet | Valeur |
|---|---|
| Point d'accès de l'updater | `https://github.com/jstephane98/FlashGet/releases/latest/download/latest.json` |
| Mode d'installation Windows | `plugins.updater.windows.installMode = "passive"` |
| Installeur | `bundle.targets = ["nsis"]`, `bundle.windows.nsis.installMode = "currentUser"`, `languages = ["French"]`, `bundle.createUpdaterArtifacts = true` |
| Nom de l'installeur produit | `FlashGet_{version}_x64-setup.exe` (+ `.sig`) |
| Dernière version de yt-dlp | `GET https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest`, champ `tag_name`, en-tête `User-Agent: FlashGet` |
| Binaire et somme | `https://github.com/yt-dlp/yt-dlp/releases/download/{tag}/yt-dlp.exe` et `.../{tag}/SHA2-256SUMS` (ligne `<hex>  yt-dlp.exe`) |
| Emplacement du yt-dlp mis à jour | `{app_data}/bin/yt-dlp.exe` ; téléchargement dans `yt-dlp.download.exe`. Si la cible est verrouillée (yt-dlp en cours), le fichier reste en `yt-dlp.next.exe` et il est échangé au démarrage suivant |
| Comparaison de versions yt-dlp | composants numériques séparés par `.`, un composant absent vaut 0 (`2026.11.08` < `2026.11.08.1`) |
| Nouveaux réglages (`Settings`) | `ytdlp_auto_update: bool` (défaut `true`), `app_update_check: bool` (défaut `true`) |
| Clés internes (table `settings`, via `get_pref`/`set_pref`, hors liste blanche UI) | `ytdlp_update.last_check`, `app_update.last_check` (secondes Unix en texte), `app_update.resume_ids` (JSON `string[]`) |
| Fréquence yt-dlp | au démarrage, 10 s après l'ouverture, si `now - last_check >= 86 400 s` ; « Rechercher » force la vérification |
| Migration | si `{new}/downloads.sqlite` n'existe pas et que `{old}/downloads.sqlite` existe : la base est copiée par `VACUUM INTO` (copie cohérente même si l'ancien FlashGet tourne), puis les autres fichiers de `{old}` sont copiés (sans écraser). L'ancien dossier n'est jamais modifié. `{old}` = dossier parent de `{new}` + `com.steph.tauri-app` |
| Démarrage avec Windows | au démarrage, si l'autolancement est actif, on l'enregistre de nouveau, pour que le chemin pointe vers `FlashGet.exe` |
| Événements Rust → front | `ytdlp-update-available {current, latest}`, `ytdlp-updated {version, pending}`, `ytdlp-update-progress {downloaded, total}`, `app-update-available {version, date, notes}`, `app-update-progress {downloaded, total}` (`total: number \| null`) |

## Global Constraints

- Aucun code repris de NeoDLP, aucune mention d'un tiers dans l'interface (spec §2).
- Ne jamais remplacer un yt-dlp qui marche par un fichier non vérifié. Il faut que la somme SHA-256 corresponde **et** que `yt-dlp.download --version` réussisse avant tout remplacement.
- Rien ne s'installe sans un clic : une mise à jour de FlashGet est seulement **signalée** au démarrage. La mise à jour automatique de yt-dlp est la seule exception, et le réglage permet de la désactiver.
- Aucune vérification ne retarde l'ouverture de la fenêtre, et aucun échec réseau n'ouvre de message bloquant. L'échec s'affiche seulement dans la section concernée.
- La clé privée de signature et son mot de passe restent chez le propriétaire. Ils ne sont jamais écrits dans le dépôt, ni dans un fichier, ni dans un log. Seule la clé **publique** va dans `tauri.conf.json`.
- Tout texte visible en français via `t()` ; chiffres et versions en JetBrains Mono ; jetons Tailwind uniquement (bg, deep, panel, field, surface, border, line, ink, ink-soft, ink-mute, accent(+soft, bright, ink), accent-fg, cta, cta-ink, ok, okdim, err, queued).
- `cargo` depuis PowerShell, avec `-j 1` ; `npm test -- --maxWorkers=2`.

## Review Focus

1. **Téléchargement de yt-dlp corrompu ou tronqué**, ou somme qui ne correspond pas : le yt-dlp en place reste intact et utilisé, le fichier temporaire est supprimé, et la section affiche « fichier téléchargé invalide ». Test : Task 3 `install_rejects_bad_checksum`.
2. **yt-dlp en cours d'utilisation pendant la mise à jour** : le remplacement échoue sous Windows. La nouvelle version attend en `yt-dlp.next.exe` et prend la place au démarrage suivant, sans casser le téléchargement en cours. Tests : Task 3 `install_falls_back_to_next_when_locked`, Task 2 `apply_pending_swap`.
3. **Migration avec un nouveau dossier déjà rempli**, ou un ancien dossier absent : rien n'est écrasé, rien n'est supprimé, et FlashGet démarre normalement. Test : Task 1 `migration_never_overwrites`.
4. **FlashGet mis à jour avec un yt-dlp plus récent que la copie du dossier de données** : la copie, plus ancienne, ne doit pas masquer la version fournie. Si la version fournie est supérieure ou égale à la copie, la copie est supprimée lors de la vérification. Test : Task 3 `stale_data_copy_removed`.
5. **Téléchargements en cours au moment d'installer FlashGet** : ils sont mis en pause, puis reprennent après le redémarrage, même si la reprise automatique est désactivée. En revanche, ceux que l'utilisateur avait mis en pause restent en pause. Test : Task 4 `resume_ids_requeued_once`.

---

## Étape propriétaire P0 (avant la Task 4) : clé de signature

Le propriétaire lance lui-même la commande ci-dessous. Elle demande un mot de passe :

```
npx tauri signer generate -w %USERPROFILE%\.tauri\flashget.key
```

Il garde `flashget.key` et son mot de passe. Il transmet seulement le contenu de `flashget.key.pub`, la clé publique, qui va dans `tauri.conf.json` (Task 4).

---

### Task 1 : Identité, installeur et migration des données

**Files :**
- Modify : `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` (`version = "2.0.0"`, `description = "FlashGet — gestionnaire de téléchargements"`), `package.json` (`version`).
- Modify : `src-tauri/src/lib.rs` (setup).
- Create : `src-tauri/src/migrate.rs`.
- Modify : `src-tauri/crates/download-core/src/storage.rs`.

**Interfaces :**
- Produces : `Storage::snapshot_to(src: &Path, dst: &Path) -> Result<(), CoreError>`, une fonction associée qui exécute `VACUUM INTO` depuis une connexion ouverte sur `src`.
- Produces : `migrate::migrate_legacy_data(new_dir: &Path, old_dir: &Path) -> MigrationOutcome`, avec `enum MigrationOutcome { NotNeeded, Migrated { files: usize }, Failed(String) }`.
- Produces : `migrate::legacy_dir(new_dir: &Path) -> PathBuf`, qui renvoie le dossier parent de `new_dir` joint à `com.steph.tauri-app`.

- [ ] **Step 1 : Tests qui échouent** (dans `migrate.rs`, avec `tempfile`) :
  - `migrates_database_and_files` : l'ancien dossier contient une base avec 2 téléchargements et `youtube-cookies.txt`. Après la migration, `Storage::open(new/downloads.sqlite).list()` renvoie 2 lignes, le fichier de cookies est copié, et l'ancien dossier n'a pas changé.
  - `migration_never_overwrites` : le nouveau dossier contient déjà `downloads.sqlite`, et le résultat est `NotNeeded`, avec des octets identiques. Si l'ancien dossier est absent, le résultat est aussi `NotNeeded`.
  - `legacy_dir_is_sibling` : `legacy_dir("C:/x/Roaming/com.jstephane98.flashget")` renvoie `C:/x/Roaming/com.steph.tauri-app`.
  - Test `storage.rs` `snapshot_to_copies_rows`.
- [ ] **Step 2 :** lancer `cargo test -j 1 -p tauri-app migrate` et `-p download-core snapshot`, et constater l'échec.
- [ ] **Step 3 : Implémenter.** Dans `lib.rs`, appeler `migrate_legacy_data` avant `Storage::open`. Un `Failed` est seulement journalisé (`eprintln!`), puis FlashGet démarre sur un dossier neuf.
  - Après `app.manage`, rafraîchir l'autolancement : `if app.autolaunch().is_enabled().unwrap_or(false) { let _ = app.autolaunch().enable(); }`.
- [ ] **Step 4 : Configuration** dans `tauri.conf.json` :
  - `"identifier": "com.jstephane98.flashget"`, `"version": "2.0.0"`, `"mainBinaryName": "FlashGet"` ;
  - `bundle.targets: ["nsis"]` ;
  - `bundle.windows.nsis: { "installMode": "currentUser", "languages": ["French"], "displayLanguageSelector": false }`.
  - Vérifier que `npx tauri info` et `cargo build -j 1` acceptent la configuration.
- [ ] **Step 5 :** lancer `cargo test --workspace -j 1` (tout doit être vert), puis commit `feat: identifiant FlashGet, installeur par utilisateur, migration des données`.

### Task 2 : Réglages de mise à jour et choix du yt-dlp à lancer

**Files :**
- Modify : `crates/download-core/src/model.rs`, `storage.rs` (2 champs `Settings`).
- Modify : `src/types.ts`.
- Create : `src-tauri/src/ytdlp_bin.rs`.
- Modify : `src-tauri/src/ytdlp.rs` (4 appels `sidecar("yt-dlp")`), `src-tauri/src/lib.rs`.

**Interfaces :**
- Produces (champs `Settings`) : `ytdlp_auto_update: bool`, `app_update_check: bool`. Ils ont les mêmes noms dans `src/types.ts` et sont enregistrés sous les clés du même nom.
- Produces : `ytdlp_bin::data_bin_dir(app: &AppHandle) -> Option<PathBuf>` (`{app_data}/bin`).
- Produces : `ytdlp_bin::resolve_override(dir: &Path) -> Option<PathBuf>`, qui renvoie `dir/yt-dlp.exe` s'il existe.
- Produces : `ytdlp_bin::apply_pending_swap(dir: &Path) -> std::io::Result<bool>`. Si `yt-dlp.next.exe` existe, il remplace `yt-dlp.exe`, et la fonction renvoie `true`.
- Produces : `ytdlp_bin::ytdlp_command(app: &AppHandle) -> Result<tauri_plugin_shell::process::Command, String>`. C'est `app.shell().command(override)` si une copie existe, sinon `app.shell().sidecar("yt-dlp")`, avec `PYTHONUTF8=1` et `PYTHONIOENCODING=utf-8` dans les deux cas.

- [ ] **Step 1 : Tests qui échouent :**
  - `settings_defaults_for_missing_keys` est étendu : les deux champs valent `true`.
  - L'aller-retour `set_settings` / `get_settings` avec `false`.
  - `resolve_override` : un dossier vide renvoie `None`, un dossier avec un fichier renvoie `Some`.
  - `apply_pending_swap` : avec `next` et l'ancien fichier, le contenu de `next` prend la place, `next` disparaît, et la fonction renvoie `true`. Sans `next`, elle renvoie `false` et ne change rien.
- [ ] **Step 2 :** constater l'échec.
- [ ] **Step 3 : Implémenter.**
  - Les 4 appels de `ytdlp.rs` passent par `ytdlp_command`. Le cache `AppState.ytdlp_version` reste inchangé.
  - Dans `lib.rs`, au setup, appeler `apply_pending_swap(data_bin_dir)` avant toute utilisation de yt-dlp. Une erreur est seulement journalisée.
- [ ] **Step 4 :** lancer `cargo test --workspace -j 1`, `npm test -- --maxWorkers=2` et `npx tsc --noEmit`, puis commit `feat: réglages de mise à jour, yt-dlp du dossier de données en priorité`.

### Task 3 : Mise à jour de yt-dlp (Rust)

**Files :**
- Create : `src-tauri/src/ytdlp_update.rs`.
- Modify : `src-tauri/Cargo.toml` (ajouter `sha2 = "0.10"` et `reqwest`, en reprenant la version et les features de `download-core`), `src-tauri/src/lib.rs` (commandes et tâche de démarrage), `src-tauri/src/state.rs` (`ytdlp_updating: AtomicBool`).

**Interfaces :**
- Consumes : `ytdlp_bin::*` (Task 2), `parse_version` et `AppState.ytdlp_version` (phase 5).
- Produces, en Rust pur :
  - `compare_versions(a: &str, b: &str) -> std::cmp::Ordering` ;
  - `parse_sha256sums(text: &str, file: &str) -> Option<String>` (hex en minuscules) ;
  - `latest_tag(json: &str) -> Option<String>`.
- Produces, côté fichiers (testable sans réseau) : `install_from_bytes(dir: &Path, bytes: &[u8], expected_sha: &str, run_version: impl Fn(&Path) -> Option<String>) -> Result<InstallOutcome, String>`, avec `enum InstallOutcome { Replaced(String), Pending(String) }` (la chaîne est la version).
- Produces, les commandes :
  - `check_ytdlp_update(app) -> Result<YtdlpUpdateInfo, String>`, avec `struct YtdlpUpdateInfo { current: Option<String>, latest: String, available: bool, last_check: Option<i64> }` ;
  - `update_ytdlp(app) -> Result<YtdlpUpdateResult, String>`, avec `struct YtdlpUpdateResult { version: String, pending: bool }` (`pending` : la nouvelle version attend le prochain démarrage, cible verrouillée) ;
  - `get_ytdlp_last_check(app) -> Option<i64>`.
- Produces : `start_ytdlp_auto_check(app: AppHandle)`, lancée par le setup.

- [ ] **Step 1 : Tests qui échouent :**
  - `compare_versions` : `2026.10.30 < 2026.11.08`, `2026.11.08 < 2026.11.08.1`, et `2026.11.08 == 2026.11.08`.
  - `parse_sha256sums` : un texte de 3 lignes avec `yt-dlp.exe`, `yt-dlp` et `yt-dlp_x86.exe` renvoie le hachage de `yt-dlp.exe` seulement, et un fichier absent renvoie `None`.
  - `latest_tag` sur un JSON réduit (`{"tag_name":"2026.11.08"}`).
  - `install_rejects_bad_checksum` : avec une mauvaise somme, la fonction renvoie `Err("fichier téléchargé invalide")`. L'ancien `yt-dlp.exe` est intact et `yt-dlp.download` est absent.
  - `install_rejects_unrunnable` : la somme est bonne mais `run_version` renvoie `None`, donc `Err`, et l'ancien fichier est intact.
  - `install_replaces` : le contenu est remplacé et la fonction renvoie `Replaced(version)`.
  - `install_falls_back_to_next_when_locked` : la cible est rendue non remplaçable (en lecture seule, ou ouverte sans partage sous Windows). La fonction renvoie `Pending(version)` et `yt-dlp.next.exe` contient les nouveaux octets.
  - `stale_data_copy_removed` : on teste `should_drop_data_copy(bundled: Option<&str>, data: Option<&str>) -> bool`. Le résultat est `true` si `bundled >= data`, et `false` si l'une des deux est inconnue.
- [ ] **Step 2 :** constater l'échec.
- [ ] **Step 3 : Implémenter.**
  - `update_ytdlp` refuse un second appel concurrent avec « Mise à jour déjà en cours ».
  - Elle émet `ytdlp-update-progress` pendant le téléchargement. Après un succès, elle vide `AppState.ytdlp_version` et enregistre `ytdlp_update.last_check`.
  - `check_ytdlp_update` compare la version en service à la dernière publiée. Si une copie existe dans le dossier de données, elle lance aussi le sidecar `--version`, et supprime la copie quand `should_drop_data_copy` le demande.
  - Tâche de démarrage : attendre 10 s, puis, si `now - last_check >= 86400` :
    - vérifier ;
    - si une version est disponible et `ytdlp_auto_update` est actif, lancer `update_ytdlp`, puis émettre `ytdlp-updated` ;
    - si une version est disponible et le réglage est inactif, émettre `ytdlp-update-available`.

    Toute erreur est journalisée, sans événement d'erreur.
  - Messages d'erreur en français :
    - « Impossible de joindre GitHub » ;
    - « fichier téléchargé invalide » ;
    - « Mise à jour déjà en cours ».
- [ ] **Step 4 :** lancer `cargo test --workspace -j 1`, puis commit `feat: mise à jour de yt-dlp vérifiée par somme SHA-256`.

### Task 4 : Mise à jour de FlashGet (Rust)

Prérequis : la clé publique de l'étape P0.

**Files :**
- Create : `src-tauri/src/app_update.rs`.
- Modify : `src-tauri/Cargo.toml` (`tauri-plugin-updater = "2"`), `src-tauri/tauri.conf.json` (`plugins.updater`, `bundle.createUpdaterArtifacts: true`), `src-tauri/src/lib.rs`, `src-tauri/src/state.rs`.

**Interfaces :**
- Consumes : `queue::pause_active`, `reevaluate`, `Storage::get_pref`/`set_pref`.
- Produces :
  - `struct AppUpdateInfo { version: String, date: Option<String>, notes: Option<String> }` ;
  - les commandes `check_app_update(app) -> Result<Option<AppUpdateInfo>, String>`, `download_app_update(app) -> Result<(), String>` (qui émet `app-update-progress`, puis garde les octets et l'`Update` dans `AppState.pending_update: Mutex<Option<(Update, Vec<u8>)>>`), `install_app_update(app) -> Result<(), String>` et `get_app_last_check(app) -> Option<i64>`.
- Produces, en Rust pur et testable : `remember_resume_ids(storage: &Storage, ids: &[String])`, puis `take_resume_ids(storage: &Storage) -> Vec<String>`, qui lit la clé puis l'efface.
- Produces : `requeue_after_update(app: &AppHandle) -> usize`. Elle remet en `queued` les lignes encore `paused` parmi les identifiants enregistrés. Elle est appelée dans le setup **avant** `start_planner_loop`, et son résultat s'ajoute à `requeued`.

- [ ] **Step 1 : Tests qui échouent :**
  - `resume_ids_requeued_once` : on enregistre `["a", "b"]`. La ligne `a` est en pause, `b` est terminée, et une ligne `c` est en pause sans être dans la liste. Après la remise en file, seule `a` passe en `queued`, `c` reste en pause, la clé est effacée, et un second appel ne fait rien.
  - `take_resume_ids` sur une clé absente ou du JSON invalide renvoie `[]`.
- [ ] **Step 2 :** constater l'échec.
- [ ] **Step 3 : Implémenter** `install_app_update` :
  1. prendre `pending_update` ; s'il est vide, renvoyer `Err("Aucune mise à jour téléchargée")` ;
  2. relever les identifiants des lignes `downloading` ou `recording` qui ont un handle ;
  3. appeler `pause_active`, puis attendre au plus 5 s que leurs handles disparaissent de `handles` ;
  4. appeler `remember_resume_ids` ;
  5. appeler `update.install(bytes)`. Sous Windows, cette fonction quitte le processus.

  Au démarrage, si `app_update_check` est actif : attendre 10 s, appeler `check_app_update`, enregistrer `app_update.last_check`, puis émettre `app-update-available` si une version est disponible. Une erreur est seulement journalisée.
- [ ] **Step 4 : Configuration.**
  - `plugins.updater` : `{ "pubkey": "<clé publique P0>", "endpoints": ["https://github.com/jstephane98/FlashGet/releases/latest/download/latest.json"], "windows": { "installMode": "passive" } }`.
  - Enregistrer le plugin dans `lib.rs` avec `.plugin(tauri_plugin_updater::Builder::new().build())`.
- [ ] **Step 5 :** lancer `cargo test --workspace -j 1` et `cargo build -j 1`, puis commit `feat: mise à jour de FlashGet via GitHub Releases`.

### Task 5 : Interface — section Mises à jour et points d'attention

**Files :**
- Modify : `src/api.ts`, `src/App.tsx`, `src/components/shell/Sidebar.tsx`, `src/components/settings/SettingsNav.tsx`, `src/pages/SettingsPage.tsx`, `src/components/settings/UpdatesSection.tsx` (réécrite), `src/i18n/fr.ts`.
- Test : `src/components/settings/updates.test.tsx`.

**Interfaces :**
- Consumes : les commandes et événements des Tasks 3 et 4.
- Produces (`api.ts`) :
  - `checkAppUpdate(): Promise<AppUpdateInfo | null>`, `downloadAppUpdate(): Promise<void>`, `installAppUpdate(): Promise<void>`, `getAppLastCheck(): Promise<number | null>` ;
  - `onAppUpdateAvailable(cb)` et `onAppUpdateProgress(cb: (p: { downloaded: number; total: number | null }) => void)` ;
  - les fonctions yt-dlp équivalentes pour la Task 6 : `checkYtdlpUpdate()`, `updateYtdlp()`, `getYtdlpLastCheck()`, `onYtdlpUpdateAvailable`, `onYtdlpUpdated`, `onYtdlpUpdateProgress`.
- Produces (App → pages) : `type UpdateFlags = { app: AppUpdateInfo | null; ytdlp: string | null }`.
  - `ytdlp` vaut la dernière version disponible quand la mise à jour automatique est désactivée.
  - Le `Sidebar` reçoit `settingsDot: boolean`, et `SettingsPage` reçoit `updates: UpdateFlags` et `onUpdatesChanged(f: UpdateFlags)`.
  - `SettingsNav` reçoit `attention: Partial<Record<SectionId, boolean>>`, avec `updates` pour l'application et `video` pour yt-dlp.

- [ ] **Step 1 : Tests qui échouent :**
  - **États de la carte FlashGet**, selon la maquette 05 :
    - « FlashGet est à jour » ;
    - « Recherche d'une nouvelle version… », avec le bouton désactivé ;
    - disponible : la version en mono, les nouveautés et « Télécharger et installer » ;
    - téléchargement : la barre et `18,4 / 29,6 Mo · 62 %` ;
    - prête : « Installer et redémarrer » appelle `installAppUpdate` ;
    - échec : « Réessayer » relance `checkAppUpdate`.
  - Quand `total` est `null`, on affiche les Mo téléchargés sans pourcentage.
  - L'interrupteur « Rechercher les mises à jour au démarrage » enregistre `app_update_check: false`.
  - Le point d'attention apparaît sur « Mises à jour » quand `updates.app` est renseigné.
  - `Sidebar` montre un point sur « Paramètres » quand `settingsDot` est vrai.
- [ ] **Step 2 :** constater l'échec.
- [ ] **Step 3 : Implémenter.**
  - `App` écoute `app-update-available`, `ytdlp-update-available` et `ytdlp-updated` (ce dernier remet `ytdlp` à `null`).
  - On pose `settingsDot = !!updates.app || !!updates.ytdlp`.
  - La section n'appelle pas `checkAppUpdate` à l'ouverture si `updates.app` est déjà connu ; elle affiche cette information.
  - Dates : « aujourd'hui à HH:MM », « hier à HH:MM », sinon `JJ/MM/AAAA` (fonction pure `formatLastCheck(sec, now)`, testée).
- [ ] **Step 4 :** lancer `npm test -- --maxWorkers=2` et `npx tsc --noEmit`, puis commit `feat(ui): section Mises à jour, points d'attention`.

### Task 6 : Interface — carte Moteur yt-dlp

**Files :**
- Create : `src/components/settings/YtdlpEngineCard.tsx`.
- Modify : `src/components/settings/VideoSection.tsx`, `src/i18n/fr.ts`.
- Test : `src/components/settings/ytdlp-engine.test.tsx`.

**Interfaces :**
- Consumes : les fonctions yt-dlp de `api.ts` (Task 5) ; `updates.ytdlp` et `onUpdatesChanged` via `SettingsPage`.
- Produces : `<YtdlpEngineCard settings save latest={updates.ytdlp} onUpdated={(v) => …} />`.

- [ ] **Step 1 : Tests qui échouent :**
  - **États**, selon la maquette 05 :
    - à jour : la version en mono, « À jour » et « Rechercher » ;
    - disponible : « Nouvelle version disponible : X » et « Mettre à jour » ;
    - en cours : la barre de progression et le bouton désactivé ;
    - échec : le message renvoyé par Rust, « La version actuelle reste utilisée » et « Réessayer » ;
    - en attente (`pending`) : « La version X sera utilisée au prochain démarrage de FlashGet. »
  - Après une mise à jour réussie, la version affichée change et `onUpdated` est appelé.
  - L'interrupteur automatique enregistre `ytdlp_auto_update: false`.
- [ ] **Step 2 :** constater l'échec.
- [ ] **Step 3 : Implémenter.** La carte remplace la carte Moteur actuelle de `VideoSection`.
- [ ] **Step 4 :** lancer `npm test -- --maxWorkers=2` et `npx tsc --noEmit`, puis commit `feat(ui): mise à jour de yt-dlp dans Vidéo et yt-dlp`.

### Task 7 : Script de publication

**Files :**
- Create : `scripts/release.mjs`, `scripts/manifest.mjs`, `scripts/manifest.test.ts`, `docs/RELEASE.md`.
- Modify : `package.json` (`"release": "node scripts/release.mjs"`), et la config Vitest si elle n'inclut pas `scripts/`.

**Interfaces :**
- Produces : `buildManifest({ version, notes, pubDate, signature, fileName, repo }): object`. Le résultat a la forme `{ version, notes, pub_date, platforms: { "windows-x86_64": { signature, url } } }`, avec `url = https://github.com/{repo}/releases/download/v{version}/{fileName}`.
- Produces : `release.mjs [--notes fichier.md]`.
  - Il s'arrête avec un message clair si `TAURI_SIGNING_PRIVATE_KEY` (ou `TAURI_SIGNING_PRIVATE_KEY_PATH`) est absent. Il ne lit jamais le mot de passe et ne l'affiche jamais.
  - Il lance `npx tauri build`, puis copie dans `release/v{version}/` l'installeur, son `.sig` et `latest.json`.

- [ ] **Step 1 : Tests qui échouent :**
  - `buildManifest` produit l'URL exacte pour `2.0.0`, avec `fileName = FlashGet_2.0.0_x64-setup.exe` et `repo = jstephane98/FlashGet`.
  - La signature reprend le contenu du `.sig` sans saut de ligne final.
  - `pub_date` est au format RFC 3339.
- [ ] **Step 2 :** constater l'échec, implémenter, puis rendre les tests verts.
- [ ] **Step 3 :** `docs/RELEASE.md` décrit en français :
  - la clé (P0) ;
  - les variables d'environnement à définir dans PowerShell (`$env:TAURI_SIGNING_PRIVATE_KEY_PATH`, `$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) ;
  - `npm run release` ;
  - la création de la Release `v{version}` sur GitHub, avec les 3 fichiers à déposer ;
  - le fait que le dépôt (ou au moins ses Releases) doit être public ;
  - le premier passage depuis l'ancien FlashGet 0.1.0, qui n'a pas d'updater, et qu'il faut donc installer à la main.
- [ ] **Step 4 :** `.gitignore` reçoit `release/`. Lancer `npm test -- --maxWorkers=2`, puis commit `chore: script de publication et guide`.

### Task 8 : Vérification d'ensemble (contrôleur et propriétaire)

- [ ] **Automatique :**
  - `npm test -- --maxWorkers=2`, `npx tsc --noEmit` et `npm run build` ;
  - `cargo test --workspace -j 1` et `cargo build -j 1` ;
  - `grep` : plus aucun `sidecar("yt-dlp")` hors de `ytdlp_bin.rs`.
- [ ] **Manuelle, par le propriétaire :**
  1. Lancer `npm run tauri dev` en ayant fermé tout FlashGet. Les données de l'ancien dossier sont là : téléchargements, dossiers, jeton de l'extension, thème.
  2. Paramètres › Vidéo et yt-dlp › « Rechercher » : la version la plus récente est trouvée. « Mettre à jour » remplace yt-dlp, la version affichée change, et une analyse de lien fonctionne ensuite.
  3. Paramètres › Mises à jour › « Rechercher » : tant qu'aucune Release 2.0.0 n'existe, l'état « Échec » s'affiche proprement.
  4. `npm run release`, avec la clé, produit `release/v2.0.0/`. L'installeur s'installe sans droits administrateur. Le programme s'appelle `FlashGet.exe`, et les Applications installées ne montrent qu'un seul FlashGet (sinon, désinstaller l'ancien 0.1.0).
  5. Publier une 2.0.0, puis une 2.0.1 de test. Depuis la 2.0.0 : point ambre, « Télécharger et installer », puis « Installer et redémarrer ». Un téléchargement en cours reprend après le redémarrage.
- [ ] **Fin de phase** — `superpowers:finishing-a-development-branch`.
