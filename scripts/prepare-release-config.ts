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
  if (!value || value.length < 40 || value.length > 2048 || /PRIVATE|SECRET/i.test(value)) {
    throw new Error("TAURI_UPDATER_PUBLIC_KEY must contain the approved Minisign public key");
  }
  if ([...value].some((character) => character < " " && character !== "\n" && character !== "\r")) {
    throw new Error("TAURI_UPDATER_PUBLIC_KEY contains control characters");
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
