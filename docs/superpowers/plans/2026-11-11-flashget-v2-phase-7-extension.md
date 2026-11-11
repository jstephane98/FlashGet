# FlashGet v2 — Phase 7 : extension navigateur — Plan d'implémentation

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal :** la liaison entre FlashGet et l'extension devient fiable et visible. Quatre ajouts :
- l'**état réel** de la connexion, dans l'application comme dans l'extension ;
- une **aide à l'installation** qui fonctionne, avec le dossier de l'extension fourni par l'installeur ;
- des **sites ignorés**, avec une liste unique partagée entre l'application et l'extension ;
- un **« Réessayer »** qui renvoie le contexte de la capture.

**Architecture :**
- **Rust est la source de vérité :**
  - qui est connecté, avec le navigateur et la version de l'extension ;
  - le dernier refus de jeton ;
  - la liste des sites ignorés ;
  - le contexte des captures échouées.
- **Le protocole WebSocket existant s'étend.** Chaque ajout est compatible avec une ancienne extension, qui ignore simplement ce qu'elle ne connaît pas :
  - le message `auth` accepte `browser` et `version` en plus ;
  - le serveur répond `config` après `authed`, puis à chaque changement ;
  - l'extension peut envoyer `set_site_ignored`.
- **Le serveur garde un canal `tokio::sync::broadcast` dans l'état de l'application.** Il pousse la configuration à toutes les extensions connectées.
- **Un événement `bridge-status` tient l'interface à jour.** Il est émis à chaque connexion, déconnexion ou refus de jeton. La barre d'état n'interroge plus le serveur.
- **L'extension garde sa logique pure dans des modules testés.** Ce sont `capture-rules.js` et un nouveau `site-rules.js`. `background.js`, `popup.js` et `options.js` restent de fines couches.

**Tech Stack :** Tauri 2.11 (`bundle.resources`, opener), Rust (`tokio-tungstenite`, `tokio::sync::broadcast`), React 19, TypeScript, Tailwind, Vitest. L'extension est en Manifest V3, en JavaScript avec des modules ES.

**Spec :** [`../specs/2026-11-10-flashget-v2-refonte-design.md`](../specs/2026-11-10-flashget-v2-refonte-design.md), §4.8 (section 5), §7.4 (déjà fait en phase 2) et §8.9.

**Maquette (validée le 2026-11-11) :** [`../specs/2026-11-10-flashget-v2-maquettes/06-extension.html`](../specs/2026-11-10-flashget-v2-maquettes/06-extension.html).

**Branche :** `flashget-v2-phase-7`, créée depuis `main` (phases 1 à 6 fusionnées, merge 661383a).

## Décisions techniques

| Sujet | Valeur |
|---|---|
| Port | Toujours `15151` (constante `PORT` de `bridge/server.rs`). Le champ « Port » disparaît des options de l'extension ; un port enregistré par une ancienne version est ignoré. |
| `auth` étendu | `{type:"auth", token, browser?: string, version?: string}`. `browser` vaut `"Chrome"`, `"Edge"`, `"Brave"` ou `"Chromium"` (détection côté extension, voir la Task 4). `version` = `chrome.runtime.getManifest().version`. |
| Messages serveur → extension | Ceux d'aujourd'hui, plus `{type:"config", ignored_sites: string[]}`. Il est envoyé juste après `authed`, puis à chaque changement de la liste. |
| Message extension → serveur | `{type:"set_site_ignored", host: string, ignored: bool}`. Il n'est accepté qu'après authentification, sinon le serveur répond `need_auth`. |
| `last_seen` | Il est mis à jour à chaque message reçu d'une connexion authentifiée. L'alarme de 30 s de l'extension envoie déjà un message ; à défaut, on ajoute `{type:"ping"}`, qui reçoit `{type:"pong"}`. |
| États de la connexion | `connected` : au moins une connexion authentifiée. `rejected` : aucune connexion authentifiée, et un refus de jeton dans les 120 dernières secondes. `waiting` : le serveur écoute, sans connexion authentifiée ni refus récent. `inactive` : le serveur n'écoute pas (port occupé). |
| Plusieurs extensions | Si plusieurs navigateurs sont connectés, on affiche le plus récemment actif. Le nombre total va dans `clients`. |
| Sites ignorés | Clé de préférence `ext.ignored_sites`, un tableau JSON d'hôtes normalisés, sans doublons, triés, au plus 200 entrées. Normalisation, dans cet ordre : espaces retirés et minuscules ; schéma, puis tout ce qui suit le premier `/`, `?` ou `#`, retirés ; port final `:chiffres` retiré ; point final retiré ; `*.` initial retiré ; `www.` initial retiré une fois ; nom international converti en punycode ASCII. Valides : `localhost`, une IPv4, ou un nom avec au moins un point dont chaque partie est `[a-z0-9-]`, non vide, sans `-` en début ni en fin. Refusés : IPv6, un nom sans point autre que `localhost` (ex. `www.com` → `com`), tout le reste. Un site nom `h` correspond à l'hôte `h` ou à `*.h` ; une IPv4 seulement à elle-même. |
| Portée d'un site ignoré | Il bloque seulement la **capture automatique** (`downloads.onDeterminingFilename`), quand l'URL du téléchargement (avant ou après redirection) **ou** son référent est sur un site ignoré. Le clic droit et les boutons de la fenêtre de l'extension continuent de marcher. |
| Contexte d'une capture échouée | `AppState.failed_captures: Mutex<HashMap<String, FailedCapture>>`, avec la clé `capture_id` (uuid v4), une durée de vie de 30 min et au plus 20 entrées (la plus ancienne est retirée). `FailedCapture { url, headers: RequestHeaders, hint: ProbeHint, filename: Option<String>, origin: Option<String>, created: Instant }`. `origin` est l'hôte du référent. |
| Événement d'échec | `link://error {url, message, capture_id?: string, origin?: string}`. `capture_id` est présent seulement pour une capture de l'extension. |
| Dossier de l'extension dans l'installeur | Il passe par `bundle.resources`, qui liste les fichiers utiles un par un : `manifest.json`, `*.js` **sauf** `*.test.js`, `*.html`, `ui.css`, `icons/*` et `fonts/*`. Ils sont copiés dans `extension/`. À l'exécution, le dossier est `app.path().resource_dir()/extension`. |
| Version de l'extension | `2.0.0` dans `extension/manifest.json`, la même version que FlashGet. |

## Global Constraints

- Aucun code repris de NeoDLP, et aucune mention d'un tiers (spec §2).
- **Compatibilité :** une extension 0.1.0 déjà installée doit continuer de capturer avec FlashGet 2.0.0. Seuls lui manquent le navigateur et la version affichés, et les sites ignorés.
- **Sécurité :** aucune capture ni aucun changement de la liste avant un `auth` valide. Le jeton n'est jamais journalisé.
- **Aucune régression des captures :** interception des téléchargements, clic droit, flux HLS/DASH repérés, bouton « Télécharger la vidéo de cette page », file d'attente des captures hors connexion.
- Les textes de l'interface passent par `t()`, en français. Les chiffres et les noms de domaine sont en JetBrains Mono. On utilise uniquement les jetons Tailwind (bg, deep, panel, field, surface, border, line, ink, ink-soft, ink-mute, accent (+ soft, bright, ink), accent-fg, cta, cta-ink, ok, okdim, err, queued). Les textes de l'extension restent en français.
- `cargo` se lance depuis PowerShell, avec `-j 1`. Les tests se lancent avec `npm test -- --maxWorkers=2`.

## Review Focus

1. **Saisie d'un site ignoré** (`https://www.Exemple.com/page`, `exemple.com:8080`, ` `, `localhost`, `*.exemple.com`, une IP) : la saisie est normalisée (`exemple.com`, `localhost`, l'IP) ou refusée avec un message en français, et la liste n'est jamais corrompue. Test : Task 2 `normalize_site`.
2. **Sous-domaines** : `drive.google.com` ignoré n'ignore pas `google.com`, mais ignore `docs.drive.google.com`. `ample.com` n'ignore pas `exemple.com`. Tests : Task 2 `host_is_ignored`, Task 4 `isIgnored`.
3. **Extension 0.1.0** connectée à FlashGet 2.0.0 : le message `auth` sans `browser` ni `version` fonctionne, et le message `config`, qu'elle ne connaît pas, ne la casse pas. Test : Task 1 `auth_without_browser_fields`.
4. **Déconnexion brutale** (navigateur fermé, veille) : l'état repasse à « en attente » et la barre d'état suit sans redémarrage. Test : Task 1 `status_after_disconnect`.
5. **« Réessayer » après l'expiration ou le nettoyage du contexte** (plus de 30 min) : l'analyse se relance sans contexte, comme aujourd'hui, sans erreur incompréhensible. Test : Task 3 `retry_unknown_capture_id`.

---

### Task 1 : État réel de la connexion (Rust)

**Files :**
- Modify : `src-tauri/src/bridge/protocol.rs`, `src-tauri/src/bridge/server.rs`, `src-tauri/src/state.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`.
- Create : `src-tauri/src/bridge/status.rs`.

**Interfaces :**
- Produces : `ClientMessage::Auth { token, browser: Option<String>, version: Option<String> }`, et `ClientMessage::Ping`, qui reçoit `ServerReply::Pong`.
- Produces : dans `status.rs`, en Rust pur :
  - `struct ClientInfo { browser: Option<String>, version: Option<String>, last_seen: i64 }` ;
  - `#[derive(Serialize)] struct BridgeStatus { state: &'static str, browser: Option<String>, version: Option<String>, last_seen: Option<i64>, clients: usize, port: u16 }` ;
  - `fn summarize(listening: bool, clients: &HashMap<u64, ClientInfo>, last_rejected: Option<i64>, now: i64) -> BridgeStatus`.
- Produces : `AppState.bridge_clients: Mutex<HashMap<u64, ClientInfo>>` (la clé est un id de connexion, tiré d'un `AtomicU64`) et `AppState.bridge_rejected_at: Mutex<Option<i64>>`.
- Produces : la commande `get_bridge_status(app) -> BridgeStatus`, et l'événement `bridge-status` (payload `BridgeStatus`), émis à chaque authentification, déconnexion d'un client authentifié ou refus de jeton. Il est aussi émis une fois que l'écoute commence ou échoue.

- [ ] **Step 1 : Tests qui échouent.**
  - `auth_without_browser_fields` : l'ancien `auth` se parse avec `browser: None`.
  - `auth_with_browser_fields`.
  - `ping_pong`.
  - `summarize` : `inactive`, `waiting`, `connected` (le client le plus récent est retenu, `clients = 2`), puis `rejected` dans les 120 s.
  - `status_after_disconnect` : après le retrait du client, l'état revient à `waiting`.
- [ ] **Step 2 :** constater l'échec.
- [ ] **Step 3 : Implémenter.**
  - `handle_conn` enregistre le client après `Authed`, met à jour `last_seen` à chaque message reçu, et le retire à la fermeture, quelle qu'en soit la cause.
  - `AuthFailed` enregistre `bridge_rejected_at`.
  - Chaque changement émet `bridge-status`.
  - `get_bridge_info` reste inchangé (jeton et port).
- [ ] **Step 4 :** `cargo test --workspace -j 1`, puis commit `feat: état réel de la connexion avec l'extension`.

### Task 2 : Sites ignorés (Rust)

**Files :**
- Create : `src-tauri/src/bridge/sites.rs`.
- Modify : `bridge/protocol.rs`, `bridge/server.rs`, `state.rs`, `commands.rs`, `lib.rs`.

**Interfaces :**
- Produces, en Rust pur dans `sites.rs` :
  - `fn normalize_site(input: &str) -> Result<String, String>` (erreur : « Adresse de site invalide ») ;
  - `fn host_is_ignored(host: &str, sites: &[String]) -> bool` ;
  - `fn set_ignored(sites: &[String], host: &str, ignored: bool) -> Vec<String>` (normalisé, sans doublon, trié, au plus 200 entrées).
- Produces : les commandes `get_ignored_sites(app) -> Vec<String>` et `set_ignored_sites(app, sites: Vec<String>) -> Result<Vec<String>, String>`. La seconde normalise chaque entrée, refuse toute la liste si une entrée est invalide (le message cite l'entrée), enregistre, diffuse `config` et émet `ignored-sites-changed` (`string[]`).
- Produces : `ClientMessage::SetSiteIgnored { host, ignored }`. Une fois authentifié, il met la liste à jour de la même façon : diffusion à toutes les extensions et événement pour l'interface.
- Produces : `AppState.bridge_config: tokio::sync::broadcast::Sender<String>`, qui porte le JSON du message `config`. Chaque `handle_conn` authentifiée s'y abonne et relaie le message.

- [ ] **Step 1 : Tests qui échouent :**
  - `normalize_site` : les cas de la Review Focus #1, plus `*.exemple.com` → `exemple.com` ;
  - `host_is_ignored` : la Review Focus #2 ;
  - `set_ignored` : ajout, retrait, doublon, tri, plafond de 200 ;
  - `set_site_ignored_requires_auth`.
- [ ] **Step 2 :** constater l'échec.
- [ ] **Step 3 : Implémenter.** Le serveur envoie `config` juste après `authed`. Une erreur de diffusion (aucun abonné) est ignorée.
- [ ] **Step 4 :** `cargo test --workspace -j 1`, puis commit `feat: sites ignorés partagés avec l'extension`.

### Task 3 : Contexte de capture, dossier de l'extension (Rust)

**Files :**
- Modify : `src-tauri/src/link.rs`, `state.rs`, `commands.rs`, `lib.rs`, `src-tauri/tauri.conf.json`.

**Interfaces :**
- Produces : `struct FailedCapture` et `AppState.failed_captures` (voir les Décisions techniques). Les fonctions pures `remember_failed(map, id, capture, now)`, qui retire les entrées expirées et applique le plafond, et `take_failed(map, id, now) -> Option<FailedCapture>`.
- Produces : `link::capture` émet `link://error {url, message, capture_id, origin}` et garde le contexte.
- Produces : la commande `retry_capture(app, capture_id: String, url: String)`.
  - Si le contexte est trouvé, elle relance `link::capture` avec les mêmes `url`, `headers`, `hint` et `filename`.
  - Sinon (contexte expiré ou inconnu), elle relance `link::capture` avec `url` seule et les valeurs par défaut.
- Toutes les nouvelles commandes des Tasks 1 à 3 sont enregistrées dans `generate_handler!`.
- Produces : la commande `open_extension_folder(app) -> Result<(), String>`. Elle ouvre `resource_dir()/extension` avec l'opener ; si le dossier est absent : « Dossier de l'extension introuvable ».
- Produces : `bundle.resources` dans `tauri.conf.json`, avec la liste de fichiers des Décisions techniques.

- [ ] **Step 1 : Tests qui échouent :**
  - `remember_and_take_failed` : la clé est trouvée une fois, puis absente ;
  - `failed_capture_expires` ;
  - `failed_capture_cap` : la 21e entrée retire la plus ancienne ;
  - `retry_unknown_capture_id` : `take_failed` renvoie `None` sans paniquer ;
  - `origin_from_referrer` : renvoie l'hôte, ou `None`.
- [ ] **Step 2 :** constater l'échec.
- [ ] **Step 3 : Implémenter.**
- [ ] **Step 4 :** `cargo test --workspace -j 1` et `cargo build -j 1`. Vérifier que `target/debug/extension/manifest.json` existe après la compilation (copie des ressources en développement) et qu'aucun `*.test.js` n'y est copié. Puis commit `feat: Réessayer garde le contexte de la capture, dossier de l'extension fourni`.

### Task 4 : Extension

**Files :**
- Create : `extension/site-rules.js`, `extension/site-rules.test.js`, `extension/browser-info.js`, `extension/browser-info.test.js`.
- Modify : `extension/background.js`, `capture-rules.js`, `capture-rules.test.js`, `popup.html`, `popup.js`, `options.html`, `options.js`, `manifest.json` (`"version": "2.0.0"`), `README.md`.

**Interfaces :**
- Consumes : le protocole des Tasks 1 et 2.
- Produces, en JS pur :
  - `site-rules.js` : `hostOf(url): string | null`, `isIgnored(url, sites): boolean` et `normalizeHost(host): string`, avec les mêmes règles que le Rust ;
  - `browser-info.js` : `browserName(uaData, ua): "Edge" | "Brave" | "Chrome" | "Chromium"`.
  - `shouldCapture(item, settings, ignoredSites)` dans `capture-rules.js` renvoie `false` si l'URL du téléchargement, ou à défaut son référent, est sur un site ignoré.
- Produces, côté background :
  - il envoie `auth` avec `browser` et `version` ;
  - il garde `ignored_sites` reçu de `config` dans `chrome.storage.local` (cache pour une capture hors connexion) ;
  - il expose à la fenêtre l'état `connected | rejected | offline` ;
  - il gère `cmd: "setSiteIgnored"`, qui envoie `set_site_ignored`.
  - Les commandes inutilisées `listMedia`, `sendMedia` et `sendPage` sont retirées.
- Produces, côté fenêtre de l'extension :
  - la ligne « Capture automatique » sur l'hôte de l'onglet actif, avec son interrupteur et la mention « est ignoré », selon la maquette 06 ;
  - la vue « Non connecté », avec un texte qui distingue « FlashGet n'est pas lancé » de « le jeton ne correspond plus ».
- Produces, côté options : le champ « Jeton » reste, le champ « Port » disparaît, et une ligne affiche l'état de la connexion.
- Produces : un README à jour (installation en mode développeur et jeton).

- [ ] **Step 1 : Tests qui échouent :**
  - `isIgnored` (Review Focus #2) ;
  - `hostOf` sur des URL invalides ;
  - `browserName` : marques Edge, Brave, Chrome, et sans `userAgentData` ;
  - `shouldCapture` avec un site ignoré, par l'URL et par le référent.
- [ ] **Step 2 :** constater l'échec, implémenter, puis `npm test -- --maxWorkers=2`.
- [ ] **Step 3 :** commit `feat(extension): état de connexion, sites ignorés, version 2.0.0`.

### Task 5 : Interface — section Extension et barre d'état

**Files :**
- Modify : `src/api.ts`, `src/types.ts`, `src/App.tsx`, `src/components/shell/StatusBar.tsx`, `src/components/settings/ExtensionSection.tsx`, `src/i18n/fr.ts`.
- Test : `src/components/settings/extension.test.tsx`, plus les tests existants de StatusBar et de `sections.test.tsx` adaptés.

**Interfaces :**
- Consumes : `get_bridge_status`, `bridge-status`, `get_ignored_sites`, `set_ignored_sites`, `ignored-sites-changed`, `open_extension_folder`, `get_bridge_info`, `regenerate_bridge_token`.
- Produces (`api.ts`) : `getBridgeStatus()`, `onBridgeStatus(cb)`, `getIgnoredSites()`, `setIgnoredSites(sites)`, `onIgnoredSitesChanged(cb)`, `openExtensionFolder()`, `retryCapture(captureId, url)` (pour la Task 6), et le type `BridgeStatus`.
- Produces : `App` tient `bridgeStatus` (lu au démarrage, puis mis à jour par l'événement) et le passe à `StatusBar` et à `SettingsPage` → `ExtensionSection`. La prop `bridgeListening` disparaît.

- [ ] **Step 1 : Tests qui échouent :**
  - **barre d'état** :
    - `connected` → point vert, « Extension : connectée » ;
    - `waiting` → point en contour, « Extension : en attente » ;
    - `inactive` → point rouge, « Extension : inactive » ;
    - `rejected` → point rouge, « Extension : jeton refusé » ;
  - **carte Connexion** : les 4 états, avec leurs textes de la maquette (navigateur, version en mono, « active à l'instant » ou « il y a N min ») ;
  - **jeton** : Copier, puis Régénérer après confirmation ;
  - **Installation** : « Copier l'adresse » copie `chrome://extensions`, « Ouvrir le dossier » appelle `openExtensionFolder`, et son erreur s'affiche ;
  - **sites ignorés** :
    - « Ajouter » appelle `setIgnoredSites` avec la liste complétée ;
    - une erreur Rust s'affiche et la liste reste inchangée ;
    - le retrait d'un site ;
    - l'événement `ignored-sites-changed` met la liste à jour.
- [ ] **Step 2 :** constater l'échec.
- [ ] **Step 3 :** implémenter selon la maquette 06.
- [ ] **Step 4 :** `npm test -- --maxWorkers=2` et `npx tsc --noEmit`, puis commit `feat(ui): section Extension, état réel de la connexion`.

### Task 6 : Interface — « Réessayer » avec le contexte

**Files :**
- Modify : `src/App.tsx` (type `ExternalPayload`), `src/pages/NewPage.tsx`, les hooks d'analyse si besoin (`useLinkProbe.ts`), `src/i18n/fr.ts`.
- Test : les tests existants de `NewPage`, plus le nouveau cas.

**Interfaces :**
- Consumes : `retryCapture(captureId, url)` (Task 5), et `link://error {url, message, capture_id?, origin?}`.
- Produces : `ExternalEvent` d'erreur porte `captureId?` et `origin?`. Quand l'erreur vient d'une capture :
  - la page affiche « Envoyé par l'extension depuis {origin} : « Réessayer » renvoie aussi les cookies et la page d'origine. » ;
  - « Réessayer » appelle `retryCapture`. La page passe alors par les mêmes états qu'une capture : `link://probing`, puis `link://probed` ou `link://error`.

  Sans `captureId`, rien ne change.

- [ ] **Step 1 : Tests qui échouent :**
  - une erreur avec `captureId` et `origin` affiche le texte, et « Réessayer » appelle `retryCapture("id", url)` sans appeler `probeLink` ;
  - une erreur sans `captureId` garde l'ancien chemin.
- [ ] **Step 2 :** constater l'échec, implémenter, puis `npm test -- --maxWorkers=2` et `npx tsc --noEmit`.
- [ ] **Step 3 :** commit `feat(ui): Réessayer renvoie le contexte de la capture`.

### Task 7 : Vérification d'ensemble (contrôleur et propriétaire)

- [ ] **Automatique :** `npm test -- --maxWorkers=2`, `npx tsc --noEmit` et `npm run build`, puis `cargo test --workspace -j 1` et `cargo build -j 1`.
- [ ] **Manuelle, par le propriétaire, avec `npm run tauri dev` :**
  1. Paramètres › Extension :
     - « Ouvrir le dossier » ouvre le dossier de l'extension ;
     - recharger l'extension dans `chrome://extensions` fait passer l'état à « Connectée », avec le navigateur et la version `2.0.0`, et la barre d'état suit ;
     - fermer le navigateur fait revenir l'état à « En attente ».
  2. Un mauvais jeton dans les options de l'extension :
     - l'application affiche « Jeton refusé » ;
     - la fenêtre de l'extension dit que le jeton ne correspond plus.
  3. Ajouter `github.com` aux sites ignorés :
     - un téléchargement depuis github.com n'est plus capturé ;
     - le clic droit « Télécharger avec FlashGet » fonctionne toujours ;
     - l'interrupteur de la fenêtre de l'extension sur github.com est désactivé, et le réactiver retire le site de la liste dans l'application.
  4. Une capture qui échoue (lien protégé), puis « Réessayer » : le texte « Envoyé par l'extension depuis … » s'affiche, et l'analyse est relancée avec le contexte.
- [ ] **Fin de phase :** `superpowers:finishing-a-development-branch`.
