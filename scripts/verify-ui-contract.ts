import { createHash } from "node:crypto";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { designTokens } from "../contracts/ui/design-token-contract";

type ManifestStatus = "review" | "approved" | "locked";
interface Manifest {
  contractVersion: string;
  status: ManifestStatus;
  approval: null | { approvedBy: string; approvedAt: string };
  lockFile: string;
  artifacts: string[];
}
interface LockFile {
  contractVersion: string;
  artifacts: Record<string, string>;
}
interface PackageJson {
  dependencies?: Record<string, string>;
}

const root = resolve(import.meta.dirname, "..");
const manifestPath = resolve(root, "contracts/ui/ui-contract.manifest.json");
const manifest = readJson<Manifest>(manifestPath);
const packageJson = readJson<PackageJson>(resolve(root, "package.json"));
const errors: string[] = [];
const shouldWriteLock = process.argv.includes("--write-lock");
const requiredLifecycleBaselines = [
  "assets/designs/tools-manager-ui/baselines/tool-lifecycle-review-1024x720.png",
  "assets/designs/tools-manager-ui/baselines/update-batch-lifecycle-review-1280x800.png",
  "assets/designs/tools-manager-ui/baselines/product-lifecycle-recovery-1440x900.png",
] as const;

if (!manifest.contractVersion) errors.push("manifest contractVersion is required");
if (!["review", "approved", "locked"].includes(manifest.status)) errors.push(`unsupported manifest status: ${manifest.status}`);
if (manifest.artifacts.length === 0) errors.push("manifest must list contract artifacts");
if (new Set(manifest.artifacts).size !== manifest.artifacts.length) errors.push("manifest contains duplicate artifacts");
for (const [packageName, expectedVersion] of Object.entries(designTokens.fontPackages)) {
  const actualVersion = packageJson.dependencies?.[packageName];
  if (actualVersion !== expectedVersion) {
    errors.push(`font package ${packageName} must be pinned to ${expectedVersion}; found ${actualVersion ?? "missing"}`);
  }
}

for (const artifact of manifest.artifacts) {
  if (artifact.includes("..") || artifact.startsWith("/")) errors.push(`artifact path must stay relative: ${artifact}`);
  if (!existsSync(resolve(root, artifact))) errors.push(`missing artifact: ${artifact}`);
  if (artifact.endsWith(".png") && existsSync(resolve(root, artifact))) verifyPngDimensions(artifact);
}

const missingLifecycleBaselines = requiredLifecycleBaselines.filter((artifact) => !manifest.artifacts.includes(artifact));
if (manifest.status !== "review" && missingLifecycleBaselines.length > 0) {
  errors.push(`approved or locked v1.1 requires lifecycle baselines: ${missingLifecycleBaselines.join(", ")}`);
}

const lockPath = resolve(root, "contracts/ui", manifest.lockFile);
if (manifest.status === "review") {
  if (manifest.approval !== null) errors.push("review manifest must not claim approval");
} else {
  if (!manifest.approval?.approvedBy || !manifest.approval.approvedAt) errors.push(`${manifest.status} manifest requires recorded project-lead approval`);
  if (!existsSync(lockPath) && !shouldWriteLock) errors.push(`${manifest.status} manifest requires ${manifest.lockFile}`);
}

if (shouldWriteLock) {
  if (manifest.status !== "locked") {
    errors.push("lock generation requires manifest status locked");
  } else if (errors.length === 0) {
    writeLock();
  }
}

if (manifest.status === "locked" && existsSync(lockPath)) verifyLock(readJson<LockFile>(lockPath));

if (errors.length > 0) {
  console.error("UI contract verification failed:");
  for (const error of errors) console.error(`- ${error}`);
  process.exit(1);
}

console.log(`UI contract structural verification passed (${manifest.status}, ${manifest.artifacts.length} artifacts).`);
if (manifest.status === "review") {
  if (existsSync(lockPath)) {
    const staleLock = readJson<LockFile>(lockPath);
    console.log(`Review status: existing ${staleLock.contractVersion} lock is intentionally stale and was not verified or regenerated.`);
  }
  if (missingLifecycleBaselines.length > 0) {
    console.log(`Review evidence pending: ${missingLifecycleBaselines.join(", ")}.`);
    console.log(`Remaining gate: capture and verify the v1.1 lifecycle viewport matrix, obtain project-lead approval for UI Contract ${manifest.contractVersion}, then regenerate the lock.`);
  } else {
    console.log("Lifecycle viewport matrix is present.");
    console.log(`Remaining gate: obtain project-lead approval for UI Contract ${manifest.contractVersion}, then regenerate the lock.`);
  }
}

function verifyLock(lock: LockFile) {
  if (lock.contractVersion !== manifest.contractVersion) errors.push("lock contractVersion does not match manifest");
  for (const artifact of manifest.artifacts) {
    const expected = lock.artifacts[artifact];
    if (!expected) {
      errors.push(`lock is missing digest: ${artifact}`);
      continue;
    }
    const actual = createHash("sha256").update(readFileSync(resolve(root, artifact))).digest("hex");
    if (actual !== expected) errors.push(`locked artifact changed: ${artifact}`);
  }
}

function writeLock() {
  const artifacts = Object.fromEntries(manifest.artifacts.map((artifact) => [
    artifact,
    createHash("sha256").update(readFileSync(resolve(root, artifact))).digest("hex"),
  ]));
  writeFileSync(lockPath, `${JSON.stringify({ contractVersion: manifest.contractVersion, artifacts }, null, 2)}\n`);
}

function verifyPngDimensions(artifact: string) {
  const expected = artifact.match(/-(\d+)x(\d+)\.png$/);
  if (!expected) return;
  const png = readFileSync(resolve(root, artifact));
  const isPng = png.length >= 24 && png.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]));
  if (!isPng) {
    errors.push(`baseline is not a PNG: ${artifact}`);
    return;
  }
  const actualWidth = png.readUInt32BE(16);
  const actualHeight = png.readUInt32BE(20);
  if (actualWidth !== Number(expected[1]) || actualHeight < Number(expected[2])) {
    errors.push(`baseline dimensions do not cover the named viewport: ${artifact} is ${actualWidth}x${actualHeight}`);
  }
}

function readJson<T>(path: string): T {
  return JSON.parse(readFileSync(path, "utf8")) as T;
}
