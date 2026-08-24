import { lstat, readFile } from "node:fs/promises";
import { resolve } from "node:path";
import {
  MAX_CATALOG_BYTES,
  MAX_MANIFEST_BYTES,
  MAX_SIGNATURE_BYTES,
  verifyAuthenticatedCatalog,
  verifyPinnedCatalogSources,
} from "./skill-catalog-lib";

interface Arguments {
  catalog: string;
  manifest: string;
  signature: string;
  at?: Date;
  minimumVersion?: number;
  acceptedHash?: string;
  verifySource: boolean;
}

function parseArguments(argv: string[]): Arguments {
  const values: Record<string, string> = {};
  const valueFlags: Readonly<Record<string, true>> = {
    "--catalog": true,
    "--manifest": true,
    "--signature": true,
    "--at": true,
    "--minimum-version": true,
    "--accepted-hash": true,
  };
  let verifySource = false;
  for (let index = 0; index < argv.length; index += 1) {
    const flag = argv[index];
    if (flag === "--verify-source") {
      if (verifySource) throw new Error("--verify-source may be specified only once");
      verifySource = true;
      continue;
    }
    if (valueFlags[flag] !== true) throw new Error(`unknown argument ${flag}`);
    if (values[flag] !== undefined) throw new Error(`${flag} may be specified only once`);
    const value = argv[index + 1];
    if (value === undefined || value.startsWith("--")) throw new Error(`${flag} requires a value`);
    values[flag] = value;
    index += 1;
  }
  let at: Date | undefined;
  if (values["--at"] !== undefined) {
    at = new Date(values["--at"]);
    if (!Number.isFinite(at.getTime())) throw new Error("--at must be a valid timestamp");
  }
  let minimumVersion: number | undefined;
  if (values["--minimum-version"] !== undefined) {
    minimumVersion = Number(values["--minimum-version"]);
    if (!Number.isSafeInteger(minimumVersion) || minimumVersion < 1) {
      throw new Error("--minimum-version must be a positive safe integer");
    }
  }
  const acceptedHash = values["--accepted-hash"];
  if (acceptedHash !== undefined && !/^[0-9a-f]{64}$/.test(acceptedHash)) {
    throw new Error("--accepted-hash must be lowercase SHA-256");
  }
  if (acceptedHash !== undefined && minimumVersion === undefined) {
    throw new Error("--accepted-hash requires --minimum-version");
  }
  return {
    catalog: resolve(values["--catalog"] ?? "catalog/skills/stable/catalog.json"),
    manifest: resolve(values["--manifest"] ?? "catalog/skills/stable/manifest.json"),
    signature: resolve(values["--signature"] ?? "catalog/skills/stable/manifest.sig.json"),
    at,
    minimumVersion,
    acceptedHash,
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

async function main(): Promise<void> {
  const args = parseArguments(process.argv.slice(2));
  const [catalogBytes, manifestBytes, signatureBytes] = await Promise.all([
    readBounded(args.catalog, MAX_CATALOG_BYTES),
    readBounded(args.manifest, MAX_MANIFEST_BYTES),
    readBounded(args.signature, MAX_SIGNATURE_BYTES),
  ]);
  const verified = verifyAuthenticatedCatalog(catalogBytes, manifestBytes, signatureBytes, {
    now: args.at,
    minimumVersion: args.minimumVersion,
    acceptedPayloadSha256AtMinimumVersion: args.acceptedHash,
  });
  if (args.verifySource) await verifyPinnedCatalogSources(verified.catalog);
  process.stdout.write(
    `Verified authenticated ${verified.catalog.channel} catalog version ${verified.catalog.catalogVersion} (${verified.catalog.skills.length} skill).\n`,
  );
}

main().catch((error: unknown) => {
  const message = error instanceof Error ? error.message : String(error);
  process.stderr.write(`Catalog verification failed: ${message}\n`);
  process.exitCode = 1;
});
