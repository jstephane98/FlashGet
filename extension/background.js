import { buildCapture } from "./buildCapture.js";
import { shouldCapture, parseCaptureTypes, DEFAULT_CAPTURE_TYPES } from "./capture-rules.js";
import { isHlsUrl, isHlsContentType, isDashUrl, isDashContentType } from "./media-rules.js";
import { browserName } from "./browser-info.js";
import { recordStream, getStreams, clearStreams } from "./stream-store.js";

const PORT = 15151; // port fixe du serveur local de FlashGet (un port enregistré par une ancienne version est ignoré)
const KEEPALIVE_ALARM = "flashget-keepalive";
const PENDING_TTL_MS = 60000; // une capture en file de plus de 60 s est abandonnée
let socket = null;
let connected = false;
let connectionState = "offline"; // "connected" | "rejected" (jeton refusé) | "offline"
let ignoredSites = []; // cache de la liste reçue de FlashGet (aussi dans chrome.storage.local)
let reconnectTimer = null;
let connecting = false;
let pending = []; // captures en attente d'une connexion authentifiée : [{ url, referrer, filename, ts }]

// Flux HLS/DASH détectés par onglet : persistés dans chrome.storage.session (via
// stream-store.js) pour survivre à l'éviction du service worker MV3 (~30 s d'inactivité).
chrome.webRequest.onBeforeRequest.addListener(
  (details) => {
    if (isHlsUrl(details.url) || isDashUrl(details.url)) {
      recordStream(chrome.storage.session, details.tabId, details.url, Date.now());
    }
  },
  { urls: ["http://*/*", "https://*/*"] },
);

chrome.webRequest.onHeadersReceived.addListener(
  (details) => {
    const ct = (details.responseHeaders || []).find(
      (h) => h.name.toLowerCase() === "content-type",
    );
    if (ct && (isHlsContentType(ct.value) || isDashContentType(ct.value))) {
      recordStream(chrome.storage.session, details.tabId, details.url, Date.now());
    }
  },
  { urls: ["http://*/*", "https://*/*"] },
  ["responseHeaders"],
);

// Purge l'entrée d'un onglet à la navigation (nouvelle URL) et à sa fermeture.
chrome.tabs.onUpdated.addListener((tabId, changeInfo) => {
  if (changeInfo.url) clearStreams(chrome.storage.session, tabId);
});
chrome.tabs.onRemoved.addListener((tabId) => clearStreams(chrome.storage.session, tabId));

// Renvoie les captures mises en file une fois la connexion authentifiée (ignore les trop vieilles).
async function flushPending() {
  if (!socket || socket.readyState !== WebSocket.OPEN || !connected) return;
  const now = Date.now();
  const items = pending;
  pending = [];
  for (const it of items) {
    if (now - it.ts > PENDING_TTL_MS) continue;
    try {
      const headers = await gatherHeaders(it.url, it.referrer);
      socket.send(JSON.stringify(buildCapture(it.url, { ...headers, filename: it.filename, kind: it.kind, fallback_url: it.fallback_url, fallback_kind: it.fallback_kind })));
    } catch (_e) {
      // URL invalide ou erreur de collecte : on ignore cette capture
    }
  }
}

// Collecte les en-têtes navigateur pour une URL capturée : cookies (incl. httpOnly via
// l'API chrome.cookies), referer (page d'origine), user-agent.
async function gatherHeaders(url, referrer) {
  let cookie = "";
  try {
    const cookies = await chrome.cookies.getAll({ url });
    // Retirer les caractères de contrôle (ex. \r\n) des valeurs pour éviter toute
    // corruption / injection d'en-tête Cookie ; les cookies normaux ne sont pas affectés.
    const clean = (s) => String(s).replace(/[\x00-\x1f]/g, "");
    cookie = cookies.map((c) => `${clean(c.name)}=${clean(c.value)}`).join("; ");
  } catch (_e) {
    // pas d'accès cookies pour cette URL : on envoie sans
  }
  return {
    referrer: referrer || undefined,
    cookie: cookie || undefined,
    // Toujours présent (navigator.userAgent est non vide) — voulu : on veut l'UA du navigateur.
    user_agent: navigator.userAgent,
  };
}

// Dernier segment de chemin d'un nom de fichier suggéré par le navigateur.
function basename(name) {
  return String(name || "").split(/[/\\]/).pop() || "";
}

async function getConfig() {
  const {
    token = "",
    autoCapture = true,
    captureTypes = DEFAULT_CAPTURE_TYPES,
    ignoredSites: cached = [],
  } = await chrome.storage.local.get(["token", "autoCapture", "captureTypes", "ignoredSites"]);
  return {
    token,
    autoCapture: autoCapture !== false,
    types: parseCaptureTypes(captureTypes),
    ignoredSites: Array.isArray(cached) ? cached : [],
  };
}

// Enregistre la liste des sites ignorés reçue de FlashGet (cache pour la capture hors connexion).
function applyIgnoredSites(list, persist = true) {
  ignoredSites = Array.isArray(list) ? list.filter((x) => typeof x === "string") : [];
  if (persist) chrome.storage.local.set({ ignoredSites }).catch(() => {});
}

function setConnected(value, state) {
  connected = value;
  connectionState = value ? "connected" : state || "offline";
  // Pastille verte/grise sur l'icône.
  chrome.action.setBadgeText({ text: value ? "●" : "" });
  chrome.action.setBadgeBackgroundColor({ color: value ? "#22c55e" : "#9ca3af" });
  if (value) flushPending();
}

async function connect() {
  if (connecting) return;
  if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
    return;
  }
  connecting = true;
  let token;
  try {
    const cfg = await getConfig();
    token = cfg.token;
    ignoredSites = cfg.ignoredSites;
  } catch (_e) {
    connecting = false;
    scheduleReconnect();
    return;
  }
  // Re-vérifier après l'await : un autre appel a pu ouvrir la socket entre-temps.
  if (socket && (socket.readyState === WebSocket.OPEN || socket.readyState === WebSocket.CONNECTING)) {
    connecting = false;
    return;
  }
  try {
    socket = new WebSocket(`ws://127.0.0.1:${PORT}`);
  } catch (_e) {
    connecting = false;
    scheduleReconnect();
    return;
  }
  connecting = false;
  const sock = socket;
  let opened = false;
  socket.onopen = () => {
    opened = true;
    connectionState = "offline"; // jusqu'à `authed` ou `error` : pas d'ancien « rejected »
    socket.send(
      JSON.stringify({
        type: "auth",
        token,
        browser: browserName(navigator.userAgentData, navigator.userAgent, Boolean(navigator.brave)),
        version: chrome.runtime.getManifest().version,
      }),
    );
  };
  socket.onmessage = (event) => {
    if (sock !== socket) return;
    let msg;
    try {
      msg = JSON.parse(event.data);
    } catch (_e) {
      return;
    }
    if (msg.type === "authed") {
      setConnected(true);
    } else if (msg.type === "config") {
      applyIgnoredSites(msg.ignored_sites);
    } else if (msg.type === "error" && (msg.error === "auth_failed" || msg.error === "need_auth")) {
      setConnected(false, "rejected");
    }
  };
  socket.onclose = () => {
    if (sock !== socket) return; // ancienne socket remplacée par une reconnexion
    // Un refus de jeton reste affiché tant que la connexion n'a pas abouti ; si le serveur
    // n'a jamais répondu (jamais ouvert), c'est que FlashGet n'est pas lancé.
    setConnected(false, opened && connectionState === "rejected" ? "rejected" : "offline");
    scheduleReconnect();
  };
  socket.onerror = () => {
    if (sock === socket && !opened) setConnected(false, "offline");
  };
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, 3000);
}

// Envoie une capture. Si la connexion n'est pas prête (service worker réveillé à froid,
// reconnexion en cours), met l'URL en file et (re)connecte : elle partira après l'auth.
// Renvoie true si la capture est partie OU a été acceptée en file.
async function sendCapture(url, referrer, filename) {
  if (socket && socket.readyState === WebSocket.OPEN && connected) {
    try {
      const headers = await gatherHeaders(url, referrer);
      socket.send(JSON.stringify(buildCapture(url, { ...headers, filename })));
      return true;
    } catch (_e) {
      return false; // URL invalide
    }
  }
  pending.push({ url, referrer, filename, ts: Date.now() });
  if (pending.length > 20) pending.shift(); // borne anti-accumulation
  connect();
  return true; // acceptée : sera envoyée dès la (re)connexion
}

// Bouton vidéo intelligent : envoie l'URL de page (yt-dlp d'abord côté app) + un flux
// HLS/DASH sniffé en repli (kind "smart").
async function sendSmart(url, referrer, fallbackUrl, title) {
  const extras = { kind: "smart" };
  if (title) extras.filename = title;
  if (fallbackUrl) {
    extras.fallback_url = fallbackUrl;
    extras.fallback_kind = isDashUrl(fallbackUrl) ? "dash" : "hls";
  }
  if (socket && socket.readyState === WebSocket.OPEN && connected) {
    try {
      const headers = await gatherHeaders(url, referrer);
      socket.send(JSON.stringify(buildCapture(url, { ...headers, ...extras })));
      return true;
    } catch (_e) {
      return false;
    }
  }
  pending.push({ url, referrer, ...extras, ts: Date.now() });
  if (pending.length > 20) pending.shift();
  connect();
  return true;
}

// Keepalive : réveille le service worker périodiquement (le min MV3 est 30 s) pour
// maintenir/relancer la connexion, sinon le SW est tué après ~30 s d'inactivité.
function ensureKeepalive() {
  chrome.alarms.create(KEEPALIVE_ALARM, { periodInMinutes: 0.5 });
}

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name !== KEEPALIVE_ALARM) return;
  connect();
  // Un message toutes les 30 s : FlashGet met à jour « dernière activité » à chaque réception.
  if (socket && socket.readyState === WebSocket.OPEN && connected) {
    try {
      socket.send(JSON.stringify({ type: "ping" }));
    } catch (_e) {
      // socket en cours de fermeture : la reconnexion s'en charge
    }
  }
});

// Menu contextuel sur liens/images/vidéos/audio.
chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "flashget-capture",
    title: "Télécharger avec FlashGet",
    contexts: ["link", "image", "video", "audio"],
  });
  ensureKeepalive();
});

// Au démarrage du navigateur : reprogrammer le keepalive et se reconnecter.
chrome.runtime.onStartup.addListener(() => {
  ensureKeepalive();
  connect();
});

chrome.contextMenus.onClicked.addListener((info) => {
  const url = info.linkUrl || info.srcUrl || info.pageUrl;
  if (url) sendCapture(url, info.pageUrl);
});

// Capture automatique : intercepte les téléchargements du navigateur (types listés) et
// les redirige vers FlashGet. N'agit QUE si l'app est connectée (sinon on laisse le
// navigateur télécharger pour ne rien perdre).
chrome.downloads.onDeterminingFilename.addListener((item) => {
  (async () => {
    if (!connected) return; // app injoignable : ne pas intercepter
    const cfg = await getConfig();
    const url = item.finalUrl || item.url;
    if (!shouldCapture({ url, requestUrl: item.url, filename: item.filename, referrer: item.referrer }, { autoCapture: cfg.autoCapture, types: cfg.types }, ignoredSites)) {
      return; // type non listé / site ignoré / schéma non récupérable : laisser le navigateur
    }
    try {
      await chrome.downloads.cancel(item.id);
    } catch (_e) {
      return; // annulation impossible : laisser le navigateur, ne pas double-télécharger
    }
    // L'annulation a réussi : le download navigateur est arrêté, on prend le relais même si
    // le nettoyage de l'historique échoue (best-effort, ne doit pas faire perdre la capture).
    try {
      await chrome.downloads.erase({ id: item.id });
    } catch (_e) {
      // nettoyage best-effort
    }
    sendCapture(url, item.referrer, basename(item.filename)).catch(() => {});
  })();
  // Listener synchrone : on n'appelle pas suggest() (le navigateur garde son nom par défaut
  // pour les downloads non capturés ; les capturés sont annulés ci-dessus).
});

// Messages depuis le popup.
chrome.runtime.onMessage.addListener((req, _sender, sendResponse) => {
  if (req.cmd === "status") {
    sendResponse({ connected, state: connectionState, ignoredSites });
    return false;
  }
  if (req.cmd === "send" && req.url) {
    (async () => {
      let referrer;
      try {
        const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
        referrer = tab && tab.url;
      } catch (_e) {
        // pas d'onglet : referer absent
      }
      const ok = await sendCapture(req.url, referrer);
      sendResponse({ ok });
    })();
    return true; // réponse asynchrone
  }
  if (req.cmd === "reconnect") {
    // Nouveau jeton : abandonner une socket non authentifiée pour repartir avec le jeton enregistré.
    if (socket && !connected) {
      try {
        socket.close();
      } catch (_e) {
        // déjà fermée
      }
      socket = null;
    }
    connect();
    sendResponse({ ok: true });
    return false;
  }
  if (req.cmd === "setSiteIgnored" && typeof req.host === "string") {
    if (socket && socket.readyState === WebSocket.OPEN && connected) {
      socket.send(JSON.stringify({ type: "set_site_ignored", host: req.host, ignored: Boolean(req.ignored) }));
      // Mise à jour optimiste ; FlashGet renvoie ensuite la liste de référence (`config`).
      const host = req.host;
      const rest = ignoredSites.filter((h) => h !== host);
      applyIgnoredSites(req.ignored ? [...rest, host] : rest, false); // mémoire seulement
      sendResponse({ ok: true });
    } else {
      sendResponse({ ok: false });
    }
    return false;
  }
  if (req.cmd === "smartDownload" && req.tabId != null) {
    (async () => {
      let url, referrer, title;
      try {
        const tab = await chrome.tabs.get(req.tabId);
        url = tab && tab.url;
        referrer = url;
        title = tab && tab.title;
      } catch (_e) {
        // pas d'onglet
      }
      if (!url) { sendResponse({ ok: false }); return; }
      const streams = await getStreams(chrome.storage.session, req.tabId);
      const fallbackUrl = streams.length > 0 ? streams[0].url : undefined;
      const ok = await sendSmart(url, referrer, fallbackUrl, title);
      sendResponse({ ok });
    })();
    return true; // réponse asynchrone
  }
  return false;
});

// Connexion initiale + keepalive au (ré)veil du service worker.
ensureKeepalive();
connect();
