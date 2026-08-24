import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { lstat, readdir, readFile, writeFile } from "node:fs/promises";
import { basename, relative, resolve, sep } from "node:path";

const root = resolve(process.argv[2] ?? "");
const expectedVersion = (process.argv[3] ?? "").replace(/^v/, "");
const manifestArgument = process.argv[4];
const outputPath = resolve(process.argv[5] ?? `${root}/release-checksums.json`);
const maximumArtifactBytes = 2 * 1024 * 1024 * 1024;
const allowedSuffixes = [".AppImage", ".deb", ".dmg", ".exe", ".gz", ".json", ".msi", ".sig", ".zip"];
const installerSuffixes = [".AppImage", ".deb", ".dmg", ".exe", ".msi"];

interface ArtifactDigest {
  path: string;
  bytes: number;
  sha256: string;
}

function assertInside(path: string): void {
  const local = relative(root, path);
  if (!local || local.startsWith("..") || local.includes(`..${sep}`)) {
    throw new Error(`artifact path escapes release root: ${path}`);
  }
}

async function collect(path: string, output: string[], depth = 0): Promise<void> {
  if (depth > 8 || output.length > 1000) throw new Error("release artifact tree exceeds bounds");
  const metadata = await lstat(path);
  if (metadata.isSymbolicLink()) throw new Error(`release artifact symlink rejected: ${path}`);
  if (metadata.isDirectory()) {
    for (const entry of await readdir(path)) await collect(resolve(path, entry), output, depth + 1);
    return;
  }
  if (!metadata.isFile() || metadata.size === 0 || metadata.size > maximumArtifactBytes) {
    throw new Error(`release artifact is empty, non-regular, or oversized: ${path}`);
  }
  assertInside(path);
  if (path !== outputPath) output.push(path);
}

async function digest(path: string): Promise<ArtifactDigest> {
  const metadata = await lstat(path);
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk as Buffer);
  return {
    path: relative(root, path).split(sep).join("/"),
    bytes: metadata.size,
    sha256: hash.digest("hex"),
  };
}

async function verifyUpdaterManifest(path: string): Promise<void> {
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > 256 * 1024) {
    throw new Error("updater manifest must be a bounded regular file");
  }
  const manifest = JSON.parse(await readFile(path, "utf8")) as Record<string, unknown>;
  if (String(manifest.version ?? "").replace(/^v/, "") !== expectedVersion) {
    throw new Error("updater manifest version does not match release version");
  }
  const platforms = manifest.platforms;
  if (!platforms || typeof platforms !== "object" || Array.isArray(platforms)) {
    throw new Error("updater manifest platforms are missing");
  }
  for (const [target, raw] of Object.entries(platforms as Record<string, unknown>)) {
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) throw new Error(`invalid updater target ${target}`);
    const entry = raw as Record<string, unknown>;
    const url = new URL(String(entry.url ?? ""));
    if (url.protocol !== "https:" || url.username || url.password || url.hash) {
      throw new Error(`updater URL for ${target} is not credential-free HTTPS`);
    }
    const signature = String(entry.signature ?? "");
    if (signature.length < 40 || signature.length > 4096 || /PRIVATE|SECRET/i.test(signature)) {
      throw new Error(`updater signature for ${target} is malformed`);
    }
  }
}

async function main(): Promise<void> {
  if (!root || !expectedVersion) throw new Error("usage: verify-release-artifacts <root> <version> [latest.json] [output]");
  const rootMetadata = await lstat(root);
  if (!rootMetadata.isDirectory() || rootMetadata.isSymbolicLink()) throw new Error("release root must be a directory");
  const files: string[] = [];
  await collect(root, files);
  const artifacts = files.filter((path) => allowedSuffixes.some((suffix) => path.endsWith(suffix)));
  if (!artifacts.some((path) => installerSuffixes.some((suffix) => path.endsWith(suffix)))) {
    throw new Error("no supported installer artifact found");
  }
  if (!artifacts.some((path) => path.endsWith(".sig"))) {
    throw new Error("no signed updater artifact found");
  }
  if (artifacts.some((path) => !basename(path).includes(expectedVersion) && !basename(path).startsWith("latest.json"))) {
    throw new Error("release artifact filename does not contain the release version");
  }
  if (manifestArgument) await verifyUpdaterManifest(resolve(manifestArgument));
  const digests = await Promise.all(artifacts.sort().map(digest));
  await writeFile(
    outputPath,
    `${JSON.stringify({ schemaVersion: 1, version: expectedVersion, artifacts: digests }, null, 2)}\n`,
    { mode: 0o600 },
  );
  process.stdout.write(`Verified ${digests.length} release artifacts for ${expectedVersion}.\n`);
}

main().catch((error: unknown) => {
  process.stderr.write(`Release artifact verification failed: ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
