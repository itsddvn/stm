import { lstat, mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

const templatePath = resolve("src-tauri/tauri.release.conf.json");
const outputPath = resolve(process.argv[2] ?? "target/release-config/tauri.release.generated.json");

async function readBoundedJson(path: string, maximumBytes: number): Promise<Record<string, unknown>> {
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > maximumBytes) {
    throw new Error(`${path} must be a bounded regular JSON file`);
  }
  const value = JSON.parse(await readFile(path, "utf8")) as unknown;
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${path} must contain a JSON object`);
  }
  return value as Record<string, unknown>;
}

function updaterPublicKey(): string {
  const value = process.env.TAURI_UPDATER_PUBLIC_KEY?.trim();
  if (!value || value.length < 40 || value.length > 2048 || !/^[A-Za-z0-9+/]+={0,2}$/.test(value)) {
    throw new Error("TAURI_UPDATER_PUBLIC_KEY must contain the Tauri-encoded Minisign public key");
  }
  const decoded = Buffer.from(value, "base64");
  if (decoded.toString("base64") !== value) {
    throw new Error("TAURI_UPDATER_PUBLIC_KEY is not canonical base64");
  }
  const document = decoded.toString("utf8");
  if (/PRIVATE|SECRET/i.test(document)) {
    throw new Error("TAURI_UPDATER_PUBLIC_KEY contains secret material");
  }
  const canonicalDocument = document.endsWith("\n") ? document.slice(0, -1) : document;
  const lines = canonicalDocument.split("\n");
  const commentId = lines[0]?.match(/^untrusted comment: minisign public key: ([A-Fa-f0-9]{8,16})$/)?.[1];
  if (
    lines.length !== 2
    || !commentId
    || !/^RW[A-Za-z0-9+/=]{40,128}$/.test(lines[1] ?? "")
  ) {
    throw new Error("TAURI_UPDATER_PUBLIC_KEY has an invalid public-key document");
  }
  return value;
}

async function main(): Promise<void> {
  const config = await readBoundedJson(templatePath, 64 * 1024);
  const plugins = config.plugins as Record<string, unknown>;
  const updater = plugins.updater as Record<string, unknown>;
  updater.pubkey = updaterPublicKey();

  if (process.platform === "win32") {
    const thumbprint = process.env.WINDOWS_CERTIFICATE_THUMBPRINT?.trim();
    if (!thumbprint || !/^[A-Fa-f0-9]{40,128}$/.test(thumbprint)) {
      throw new Error("WINDOWS_CERTIFICATE_THUMBPRINT is required for a signed Windows release");
    }
    const bundle = config.bundle as Record<string, unknown>;
    const windows = (bundle.windows ?? {}) as Record<string, unknown>;
    windows.certificateThumbprint = thumbprint;
    windows.digestAlgorithm = "sha256";
    windows.timestampUrl = "https://timestamp.digicert.com";
    bundle.windows = windows;
  }

  await mkdir(dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(config, null, 2)}\n`, { mode: 0o600 });
  process.stdout.write(`${outputPath}\n`);
}

main().catch((error: unknown) => {
  process.stderr.write(`Release config generation failed: ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
