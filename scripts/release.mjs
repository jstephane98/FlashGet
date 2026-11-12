// Publication locale : compile, signe, puis prépare release/v{version}/.
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { buildManifest, checkPubkey, platformTarget } from "./manifest.mjs";

const REPO = "jstephane98/FlashGet";
const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const notesIdx = args.indexOf("--notes");
const notesFile = notesIdx >= 0 ? args[notesIdx + 1] : null;

function fail(msg) {
  console.error(msg);
  process.exit(1);
}

if (notesIdx >= 0 && !notesFile) fail("L'option --notes attend un fichier.");
// Vérifié avant la compilation (plusieurs minutes) : une faute de nom doit échouer tout de suite.
if (notesFile && !existsSync(resolve(notesFile))) {
  fail(`Fichier de notes introuvable : ${resolve(notesFile)}. Créez-le, ou relancez sans --notes.`);
}

const conf = JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8"));
const pubkeyCheck = checkPubkey(conf);
if (!pubkeyCheck.ok) fail(pubkeyCheck.message);

if (!process.env.TAURI_SIGNING_PRIVATE_KEY && !process.env.TAURI_SIGNING_PRIVATE_KEY_PATH) {
  fail(
    "Clé de signature absente : définissez TAURI_SIGNING_PRIVATE_KEY_PATH " +
      "(ou TAURI_SIGNING_PRIVATE_KEY) avant de publier. Voir docs/RELEASE.md.",
  );
}

const version = conf.version;
const target = platformTarget(process.platform, version);
if (!target) fail(`Système non pris en charge pour la publication : ${process.platform}.`);
const fileName = target.updater;
const bundleDir = join(root, "src-tauri", "target", "release", "bundle", target.bundleDir);
const exePath = join(bundleDir, fileName);
const sigPath = `${exePath}.sig`;
const outDir = join(root, "release", `v${version}`);
const manifestPath = join(outDir, "latest.json");

if (dryRun) {
  console.log(`[dry-run] version : ${version} (${target.key})`);
  console.log(`[dry-run] installeur attendu : ${exePath}`);
  console.log(`[dry-run] signature attendue : ${sigPath}`);
  console.log(`[dry-run] dossier de sortie : ${outDir}`);
  process.exit(0);
}

const notes = notesFile ? readFileSync(resolve(notesFile), "utf8") : "";

const build = spawnSync("npx tauri build", {
  cwd: root,
  stdio: "inherit",
  shell: true,
  env: {
    ...process.env,
    // `tauri build` lit seulement TAURI_SIGNING_PRIVATE_KEY (contenu de la clé ou chemin).
    TAURI_SIGNING_PRIVATE_KEY: process.env.TAURI_SIGNING_PRIVATE_KEY ?? process.env.TAURI_SIGNING_PRIVATE_KEY_PATH,
  },
});
if (build.status !== 0) fail("La compilation a échoué.");

if (!existsSync(exePath)) fail(`Installeur introuvable : ${exePath}`);
if (!existsSync(sigPath)) fail(`Signature introuvable : ${sigPath}`);

mkdirSync(outDir, { recursive: true });
const copied = [];
for (const f of target.files) {
  const src = join(bundleDir, f);
  if (!existsSync(src)) fail(`Fichier introuvable : ${src}`);
  const name = f.split("/").pop();
  copyFileSync(src, join(outDir, name));
  copied.push(name);
}
copyFileSync(sigPath, join(outDir, `${fileName}.sig`));
// Un latest.json déjà présent (l'autre système, même version) est complété, pas écrasé.
const existing = existsSync(manifestPath) ? JSON.parse(readFileSync(manifestPath, "utf8")) : null;
const manifest = buildManifest({
  version,
  notes,
  pubDate: new Date(),
  signature: readFileSync(sigPath, "utf8"),
  fileName,
  repo: REPO,
  platform: target.key,
  existing,
});
writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n");

console.log(`\nPrêt dans ${outDir} (${Object.keys(manifest.platforms).join(", ")})`);
console.log(`Étapes suivantes : créez (ou complétez) la Release GitHub v${version} sur ${REPO}`);
console.log(`et déposez-y : ${[...copied, `${fileName}.sig`, "latest.json"].join(", ")}.`);
