# Phase 3a — Intégration navigateur : tuyau de capture manuelle — Design

**Date :** 2026-06-03
**Statut :** Validé (design, 4 sections approuvées) — en attente de relecture finale puis plan d'implémentation
**Projet :** FlashGet (clone d'IDM) — voir [spec produit](2026-06-02-idm-clone-design.md)
**Pré-requis :** Phases 1, 2a, 2b — fusionnées dans `main`.

## 1. Objectif et périmètre

La Phase 3 (intégration navigateur) est scindée en **3a** (ce document) et **3b** (plus tard).

**3a livre le tuyau de bout en bout :** un serveur local dans l'app + une extension Chromium (MV3) permettant la **capture manuelle** de liens (clic droit « Télécharger avec FlashGet » et popup pour envoyer l'onglet courant / une URL collée).

**Hors périmètre 3a (→ 3b ou plus tard) :** capture **automatique** (interception des téléchargements du navigateur), transfert des **cookies/session/referer**, support **Firefox**, capture média/streaming (HLS/DASH = Phase 4).

## 2. Décisions de conception (validées en brainstorming)

- **Transport :** serveur **local dans l'app** (pas de native messaging host). Écoute **loopback** `127.0.0.1`.
- **Protocole :** **WebSocket** (connexion persistante → indicateur « connecté » live + accusés ; bonne base pour 3b).
- **Sécurité :** **jeton** obligatoire (auth avant toute capture), généré au 1er lancement, régénérable ; appairage par **copier-coller manuel** du jeton (app → options extension).
- **Navigateurs :** **Chrome + Edge** (un seul codebase Chromium MV3). Firefox plus tard.
- **Port :** **15151** par défaut (configurable dans les options de l'extension).

## 3. Architecture & composants

```
Extension Chromium (MV3)  ──ws://127.0.0.1:15151 (auth jeton)──▶  App FlashGet (module `bridge`)
 • service worker : WS + reconnexion + statut          {type:"auth",token}      • serveur WS loopback
 • menu contextuel « Télécharger avec FlashGet »        {type:"capture",url,…}   • valide le jeton
 • popup : statut + URL collée + onglet courant        ◀─{type:"ack",id}/error─  • crée le téléchargement via
 • options : coller le jeton (+ port)                                              le pipeline add_download existant
```

### 3.1 App (Rust) — nouveau module `bridge`
- Serveur **WebSocket** sur `127.0.0.1:15151`, lancé au démarrage via **`tauri::async_runtime::spawn`** (jamais `tokio::spawn` direct — piège corrigé en 2a).
- **Logique de protocole pure et testable**, séparée de l'I/O réseau :
  - `parse_client_message(&str) -> ClientMessage` (Auth{token} | Capture{url, filename?, referrer?} | Unknown).
  - `handle_message(authed: bool, expected_token, msg) -> (new_authed, ServerReply)` où `ServerReply` ∈ { Authed | AuthFailed | NeedAuth | CaptureRequested{url,…} | Error{msg} }. Règle clé : une `Capture` reçue avant un `Auth` valide ⇒ `NeedAuth` (pas de capture).
- La **boucle WS** (fine couche) : accepte la connexion loopback, lit les messages, appelle `handle_message`, et sur `CaptureRequested` invoque le **pipeline d'ajout existant** (sonde → `categorize` → dossier catégorie → file d'attente), puis renvoie `ack{id}` ou `error`.
- Réutilise toute la logique 2b (catégorisation, dossiers, file) — le bridge ne fait que recevoir l'URL.

### 3.2 Stockage / réglages
- `storage` : `get_or_create_bridge_token() -> String` (persisté en table `settings`, clé p.ex. `bridge.token`, généré aléatoire fort si absent) et `regenerate_bridge_token() -> String`.
- Commandes Tauri : `get_bridge_info() -> { token, port, listening }` et `regenerate_bridge_token() -> String`.

### 3.3 Extension (Chromium MV3) — nouveau dossier `extension/`
- `manifest.json` (MV3) : permissions `contextMenus`, `storage`, `activeTab` ; host permission `ws://127.0.0.1:15151/*`.
- `background.js` (service worker) : connexion WS, auth au connect, **reconnexion automatique**, exposition de l'état (connecté/déconnecté) ; envoie les captures.
- Menu contextuel « Télécharger avec FlashGet » sur cibles lien/image/vidéo/audio.
- `popup.html`/`popup.js` : indicateur connecté/déconnecté, champ URL (collage), bouton « Envoyer l'onglet courant », retour « Envoyé ✓ ».
- `options.html`/`options.js` : champ jeton (+ port), stockés via `chrome.storage`.
- Logique pure isolée : `buildCapture(url) -> message` (testable Vitest).

## 4. Flux, sécurité & limites

**Flux :** clic droit/lien ou popup → `{capture,url}` sur la WS authentifiée → bridge crée le téléchargement (pipeline existant) → `ack{id}` → popup affiche « Envoyé ✓ ».

**Sécurité :** écoute 127.0.0.1 uniquement ; jeton obligatoire (sinon socket refusée/fermée) ; le bridge n'expose **que** la création de téléchargements (aucune lecture de fichiers ni autre commande).

**Limites connues (assumées en 3a) :**
- **Pas de transfert de cookies/session/referer** → les téléchargements derrière authentification peuvent échouer (traité en 3b).
- **L'app doit être lancée** pour capturer.
- **Capture automatique = 3b.**

**Erreurs :** WS déconnectée → popup « FlashGet non détecté, lancez l'application » ; jeton invalide → « Jeton incorrect, vérifiez les options » ; URL/sonde en échec → l'erreur du pipeline remonte en `error`.

## 5. Interface (validée via maquettes)

- **Popup connecté** : pastille « ● Connecté », champ URL + « Envoyer à FlashGet », « Envoyer l'onglet courant », toast « Envoyé ✓ ».
- **Popup déconnecté** : « ● Non détecté » + message + boutons Options / Réessayer.
- **Options extension** : jeton (collé) + port (15151).
- **App → Réglages → section « 🧩 Extension navigateur »** : jeton affiché + Copier, état du serveur local (à l'écoute), bouton Régénérer le jeton.

## 6. Stratégie de tests

- **`bridge::handle_message` (Rust, TDD, pur)** : `capture` sans `auth` ⇒ NeedAuth ; `auth` bon/mauvais jeton ⇒ Authed/AuthFailed ; `capture` après auth ⇒ CaptureRequested{url} ; JSON malformé ⇒ Error.
- **`storage` (TDD)** : `get_or_create_bridge_token` stable + non vide ; `regenerate` change la valeur.
- **Extension (Vitest)** : `buildCapture` produit le bon payload.
- **Manuel (smoke-test)** : charger l'extension non empaquetée dans Chrome, coller le jeton, clic droit sur un lien → apparaît dans la file ; app fermée → popup « non détecté ».

## 7. Découpage prévisionnel du plan (à détailler en writing-plans)

1. `bridge` protocole pur (`parse_client_message` + `handle_message`) — TDD.
2. Jeton en `storage` (`get_or_create_bridge_token`/`regenerate`) — TDD.
3. Serveur WS (boucle loopback) + commandes Tauri `get_bridge_info`/`regenerate_bridge_token` + lancement au démarrage (async_runtime::spawn) + branchement sur le pipeline d'ajout.
4. UI app : section « Extension » dans `SettingsDialog` (jeton/copier/régénérer/état).
5. Extension Chromium MV3 : manifest, background (WS + reconnexion + auth), menu contextuel, popup, options ; `buildCapture` + test Vitest.
6. Vérification (suites + smoke-test manuel Chrome).

## 8. Décisions reportées / hors 3a

- Cookies/referer/session forwarding ; capture automatique (interception) ; Firefox ; média/HLS (P4).
- Choix de la lib WebSocket Rust (à trancher au plan : p. ex. `tokio-tungstenite`, ou réutiliser une lib HTTP+WS légère). Aucune nouvelle dépendance encore engagée.
- Backlog technique antérieur (2a/2b) toujours ouvert : fast-path lock-free du limiteur ; snapshot atomique de `get_category_dirs` ; re-catégorisation si le nom final diffère ; test e2e d'erreur.
