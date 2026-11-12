import { hostOf, matchingSite } from "./site-rules.js";
import { isStaleBackground } from "./popup-state.js";
import { t, appendWithNode } from "./i18n.js";

const dot = document.getElementById("dot");
const statusText = document.getElementById("statusText");
const connectedView = document.getElementById("connectedView");
const disconnectedView = document.getElementById("disconnectedView");
const toast = document.getElementById("toast");

const disconnectedText = document.getElementById("disconnectedText");
const siteRow = document.getElementById("siteRow");
const siteBan = document.getElementById("siteBan");
const siteText = document.getElementById("siteText");
const siteSwitch = document.getElementById("siteSwitch");
const staleNotice = document.getElementById("staleNotice");
let matched = null; // entrée de la liste qui ignore l'onglet actif (peut être un site parent)
let activeHost = null; // hôte de l'onglet actif (null si l'onglet n'est pas en http/https)

function render(connected, state) {
  const rejected = !connected && state === "rejected";
  dot.className = "dot " + (connected ? "on" : "err");
  statusText.textContent = t(connected ? "popupConnected" : "popupNotConnected");
  disconnectedText.textContent = t(rejected ? "popupTokenRejected" : "popupOffline");
  connectedView.classList.toggle("hidden", !connected);
  disconnectedView.classList.toggle("hidden", connected);
}

function renderSite(sites) {
  siteRow.classList.toggle("hidden", !activeHost);
  if (!activeHost) return;
  matched = matchingSite("https://" + activeHost + "/", sites);
  const ignored = matched !== null;
  siteSwitch.setAttribute("aria-checked", String(!ignored));
  siteBan.classList.toggle("hidden", !ignored);
  siteText.textContent = "";
  const host = document.createElement("span");
  host.className = "mono";
  host.textContent = ignored ? matched : activeHost;
  // Le message garde des espaces insécables autour de l'hôte (un espace en bord de nœud
  // texte peut disparaître au rendu) ; le jeton y est remplacé par l'élément de l'hôte.
  const token = "{host}"; // « { » ne peut pas figurer dans un hôte normalisé
  appendWithNode(siteText, t(ignored ? "popupSiteIgnored" : "popupSiteActive", [token]), token, host);
}

async function refresh(retry = true) {
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    activeHost = tab && tab.url ? hostOf(tab.url) : null;
  } catch (_e) {
    activeHost = null;
  }
  chrome.runtime.sendMessage({ cmd: "status" }, (res) => {
    const ok = Boolean(res && res.connected);
    render(ok, res && res.state);
    // Ancien service d'arrière-plan encore en mémoire (extension non rechargée après une mise
    // à jour) : il ne connaît pas la liste, la ligne « Capture automatique » serait fausse.
    const stale = isStaleBackground(res, chrome.runtime.getManifest().version);
    staleNotice.classList.toggle("hidden", !stale);
    if (stale) siteRow.classList.add("hidden");
    else renderSite((res && res.ignoredSites) || []);
    // Réveil à froid du service worker : la connexion n'est pas encore établie.
    if (!ok && retry) setTimeout(() => refresh(false), 800);
  });
}

siteSwitch.addEventListener("click", () => {
  if (!activeHost) return;
  // Interrupteur actif -> le couper ignore l'hôte ; sinon le rallumer retire l'entrée qui ignore.
  const ignore = matched === null;
  const host = ignore ? activeHost : matched;
  chrome.runtime.sendMessage({ cmd: "setSiteIgnored", host, ignored: ignore }, (res) => {
    if (!(res && res.ok)) flash(t("toastFailed"));
    refresh(false);
  });
});

function flash(text) {
  toast.textContent = text;
  setTimeout(() => (toast.textContent = ""), 1500);
}

document.getElementById("sendUrl").addEventListener("click", () => {
  const url = document.getElementById("url").value;
  if (!url.trim()) return;
  chrome.runtime.sendMessage({ cmd: "send", url }, (res) => {
    flash(t(res && res.ok ? "toastSent" : "toastFailed"));
  });
});

document.getElementById("downloadVideo").addEventListener("click", async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab || !tab.id || !tab.url) return;
  chrome.runtime.sendMessage({ cmd: "smartDownload", tabId: tab.id }, (res) => {
    flash(t(res && res.ok ? "toastSent" : "toastFailed"));
  });
});

document.getElementById("openOptions").addEventListener("click", () => {
  chrome.runtime.openOptionsPage();
});

document.getElementById("retry").addEventListener("click", () => {
  chrome.runtime.sendMessage({ cmd: "reconnect" }, () => setTimeout(refresh, 500));
});

refresh();

