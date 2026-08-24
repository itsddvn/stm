import { createPrivateKey, createPublicKey, sign } from "node:crypto";
import { chmod, lstat, mkdir, readFile, rename, unlink, writeFile } from "node:fs/promises";
import { homedir } from "node:os";
import { dirname, resolve } from "node:path";
import {
  CATALOG_CHANNEL,
  CATALOG_KEY_ID,
  CATALOG_PUBLIC_KEY_PEM,
  CATALOG_SCHEMA_VERSION,
  MAX_CATALOG_BYTES,
  parseCatalog,
  sha256Hex,
  stableJsonBytes,
  verifyPinnedCatalogSources,
  type SkillCatalogManifest,
  type SkillCatalogSignature,
} from "./skill-catalog-lib";

interface Arguments {
  catalog: string;
  manifest: string;
  signature: string;
  key: string;
  createdAt: string;
  expiresAt: string;
  verifySource: boolean;
}

function parseArguments(argv: string[]): Arguments {
  const values: Partial<Arguments> = {};
  const flagToKey: Record<string, keyof Omit<Arguments, "verifySource">> = {
    "--catalog": "catalog",
    "--manifest": "manifest",
    "--signature": "signature",
    "--key": "key",
    "--created-at": "createdAt",
    "--expires-at": "expiresAt",
  };
  let verifySource = false;
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    if (flag === "--verify-source") {
      if (verifySource) throw new Error("--verify-source may be specified only once");
      verifySource = true;
      continue;
    }
    const key = flagToKey[flag];
    if (key === undefined) throw new Error(`unknown argument ${flag}`);
    if (values[key] !== undefined) throw new Error(`${flag} may be specified only once`);
    const value = argv[index + 1];
    if (value === undefined || value.startsWith("--")) throw new Error(`${flag} requires a value`);
    values[key] = value;
    index += 1;
  }
  const createdAt = values.createdAt;
  const expiresAt = values.expiresAt;
  if (createdAt === undefined || expiresAt === undefined) {
    throw new Error("--created-at and --expires-at are required for reproducible publication");
  }
  return {
    catalog: resolve(values.catalog ?? "catalog/skills/stable/catalog.json"),
    manifest: resolve(values.manifest ?? "catalog/skills/stable/manifest.json"),
    signature: resolve(values.signature ?? "catalog/skills/stable/manifest.sig.json"),
    key: resolve(values.key ?? `${homedir()}/.config/stm/signing/skill-catalog-ed25519.pem`),
    createdAt,
    expiresAt,
    verifySource,
  };
}

async function readBounded(path: string, maximum: number): Promise<Buffer> {
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.isSymbolicLink()) throw new Error(`${path} must be a regular file`);
  if (metadata.size < 1 || metadata.size > maximum) throw new Error(`${path} exceeds its size limit`);
  const bytes = await readFile(path);
  if (bytes.length > maximum) throw new Error(`${path} changed while it was read`);
  return bytes;
}

async function atomicWrite(path: string, bytes: Buffer): Promise<void> {
  await mkdir(dirname(path), { recursive: true, mode: 0o755 });
  const temporary = `${path}.tmp-${process.pid}-${Date.now()}`;
  try {
    await writeFile(temporary, bytes, { flag: "wx", mode: 0o644 });
    await chmod(temporary, 0o644);
    await rename(temporary, path);
  } catch (error) {
    try {
      await unlink(temporary);
    } catch {
      // Nothing to clean up when creation failed or rename completed.
    }
    throw error;
  }
}

async function main(): Promise<void> {
  const args = parseArguments(process.argv.slice(2));
  const catalogBytes = await readBounded(args.catalog, MAX_CATALOG_BYTES);
  const catalog = parseCatalog(catalogBytes);
  if (args.verifySource) await verifyPinnedCatalogSources(catalog);

  const keyMetadata = await lstat(args.key);
  if (!keyMetadata.isFile() || keyMetadata.isSymbolicLink()) throw new Error("offline signing key must be a regular file");
  if ((keyMetadata.mode & 0o777) !== 0o600) throw new Error("offline signing key must have mode 0600");
  if (typeof process.getuid === "function" && keyMetadata.uid !== process.getuid()) {
    throw new Error("offline signing key must be owned by the current user");
  }
  const privateKeyBytes = await readBounded(args.key, 16_384);
  const privateKey = createPrivateKey(privateKeyBytes);
  if (privateKey.asymmetricKeyType !== "ed25519") throw new Error("offline signing key is not Ed25519");
  const expectedPublic = createPublicKey(CATALOG_PUBLIC_KEY_PEM).export({ type: "spki", format: "der" });
  const actualPublic = createPublicKey(privateKey).export({ type: "spki", format: "der" });
  if (!Buffer.from(actualPublic).equals(Buffer.from(expectedPublic))) {
    throw new Error("offline signing key does not match the compiled catalog trust root");
  }

  const manifest: SkillCatalogManifest = {
    schemaVersion: CATALOG_SCHEMA_VERSION,
    catalogVersion: catalog.catalogVersion,
    channel: CATALOG_CHANNEL,
    createdAt: args.createdAt,
    expiresAt: args.expiresAt,
    payloadSha256: sha256Hex(catalogBytes),
    payloadLength: catalogBytes.length,
  };
  const manifestBytes = stableJsonBytes(manifest);
  const signature: SkillCatalogSignature = {
    schemaVersion: CATALOG_SCHEMA_VERSION,
    algorithm: "Ed25519",
    keyId: CATALOG_KEY_ID,
    signature: sign(null, manifestBytes, privateKey).toString("base64"),
  };
  await atomicWrite(args.manifest, manifestBytes);
  await atomicWrite(args.signature, stableJsonBytes(signature));
  process.stdout.write(`Signed catalog version ${catalog.catalogVersion} for ${CATALOG_CHANNEL}.\n`);
}

main().catch((error: unknown) => {
  const message = error instanceof Error ? error.message : String(error);
  process.stderr.write(`Catalog signing failed: ${message}\n`);
  process.exitCode = 1;
});
