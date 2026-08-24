import { lstat, readFile } from "node:fs/promises";
import { resolve } from "node:path";

interface MatrixTarget {
  platform: "linux" | "macos" | "windows";
  architecture: "aarch64" | "x86_64";
  rustTarget: string;
  runner: string;
  minimumVersion: string;
  bundles: string[];
  updaterKey: string;
  lifecycleTier: "experimental" | "supported";
}

interface PlatformMatrix {
  schemaVersion: number;
  channel: string;
  targets: MatrixTarget[];
}

async function readBounded(path: string, maximumBytes = 1024 * 1024): Promise<string> {
  const absolute = resolve(path);
  const metadata = await lstat(absolute);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size === 0 || metadata.size > maximumBytes) {
    throw new Error(`${path} must be a bounded regular file`);
  }
  return readFile(absolute, "utf8");
}

async function readJson<T>(path: string): Promise<T> {
  return JSON.parse(await readBounded(path)) as T;
}

function requireText(content: string, values: string[], owner: string): void {
  for (const value of values) {
    if (!content.includes(value)) throw new Error(`${owner} is missing ${value}`);
  }
}

async function main(): Promise<void> {
  const matrix = await readJson<PlatformMatrix>("release/platform-matrix.json");
  if (matrix.schemaVersion !== 1 || matrix.channel !== "stable" || !Array.isArray(matrix.targets)) {
    throw new Error("release matrix schema or channel is invalid");
  }
  const identities = new Set<string>();
  const stable = matrix.targets.filter((target) => target.lifecycleTier === "supported");
  if (stable.length !== 4) throw new Error("stable release matrix must contain four owned targets");
  for (const target of matrix.targets) {
    const identity = `${target.platform}:${target.architecture}`;
    if (identities.has(identity)) throw new Error(`duplicate release target ${identity}`);
    identities.add(identity);
    if (
      !target.rustTarget
      || !target.runner
      || !target.minimumVersion
      || !target.updaterKey
      || target.bundles.length === 0
    ) {
      throw new Error(`release target ${identity} is incomplete`);
    }
  }
  for (const required of [
    "macos:aarch64",
    "macos:x86_64",
    "windows:x86_64",
    "linux:x86_64",
  ]) {
    if (!stable.some((target) => `${target.platform}:${target.architecture}` === required)) {
      throw new Error(`stable release matrix is missing ${required}`);
    }
  }

  const releaseConfig = await readJson<Record<string, unknown>>("src-tauri/tauri.release.conf.json");
  const serializedConfig = JSON.stringify(releaseConfig);
  requireText(serializedConfig, ["createUpdaterArtifacts", "https://github.com/itsddvn/stm/releases/latest/download/latest.json"], "release config");
  if (serializedConfig.includes("pubkey") || /PRIVATE|SECRET/.test(serializedConfig)) {
    throw new Error("release template must not contain injected keys or secret placeholders");
  }

  const workspaceCargo = await readBounded("Cargo.toml");
  const desktopCargo = await readBounded("src-tauri/Cargo.toml");
  const desktopLib = await readBounded("src-tauri/src/lib.rs");
  const productUpdate = await readBounded("src-tauri/src/product_update.rs");
  const signedMetadata = await readBounded("src-tauri/src/signed_update_metadata.rs");
  requireText(workspaceCargo, ["tauri-plugin-updater", "crates/release-verifier"], "workspace Cargo manifest");
  requireText(desktopCargo, ["tauri-plugin-updater.workspace = true"], "desktop Cargo manifest");
  requireText(desktopLib, ["tauri_plugin_updater::Builder", "ProductUpdateRuntime", "reconcile_startup"], "desktop host");
  requireText(productUpdate, ["UpdaterExt", "SignedProductUpdate", "UserConfirmation", "download_and_install", "persist_pending_install", "active_operation"], "product updater boundary");
  requireText(signedMetadata, ["latest.json.sig", "verify_release_metadata", "downgrade", "same-version drift"], "signed updater metadata");

  const releaseWorkflow = await readBounded(".github/workflows/release.yml");
  const securityWorkflow = await readBounded(".github/workflows/security.yml");
  for (const target of stable) {
    requireText(releaseWorkflow, [target.rustTarget, target.runner], "release workflow");
  }
  requireText(releaseWorkflow, [
    "environment: signed-release",
    "releaseDraft: true",
    "TAURI_SIGNING_PRIVATE_KEY",
    "TAURI_UPDATER_PUBLIC_KEY",
    "verify:release-artifacts",
    "attest-build-provenance",
    "release-quality-security",
    "release-codeql",
    "stm-release-verifier",
    "latest.json.sig",
    "git rev-parse",
  ], "release workflow");
  for (const [workflowName, workflow] of [["release", releaseWorkflow], ["security", securityWorkflow]] as const) {
    const actions = [...workflow.matchAll(/uses:\s+[^@\s]+@([^\s#]+)/g)];
    if (actions.length === 0 || actions.some((match) => !/^[a-f0-9]{40}$/.test(match[1]))) {
      throw new Error(`${workflowName} workflow contains an unpinned action`);
    }
  }
  requireText(securityWorkflow, [
    "dependency-review-action",
    "cargo audit",
    "pnpm audit",
    "verify:no-secrets",
    "sbom-action",
    "codeql-action",
  ], "security workflow");

  const packageJson = await readBounded("package.json");
  requireText(packageJson, [
    "verify:release-contract",
    "verify:release-artifacts",
    "verify:no-secrets",
    "verify:release-tooling",
    "release:config",
    "verify:release-version",
  ], "package scripts");
  for (const documentation of [
    "README.md",
    "docs/deployment-guide.md",
    "docs/security-model.md",
    "docs/supported-platforms.md",
  ]) {
    const content = await readBounded(documentation);
    requireText(content, ["STM"], documentation);
  }
  const deployment = await readBounded("docs/deployment-guide.md");
  requireText(deployment, ["draft", "latest.json", "Rollback", "TAURI_SIGNING_PRIVATE_KEY"], "deployment guide");
  const security = await readBounded("docs/security-model.md");
  requireText(security, ["separate", "credential", "XChaCha20-Poly1305", "signing-key compromise"], "security model");

  process.stdout.write(`Release contract verification passed (${stable.length} stable targets, ${matrix.targets.length - stable.length} experimental).\n`);
}

main().catch((error: unknown) => {
  process.stderr.write(`Release contract verification failed: ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
