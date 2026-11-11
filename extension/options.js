import { DEFAULT_CAPTURE_TYPES } from "./capture-rules.js";

const tokenInput = document.getElementById("token");
const connDot = document.getElementById("connDot");
const connText = document.getElementById("connText");
const autoCaptureInput = document.getElementById("autoCapture");
const captureTypesInput = document.getElementById("captureTypes");
const saved = document.getElementById("saved");

async function load() {
  const {
    token = "",
    autoCapture = true,
    captureTypes = DEFAULT_CAPTURE_TYPES,
  } = await chrome.storage.local.get(["token", "autoCapture", "captureTypes"]);
  tokenInput.value = token;
  autoCaptureInput.checked = autoCapture !== false;
  captureTypesInput.value = captureTypes;
}

document.getElementById("save").addEventListener("click", async () => {
  await chrome.storage.local.set({
    token: tokenInput.value.trim(),
    autoCapture: autoCaptureInput.checked,
    captureTypes: captureTypesInput.value,
  });
  saved.textContent = "Enregistré ✓";
  setTimeout(() => (saved.textContent = ""), 1500);
  // Forcer une reconnexion avec le nouveau jeton.
  chrome.runtime.sendMessage({ cmd: "reconnect" }, () => setTimeout(refreshStatus, 800));
});

function refreshStatus() {
  chrome.runtime.sendMessage({ cmd: "status" }, (res) => {
    const state = res && res.connected ? "connected" : (res && res.state) || "offline";
    connDot.className = "dot " + (state === "connected" ? "on" : state === "rejected" ? "err" : "off");
    connText.textContent =
      state === "connected"
        ? "Connectée à FlashGet"
        : state === "rejected"
          ? "Jeton refusé : collez le jeton actuel de FlashGet."
          : "FlashGet n'est pas lancé.";
  });
}

load();
refreshStatus();
setInterval(refreshStatus, 3000);
