// Publication locale : compile, signe, puis prépare release/v{version}/.
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { buildManifest, checkPubkey } from "./manifest.mjs";

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
const fileName = `FlashGet_${version}_x64-setup.exe`;
const bundleDir = join(root, "src-tauri", "target", "release", "bundle", "nsis");
const exePath = join(bundleDir, fileName);
const sigPath = `${exePath}.sig`;
const outDir = join(root, "release", `v${version}`);

if (dryRun) {
  console.log(`[dry-run] version : ${version}`);
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
copyFileSync(exePath, join(outDir, fileName));
copyFileSync(sigPath, join(outDir, `${fileName}.sig`));
const manifest = buildManifest({
  version,
  notes,
  pubDate: new Date(),
  signature: readFileSync(sigPath, "utf8"),
  fileName,
  repo: REPO,
});
writeFileSync(join(outDir, "latest.json"), JSON.stringify(manifest, null, 2) + "\n");

console.log(`\nPrêt dans ${outDir}`);
console.log(`Étapes suivantes : créez la Release GitHub v${version} sur ${REPO}`);
console.log(`et déposez-y les 3 fichiers : ${fileName}, ${fileName}.sig, latest.json.`);
