import { DEFAULT_CAPTURE_TYPES } from "./capture-rules.js";

const tokenInput = document.getElementById("token");
const portInput = document.getElementById("port");
const autoCaptureInput = document.getElementById("autoCapture");
const captureTypesInput = document.getElementById("captureTypes");
const saved = document.getElementById("saved");

async function load() {
  const {
    token = "",
    port = 15151,
    autoCapture = true,
    captureTypes = DEFAULT_CAPTURE_TYPES,
  } = await chrome.storage.local.get(["token", "port", "autoCapture", "captureTypes"]);
  tokenInput.value = token;
  portInput.value = port;
  autoCaptureInput.checked = autoCapture !== false;
  captureTypesInput.value = captureTypes;
}

document.getElementById("save").addEventListener("click", async () => {
  await chrome.storage.local.set({
    token: tokenInput.value.trim(),
    port: Number(portInput.value) || 15151,
    autoCapture: autoCaptureInput.checked,
    captureTypes: captureTypesInput.value,
  });
  saved.textContent = "Enregistré ✓";
  setTimeout(() => (saved.textContent = ""), 1500);
  // Forcer une reconnexion avec le nouveau jeton/port.
  chrome.runtime.sendMessage({ cmd: "reconnect" });
});

load();
