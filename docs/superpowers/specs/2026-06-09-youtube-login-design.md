# Connexion YouTube intégrée (cookies persistants pour yt-dlp) — Design

**Date :** 2026-06-09
**Statut :** validé
**Module :** FlashGet (Tauri v2 — Rust + React ; yt-dlp sidecar)

## Intention

Permettre à l'utilisateur de **se connecter à son compte YouTube DANS l'app** (style 4K Video
Downloader), de sorte que les cookies de session soient **toujours présents et frais**. Ainsi,
**coller un lien YouTube suffit** : yt-dlp télécharge avec le compte (vidéos privées/membres/âge,
anti-bot), sans dépendre des cookies de l'extension (qui ne voit pas les cookies **httpOnly**
d'auth et n'existe pas quand on colle un lien directement dans l'app).

## Problème actuel

yt-dlp reçoit aujourd'hui les cookies via `--add-header Cookie:<v>` issus de l'extension
(`chrome.cookies`). Limites : (1) coller un lien dans l'app = aucun cookie ; (2) les cookies
httpOnly de session YouTube (`SID`, `SAPISID`, `__Secure-3PSID`…) ne sont pas récupérables par
`chrome.cookies`/`document.cookie`. D'où les échecs.

## Faisabilité (confirmée)

Tauri 2.11.2 expose `WebviewWindow::cookies_for_url(url) -> Vec<cookie::Cookie>` et `cookies()`
qui renvoient les cookies du magasin WebView2 **y compris httpOnly**. On peut donc exporter un
vrai `cookies.txt`. WebView2 utilise un dossier de données **persistant** partagé par les
fenêtres de l'app → la session de connexion survit aux redémarrages, et les cookies posés dans
la fenêtre de login sont lisibles depuis le webview de la fenêtre `main` (magasin partagé).

## Approche retenue

**Connexion intégrée (primaire) + repli « cookies du navigateur » (optionnel).**

## Architecture

### 1. Cœur pur (testable) — `download-core/src/cookies.rs` (nouveau)
- `pub struct NetscapeCookie { pub domain: String, pub path: String, pub secure: bool, pub http_only: bool, pub expires: Option<i64>, pub name: String, pub value: String }`
- `pub fn to_netscape(cookies: &[NetscapeCookie]) -> String` — produit un fichier **Netscape** :
  - 1re ligne `# Netscape HTTP Cookie File`.
  - une ligne par cookie : `domaine\tINCLUDE_SUB\tpath\tSECURE\texpiry\tnom\tvaleur`, où
    `INCLUDE_SUB` = `TRUE` si `domain` commence par `.` sinon `FALSE` ; `SECURE` = `TRUE`/`FALSE` ;
    `expiry` = epoch (`expires` ou `0` pour un cookie de session).
  - **httpOnly** : préfixer le domaine par `#HttpOnly_` (convention lue par yt-dlp/curl).
- `pub fn cookies_apply_to(url: &str) -> bool` — `true` si l'hôte appartient à YouTube/Google
  (`youtube.com`, `youtu.be`, `googlevideo.com`, `google.com`, `accounts.google.com`,
  `ytimg.com` non requis) ; sert à n'attacher les cookies qu'à ces domaines.
- `pub enum YtCookieArg { File(String), Browser(String), None }` +
  `pub fn choose_cookie_arg(url: &str, has_file: bool, browser_fallback: Option<&str>) -> YtCookieArg`
  - si `cookies_apply_to(url) && has_file` → `File(path)` *(le path est injecté par l'appelant ;
    la fonction décide juste du type — signature exacte affinée au plan : prend `has_file: bool`)*
  - sinon si `cookies_apply_to(url)` et `browser_fallback = Some(b)` → `Browser(b)`
  - sinon → `None`.
- Tests : format Netscape (httpOnly préfixé, session→0, secure, include-sub) ; `cookies_apply_to`
  (youtube/youtu.be/google = true, autres = false) ; `choose_cookie_arg` (3 branches).

### 2. Glue Tauri — `src-tauri/src/youtube.rs` (nouveau) + commandes
- `youtube_login(app)` : construit une `WebviewWindow` label `yt-login`, URL externe
  `https://www.youtube.com/`, **User-Agent Chrome réaliste** (ex.
  `Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36`),
  taille raisonnable, titre « Connexion YouTube ». L'utilisateur clique « Connexion » et
  s'authentifie. À la **fermeture** de `yt-login` → appeler la synchro (ci-dessous).
- `youtube_sync_cookies(app) -> bool` : lit `cookies_for_url` de la fenêtre `main` pour
  `https://www.youtube.com`, `https://youtube.com`, `https://accounts.google.com`,
  `https://www.google.com` ; convertit en `NetscapeCookie` ; écrit
  `app_data_dir/youtube-cookies.txt` via `cookies::to_netscape`. Renvoie `connected` =
  présence d'un cookie de session (`__Secure-3PSID` **ou** `SAPISID` **ou** `SID`).
- `youtube_status(app) -> bool` : `true` si `youtube-cookies.txt` existe et contient un cookie
  de session (relire le fichier ou re-synchroniser — au plan : statut basé sur l'existence du
  fichier + un cookie de session lu du webview).
- `youtube_logout(app)` : supprime `youtube-cookies.txt` (best-effort : effacer aussi les
  cookies du webview si une API de suppression existe ; sinon le fichier suffit côté yt-dlp).
- `get_cookies_from_browser() -> Option<String>` / `set_cookies_from_browser(value: Option<String>)`
  persiste le repli dans la table `settings` (`cookies_from_browser`).
- Le chemin `youtube-cookies.txt` (sous `app_data_dir`) est exposé à `ytdlp.rs`.

### 3. Intégration yt-dlp — `src-tauri/src/ytdlp.rs`
- Avant chaque run yt-dlp (probe `-J` ET download) : **re-synchroniser** les cookies si
  `cookies_apply_to(url)` (lecture webview `main`, écrit le fichier) — cookies frais.
- Construire les args cookies via `choose_cookie_arg(url, fichier_existe, repli_navigateur)` :
  - `File(path)` → `--cookies <path>`
  - `Browser(b)` → `--cookies-from-browser <b>`
  - `None` → rien (le comportement existant `--add-header Cookie:` de l'extension reste pour
    les autres sites).
- **Quand `--cookies`/`--cookies-from-browser` est utilisé pour une URL YouTube, NE PAS aussi
  ajouter `--add-header Cookie:`** (éviter conflit). `header_args` devient conscient de ce cas
  (ou le runner filtre).

### 4. Frontend
- `api.ts` : `youtubeLogin()`, `youtubeStatus(): Promise<boolean>`, `youtubeLogout()`,
  `getCookiesFromBrowser(): Promise<string|null>`, `setCookiesFromBrowser(v: string|null)`.
- `SettingsDialog.tsx` : nouvelle section **« 🎬→ Compte YouTube »** (icône Lucide, ex. `Youtube`
  ou `UserCircle`) :
  - Statut : « Connecté » (point vert) / « Non connecté » (gris), chargé via `youtubeStatus()`.
  - Bouton **« Se connecter à YouTube »** → `youtubeLogin()` ; au retour (fermeture fenêtre) →
    rafraîchir le statut.
  - Bouton **« Se déconnecter »** → `youtubeLogout()` → statut.
  - Menu **« Repli : cookies du navigateur »** : Aucun / Chrome / Edge / Firefox →
    `setCookiesFromBrowser`.

## Flux « coller un lien »
Connexion une fois (fenêtre) → `youtube-cookies.txt` écrit. Coller un lien YouTube → routage
yt-dlp existant → `ytdlp.rs` re-synchronise + ajoute `--cookies youtube-cookies.txt` → le
téléchargement part avec le compte. Plus besoin de l'extension pour YouTube.

## Limites & sécurité (assumées)
- **Blocage Google en webview** possible (« navigateur non sécurisé ») : mitigé par l'UA Chrome ;
  sinon → repli `--cookies-from-browser`.
- `youtube-cookies.txt` contient la **session Google** : stocké dans le dossier app, **effacé à
  la déconnexion**, n'est passé qu'à yt-dlp pour les URL YouTube/Google.
- YouTube évolue (jetons PO) : la connexion améliore fortement la fiabilité sans garantie absolue.
- Magasin de cookies partagé entre fenêtres (WebView2) : **à confirmer à l'implémentation** ; si
  non partagé, lire les cookies depuis la fenêtre `yt-login` avant sa fermeture (au lieu de `main`).

## Tests / vérification
Pur/testé (`download-core`) : `to_netscape`, `cookies_apply_to`, `choose_cookie_arg`. Glue Tauri
(fenêtre, lecture magasin, écriture fichier, commandes) = non testée unitairement (besoin
webview/AppHandle) → compilation + **smoke manuel** : se connecter, coller un lien YouTube,
vérifier le téléchargement + l'existence de `youtube-cookies.txt` + déconnexion.

## Hors périmètre (YAGNI)
- Afficher l'e-mail/nom du compte connecté.
- Gérer d'autres comptes que YouTube/Google dans cette fenêtre.
- Rafraîchissement/rotation programmée hors « avant chaque téléchargement ».
- Chiffrement du `cookies.txt` au repos.

## Séquencement
Branche `phase-youtube-login` depuis `main`. Tasks : cœur (`cookies.rs` + tests) → glue
`youtube.rs` + commandes → intégration `ytdlp.rs` → frontend (api + Réglages) → vérif + smoke.
