# Phase 3b-2 — Capture automatique des téléchargements — Design

**Date :** 2026-06-03
**Statut :** Validé (design approuvé en brainstorming) — en attente de relecture utilisateur puis plan d'implémentation
**Projet :** FlashGet (clone d'IDM) — voir [spec produit](2026-06-02-idm-clone-design.md)
**Pré-requis :** Phases 1, 2a, 2b, 3a, **3b-1** (cookies/referer/user-agent) — fusionnées dans `main`.

## 1. Objectif et périmètre

**3b-2 livre la capture automatique :** l'extension intercepte les téléchargements déclenchés par le navigateur et les redirige vers FlashGet, sans clic droit, en réutilisant le transfert de cookies/referer/user-agent de 3b-1. Filtrage **par liste blanche d'extensions** (façon IDM), interrupteur maître **activé par défaut**, et transmission du **nom de fichier** résolu par le navigateur.

**Hors périmètre 3b-2 (→ plus tard) :** filtre par taille minimale ; règles par domaine ; corrélation `ack{id}` → popup ; chiffrement des cookies au repos ; Firefox ; média/HLS/DASH (P4). La capture **manuelle** (clic droit / popup) de 3a/3b-1 reste disponible et inchangée.

## 2. Décisions de conception (validées en brainstorming)

- **Filtrage :** **liste blanche d'extensions** configurable (par défaut basée sur le catégoriseur du cœur), éditable dans les options de l'extension.
- **Interrupteur maître :** capture automatique **ON par défaut** (expérience « ça marche tout seul », conforme à IDM).
- **Nom de fichier :** transmettre `item.filename` (résolu par le navigateur, Content-Disposition inclus) ; `add_url` l'utilise quand présent, sinon retombe sur le nom déduit du probe/URL. Le champ `filename` existe déjà dans le protocole (parsé mais jusqu'ici ignoré).
- **Hook d'interception :** `chrome.downloads.onDeterminingFilename` (fournit le nom résolu + URL + referer) ; repli possible sur `onCreated` si nécessaire à l'implémentation.
- **Garde « seulement si connecté » :** on n'intercepte (annule le download navigateur) **que si** l'extension est connectée+authentifiée à l'app, sinon le navigateur télécharge normalement (ne jamais faire perdre un download quand FlashGet est fermé).

## 3. Architecture & flux

```
Navigateur démarre un download → chrome.downloads.onDeterminingFilename(item)
  item = { id, url, finalUrl, filename (résolu), referrer, mime, fileSize }
  Si autoCapture ON ET connected ET shouldCapture(item, settings) :
     → chrome.downloads.cancel(item.id) PUIS chrome.downloads.erase({id})
     → sendCapture(finalUrl||url, item.referrer, basename(item.filename))
       → gatherHeaders(url, referrer) + buildCapture(url, {referrer,cookie,user_agent,filename})
  → bridge: ClientMessage::Capture → ServerReply::CaptureRequested{url, filename, referrer, cookie, user_agent}
  → server.rs: add_url(app, url, "", RequestHeaders, filename)
  → add_url: file_name = choose_file_name(filename, probe.file_name)
```

Côté FlashGet, le download auto-capturé suit **exactement** le pipeline existant (sonde avec en-têtes 3b-1 → catégorie → file → reevaluate), émet `download://added`, et fait surgir la fenêtre.

## 4. Composants

### 4.1 Extension — logique pure (nouveau module `extension/capture-rules.js`, testable Vitest)
- `extensionOf(nameOrUrl: string) -> string` : extrait la dernière extension en minuscules, en ignorant query/fragment d'une URL (`archive.tar.gz` ⇒ `gz` ; `https://h/f.zip?x=1` ⇒ `zip` ; sans extension ⇒ `""`).
- `parseCaptureTypes(text: string) -> string[]` : découpe sur virgules/espaces/sauts de ligne, met en minuscules, retire les points de tête, dédoublonne, ignore les vides.
- `shouldCapture(item, settings) -> boolean` où `item = { url, filename }` et `settings = { autoCapture: bool, types: string[] }`. Renvoie `true` ssi : `settings.autoCapture` est vrai ET `url` commence par `http://`/`https://` ET `extensionOf(item.filename) || extensionOf(item.url)` ∈ `settings.types`. (La garde **connected** est vérifiée par l'appelant, pas par cette fonction pure.)

### 4.2 Extension — `background.js`
- Listener `chrome.downloads.onDeterminingFilename((item, suggest) => …)` :
  - Lit les réglages (`autoCapture`, `captureTypes`) via `getConfig`/`chrome.storage.local`.
  - Si `autoCapture && connected && shouldCapture({url: item.finalUrl||item.url, filename: item.filename}, {autoCapture, types})` :
    - `chrome.downloads.cancel(item.id)` puis `chrome.downloads.erase({ id: item.id })` (best-effort, try/catch) ;
    - `sendCapture(item.finalUrl || item.url, item.referrer, basename(item.filename))`.
  - Sinon : ne pas interférer (le navigateur poursuit avec son nom par défaut). La capture n'a pas besoin de `suggest()`.
- `sendCapture(url, referrer, filename)` : signature étendue d'un `filename` optionnel, propagé jusqu'à `buildCapture(url, { …, filename })` ; la file `pending` stocke désormais `{ url, referrer, filename, ts }` (les cookies restent (re)collectés frais à l'envoi via `gatherHeaders`).
- `gatherHeaders` inchangé. Le referer d'une auto-capture est `item.referrer` (plus fiable que l'onglet actif).

### 4.3 Extension — `buildCapture.js`
- Ajouter `filename` à la liste des clés autorisées de `extras` (déjà : `referrer`, `cookie`, `user_agent`). Les valeurs vides restent omises. `buildCapture` reste **pur**.

### 4.4 Extension — `options.html` / `options.js` / `manifest.json`
- `manifest.json` : ajouter la permission **`downloads`** (en plus de `contextMenus, storage, activeTab, alarms, cookies`).
- `options.html` : case à cocher **« Capture automatique des téléchargements »** + zone de texte **« Types capturés »** (extensions séparées par des virgules).
- `options.js` : charge/enregistre `autoCapture` (booléen, **défaut `true`**) et `captureTypes` (texte, **défaut = liste §4.6**) dans `chrome.storage.local`, à côté de `token`/`port`. Au save, envoie un message `reconnect` comme aujourd'hui.
- `getConfig` (background) lit aussi `autoCapture` (défaut true) et `captureTypes` (défaut liste), et expose `parseCaptureTypes(captureTypes)`.

### 4.5 Backend — branchement du `filename` (petit)
- **`download-core`** : nouveau helper **pur** `choose_file_name(suggested: Option<&str>, probe: Option<&str>) -> String` : renvoie `suggested` nettoyé (basename, non vide) sinon `probe` non vide sinon `"download.bin"`. (Emplacement : un petit module du cœur, p. ex. `naming.rs`, ou une fonction libre réutilisable.)
- **`commands::add_url`** : nouveau param `suggested_name: Option<String>` ; `let file_name = choose_file_name(suggested_name.as_deref(), probe.file_name.as_deref());`. La garde existante `dest_path.parent() != Some(dest_dir)` rejette toujours un nom contenant un séparateur (anti-traversée).
- **`bridge::server.rs`** : passe `filename` (de `CaptureRequested`) en 5e argument d'`add_url`.
- **`add_download`** (commande UI manuelle) : passe `None`.

## 5. Garde-fous, sécurité & limites

- **Connecté uniquement** : pas d'annulation du download navigateur si l'app n'est pas joignable (sinon perte). La file `pending` couvre une coupure brève (réveil du service worker MV3).
- **Schémas non récupérables** : `blob:`/`data:`/`file:`/`chrome:` toujours laissés au navigateur.
- **Pas de boucle** : FlashGet télécharge via `reqwest` (processus séparé), donc `onDeterminingFilename` ne se redéclenche pas.
- **Nom de fichier sûr** : le navigateur fournit un nom déjà nettoyé ; `choose_file_name` en prend le basename et la garde anti-traversée d'`add_url` protège en défense en profondeur.
- **Limite assumée** : les URL à jeton à usage unique peuvent expirer entre l'annulation navigateur et la requête FlashGet (re-téléchargement) → le download apparaît en erreur (limite inhérente à la redirection, comme IDM).
- **Permission `downloads`** : avertissement navigateur « gérer vos téléchargements » assumé.

## 6. Stratégie de tests

- **Vitest (pur)** : `shouldCapture` (désactivé ⇒ false ; `blob:`/`data:` ⇒ false ; type listé ⇒ true ; hors liste ⇒ false ; extension prise du `filename` à défaut de l'URL) ; `parseCaptureTypes` (virgules/espaces/points/casse/doublons) ; `extensionOf` (URL avec query, multi-points, sans extension) ; `buildCapture` (inclut `filename`, omet vide).
- **Rust (TDD)** : `choose_file_name` (suggéré non vide ⇒ suggéré ; suggéré vide/None ⇒ probe ; les deux absents ⇒ `download.bin`).
- **Manuel (smoke-test)** : auto-capture ON + connecté → cliquer un lien `.zip` ⇒ apparaît dans FlashGet, **le navigateur ne télécharge pas**, et le **nom du fichier** est correct ; un `.txt` (hors liste) ⇒ le navigateur télécharge normalement ; un bouton générant un `blob:` ⇒ le navigateur gère ; app fermée ⇒ le navigateur télécharge (aucune interception) ; décocher la case dans les options ⇒ plus d'interception.

## 7. Découpage prévisionnel du plan (à détailler en writing-plans)

1. `extension/capture-rules.js` : `extensionOf` + `parseCaptureTypes` + `shouldCapture` — TDD Vitest.
2. `buildCapture` : autoriser la clé `filename` — TDD Vitest.
3. Backend : `choose_file_name` (cœur, TDD) + `add_url(suggested_name)` + `server.rs` passe `filename` + `add_download` passe `None`.
4. Extension : `manifest` (perm `downloads`), `getConfig` lit `autoCapture`/`captureTypes`, listener `onDeterminingFilename` (garde connected), `sendCapture(url, referrer, filename)` + file `pending` enrichie.
5. Options : case « Capture automatique » + champ « Types capturés » (défauts), persistance `chrome.storage.local`.
6. Vérification (suites + smoke-test auto-capture / hors-liste / blob / app fermée / nom de fichier).

## 8. Décisions reportées / hors 3b-2

- Filtre par taille minimale ; règles d'inclusion/exclusion par domaine ; corrélation `ack{id}` → popup (« Envoyé ✓ » optimiste) ; chiffrement des cookies au repos ; cookies cross-domaine après redirection ; Firefox ; média/HLS/DASH (P4).
- Backlog technique antérieur (2a/2b/3a) toujours ouvert : `get_category_dirs` atomique ; re-catégorisation si le nom final diffère.
