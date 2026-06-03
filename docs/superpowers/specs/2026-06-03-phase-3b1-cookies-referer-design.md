# Phase 3b-1 — Transfert cookies / session / referer / user-agent — Design

**Date :** 2026-06-03
**Statut :** Validé (design approuvé en brainstorming) — en attente de relecture utilisateur puis plan d'implémentation
**Projet :** FlashGet (clone d'IDM) — voir [spec produit](2026-06-02-idm-clone-design.md)
**Pré-requis :** Phases 1, 2a, 2b, **3a** (bridge navigateur + extension MV3) — fusionnées dans `main`.

## 1. Objectif et périmètre

La Phase 3b (intégration navigateur, suite) est scindée en **3b-1** (ce document) et **3b-2** (capture automatique, plus tard).

**3b-1 livre les téléchargements authentifiés :** transmettre du navigateur vers l'app les **cookies (session/login), le referer et le user-agent** d'un lien capturé, et faire en sorte que le moteur Rust **utilise ces en-têtes** dans ses requêtes HTTP (sonde + segments + reprise). Bénéficie **immédiatement à la capture manuelle de 3a** (clic droit / popup) : les liens derrière une authentification ou une protection hotlink/referer fonctionnent désormais.

**Hors périmètre 3b-1 (→ 3b-2 ou plus tard) :** capture **automatique** (interception `chrome.downloads`) = 3b-2 ; cookies cross-domaine après redirection vers un CDN d'un autre domaine (limite assumée) ; chiffrement des cookies au repos (amélioration future) ; Firefox ; média/HLS (P4).

## 2. Décisions de conception (validées en brainstorming)

- **En-têtes transmis :** **Cookies + Referer + User-Agent** (le trio IDM).
- **Stockage :** **persistés en base** (3 colonnes sur `downloads`) → la reprise après redémarrage de l'app fonctionne pour les liens authentifiés. Contrepartie assumée : cookies de session écrits en clair dans le SQLite du profil utilisateur (même posture qu'IDM).
- **Permissions extension :** **accès large `<all_urls>`** + permission `cookies` (nécessaire pour lire les cookies `httpOnly` de n'importe quel lien capturé).
- **Threading backend :** **struct `RequestHeaders` + paramètre du trait `ByteFetcher`** (approche A) — explicite, typé, testable ; conserve le `reqwest::Client` mutualisé (en-têtes posés **par requête**).
- **Transfert toujours actif** pour un lien explicitement capturé (pas de toggle en 3b-1 — YAGNI).
- **Referer** = URL de la **page d'origine** (`info.pageUrl` en clic droit ; URL de l'onglet courant en popup), pas le lien lui-même.

## 3. Architecture & flux de données

```
Extension (capture manuelle 3a, enrichie)              App FlashGet
 • url                                                  ws (jeton) ─▶ ClientMessage::Capture{
 • referrer = page d'origine                                              url, referrer, cookie, user_agent }
 • user_agent = navigator.userAgent                     ─▶ server.rs: add_url(app, url, "", RequestHeaders)
 • cookie = chrome.cookies.getAll({url})                ─▶ Download{…, referrer, cookie, user_agent} (persisté)
   → "n1=v1; n2=v2"                                      ─▶ moteur: probe/fetch_range(url, range, &headers)
                                                         ─▶ reqwest pose Referer/Cookie/User-Agent PAR requête
```

## 4. Composants

### 4.1 Protocole bridge (Rust, pur, TDD) — `src-tauri/src/bridge/protocol.rs`
- `RawMessage::Capture` et `ClientMessage::Capture` gagnent `cookie: Option<String>` et `user_agent: Option<String>` (le champ `referrer` existe déjà depuis 3a, jusqu'ici inerte).
- `parse_client_message` mappe les nouveaux champs (`#[serde(default)]`).
- `handle_message` les porte dans `ServerReply::CaptureRequested{ url, referrer, cookie, user_agent }`.
- Règles d'auth inchangées.

### 4.2 Cœur de téléchargement — `src-tauri/crates/download-core/`
- **Modèle** (`model.rs`) : `Download` gagne `referrer: Option<String>`, `cookie: Option<String>`, `user_agent: Option<String>`. Nouveau struct **`RequestHeaders { referrer: Option<String>, cookie: Option<String>, user_agent: Option<String> }`** (+ méthode pratique `Download::headers() -> RequestHeaders` ou équivalent).
- **Storage** (`storage.rs`) : 3 colonnes ajoutées par migration additive `ALTER TABLE downloads ADD COLUMN … TEXT` (erreur ignorée si déjà présente, comme `priority`/`category`). `upsert`, `list`, `get_by_id` lisent/écrivent les 3 colonnes (NULL si absent). Tests round-trip.
- **`ByteFetcher`** (`http.rs`) : signatures étendues
  - `async fn probe(&self, url: &str, headers: &RequestHeaders) -> Result<ProbeResult, CoreError>`
  - `async fn fetch_range(&self, url: &str, range: Option<(u64,u64)>, headers: &RequestHeaders) -> Result<BoxStream<…>, CoreError>`
  - `ReqwestFetcher` pose `Referer`, `Cookie`, `User-Agent` sur **chaque** requête quand présents (le `User-Agent` par requête écrase celui par défaut du client). Client partagé conservé.
  - Le `FakeFetcher` des tests implémente la nouvelle signature et peut **vérifier** les en-têtes reçus.
- **Moteur** (`engine.rs`) : `engine::download(…)` reçoit les `RequestHeaders` (nouveau paramètre) et les passe à `probe` et à chaque `fetch_range` (segments + reprise via `resume.rs`).

### 4.3 App (Rust) — `src-tauri/src/`
- **`commands::add_url`** : nouvelle signature `add_url(app: &AppHandle, url: &str, dest_dir: &str, headers: RequestHeaders) -> Result<Download,String>` — construit le `Download` avec les en-têtes, les persiste, lance la file. La commande Tauri **`add_download`** (ajout manuel via l'UI) passe des `RequestHeaders` **vides**.
- **`bridge::server`** : sur `CaptureRequested{url, referrer, cookie, user_agent}` → `add_url(&app, &url, "", RequestHeaders{referrer, cookie, user_agent})`. Émission `download://added` + remontée fenêtre inchangées.
- **`launch`/relais** : le moteur reçoit les en-têtes depuis le `Download` (via storage) au moment du lancement/reprise.

### 4.4 Extension (Chromium MV3) — `extension/`
- **`manifest.json`** : `permissions` += `"cookies"` ; `host_permissions` += `"<all_urls>"` (conserver `ws://127.0.0.1/*`).
- **`background.js`** : nouvelle async `gatherHeaders(url, referrer)` → `chrome.cookies.getAll({url})` assemblés en `"n1=v1; n2=v2"`, `user_agent = navigator.userAgent`, `referrer`. Le menu contextuel utilise `info.pageUrl` comme referer ; le popup utilise l'URL de l'onglet courant. La file d'attente (`pending`) stocke désormais `{ url, referrer }` (les cookies sont (re)collectés au moment de l'envoi, frais).
- **`buildCapture.js`** reste **pur** : `buildCapture(url, extras)` → `{ type:"capture", url, ...extras }` (extras = `{referrer, cookie, user_agent}` filtrés). Testé Vitest. La collecte `chrome.cookies` vit dans `background.js`, hors de `buildCapture`.

## 5. Sécurité / confidentialité

- Cookies transmis sur la WebSocket **loopback déjà authentifiée par jeton** (3a).
- Cookies/UA/referer **persistés en clair** dans le SQLite du profil utilisateur (`Roaming\com.steph.tauri-app\downloads.sqlite`) — posture assumée, documentée ; périmètre de confiance = compte utilisateur OS. Chiffrement au repos = amélioration future.
- Les en-têtes ne servent **qu'**aux requêtes du téléchargement concerné. Le bridge n'expose toujours **que** la création de téléchargements.
- Permission `<all_urls>` : avertissement navigateur « lire vos données sur tous les sites » assumé (nécessaire pour lire les cookies de n'importe quel lien).

## 6. Stratégie de tests

- **Protocole (TDD, pur)** : `parse_client_message` d'une capture avec `cookie`/`user_agent` ; `handle_message` après auth → `CaptureRequested` porte cookie/referer/UA ; rétro-compat (capture sans ces champs ⇒ `None`).
- **Storage (TDD)** : `upsert` puis `list`/`get_by_id` round-trip des 3 colonnes (présents et NULL).
- **`ReqwestFetcher` (intégration `tiny_http`)** : le serveur de test reçoit bien les en-têtes `Cookie`, `Referer`, `User-Agent` quand fournis ; absents quand `RequestHeaders` vide.
- **`buildCapture` (Vitest)** : assemble `{type:"capture",url,referrer,cookie,user_agent}` ; champs absents omis.
- **Manuel (smoke-test)** : capturer un lien derrière login (cookie de session présent dans le navigateur) → le téléchargement réussit dans l'app ; vérifier qu'un lien public marche toujours (rétro-compat).

## 7. Découpage prévisionnel du plan (à détailler en writing-plans)

1. `RequestHeaders` + champs `Download` + migration `storage` (3 colonnes) — TDD round-trip.
2. `ByteFetcher` : signatures + `ReqwestFetcher` pose les en-têtes + adapter `FakeFetcher` — TDD intégration `tiny_http`.
3. Moteur : threader `RequestHeaders` (`engine::download`, segments, resume).
4. Protocole bridge : `cookie`/`user_agent` dans `Capture`/`CaptureRequested` — TDD pur.
5. `add_url` + `server.rs` : passer les en-têtes du bridge au pipeline ; `add_download` UI passe des en-têtes vides.
6. Extension : `manifest` (cookies + `<all_urls>`), `gatherHeaders`, `buildCapture(url, extras)` + Vitest, file `pending` enrichie.
7. Vérification (suites + smoke-test lien authentifié).

## 8. Décisions reportées / hors 3b-1

- **3b-2** : capture **automatique** (interception `chrome.downloads`, annuler le téléchargement navigateur et le rediriger vers FlashGet), réutilise 3b-1 pour les cookies.
- Cookies cross-domaine après redirection (CDN d'un autre domaine) ; chiffrement des cookies au repos ; toggle on/off du transfert ; Firefox ; média/HLS (P4).
- Backlog technique antérieur (2a/2b/3a) toujours ouvert : `get_category_dirs` atomique ; re-catégorisation si le nom final diffère ; corrélation `ack{id}` → popup (« Envoyé ✓ » optimiste) ; limiteur fast-path lock-free.
