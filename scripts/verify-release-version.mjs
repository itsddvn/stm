import { readFile } from "node:fs/promises";

const tag = process.argv[2] ?? "";
if (!/^v(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$/.test(tag)) {
  throw new Error("release tag must be v-prefixed SemVer");
}
const packageJson = JSON.parse(await readFile("package.json", "utf8"));
const tauriConfig = JSON.parse(await readFile("src-tauri/tauri.conf.json", "utf8"));
const cargo = await readFile("Cargo.toml", "utf8");
const cargoVersion = cargo.match(/^version = "([^"]+)"$/m)?.[1];
const expected = tag.slice(1);
if (packageJson.version !== expected || tauriConfig.version !== expected || cargoVersion !== expected) {
  throw new Error(`release version mismatch: tag=${expected}, package=${packageJson.version}, tauri=${tauriConfig.version}, cargo=${cargoVersion}`);
}
process.stdout.write(`Release version ${expected} is synchronized.\n`);
