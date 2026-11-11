import { hostOf, isIgnored } from "./site-rules.js";

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
let activeHost = null; // hôte de l'onglet actif (null si l'onglet n'est pas en http/https)

const TEXT_OFFLINE = "FlashGet n'est pas lancé. Ouvrez l'application, puis réessayez.";
const TEXT_REJECTED = "Le jeton ne correspond plus : collez le jeton actuel dans les options.";

function render(connected, state) {
  const rejected = !connected && state === "rejected";
  dot.className = "dot " + (connected ? "on" : rejected ? "err" : "off");
  statusText.textContent = connected ? "Connecté" : "Non connecté";
  disconnectedText.textContent = rejected ? TEXT_REJECTED : TEXT_OFFLINE;
  connectedView.classList.toggle("hidden", !connected);
  disconnectedView.classList.toggle("hidden", connected);
}

function renderSite(sites) {
  siteRow.classList.toggle("hidden", !activeHost);
  if (!activeHost) return;
  const ignored = isIgnored("https://" + activeHost + "/", sites);
  siteSwitch.setAttribute("aria-checked", String(!ignored));
  siteBan.classList.toggle("hidden", !ignored);
  siteText.textContent = "";
  const host = document.createElement("span");
  host.className = "mono";
  host.textContent = activeHost;
  siteText.append(ignored ? "" : "sur ", host, ignored ? " est ignoré" : "");
}

async function refresh() {
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    activeHost = tab && tab.url ? hostOf(tab.url) : null;
  } catch (_e) {
    activeHost = null;
  }
  chrome.runtime.sendMessage({ cmd: "status" }, (res) => {
    render(Boolean(res && res.connected), res && res.state);
    renderSite((res && res.ignoredSites) || []);
  });
}

siteSwitch.addEventListener("click", () => {
  if (!activeHost) return;
  const ignored = siteSwitch.getAttribute("aria-checked") === "true"; // on -> off = ignorer
  chrome.runtime.sendMessage({ cmd: "setSiteIgnored", host: activeHost, ignored }, (res) => {
    if (!(res && res.ok)) flash("Échec — non connecté");
    refresh();
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
    flash(res && res.ok ? "Envoyé ✓" : "Échec — non connecté");
  });
});

document.getElementById("downloadVideo").addEventListener("click", async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab || !tab.id || !tab.url) return;
  chrome.runtime.sendMessage({ cmd: "smartDownload", tabId: tab.id }, (res) => {
    flash(res && res.ok ? "Envoyé ✓" : "Échec — non connecté");
  });
});

document.getElementById("openOptions").addEventListener("click", () => {
  chrome.runtime.openOptionsPage();
});

document.getElementById("retry").addEventListener("click", () => {
  chrome.runtime.sendMessage({ cmd: "reconnect" }, () => setTimeout(refresh, 500));
});

refresh();

