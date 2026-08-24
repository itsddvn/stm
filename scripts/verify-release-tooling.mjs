import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";
import { execFile } from "node:child_process";

const run = promisify(execFile);
const root = resolve(".");

async function invoke(script, args = [], env = {}) {
  try {
    const result = await run(process.execPath, ["--import", "tsx", resolve(script), ...args], {
      cwd: root,
      env: { ...process.env, ...env },
    });
    return { code: 0, stdout: result.stdout, stderr: result.stderr };
  } catch (error) {
    return {
      code: error.code ?? 1,
      stdout: error.stdout ?? "",
      stderr: error.stderr ?? String(error),
    };
  }
}

async function main() {
  const temp = await mkdtemp(join(tmpdir(), "stm-release-tooling-"));
  try {
    const artifacts = join(temp, "artifacts");
    await mkdir(artifacts);
    await writeFile(join(artifacts, "STM_0.1.0_amd64.AppImage"), "artifact\n");
    await writeFile(join(artifacts, "STM_0.1.0_amd64.AppImage.sig"), "RWTGZml4dHVyZVNpZ25hdHVyZUJ5dGVzRm9yUmVsZWFzZVZlcmlmaWNhdGlvbjAx\n");
    await writeFile(join(artifacts, "STM_amd64.AppImage.tar.gz"), "updater\n");
    await writeFile(join(artifacts, "STM_amd64.AppImage.tar.gz.sig"), "RWTGZml4dHVyZVNpZ25hdHVyZUJ5dGVzRm9yUmVsZWFzZVZlcmlmaWNhdGlvbjAy\n");
    const manifestPath = join(artifacts, "latest.json");
    await writeFile(manifestPath, JSON.stringify({
      version: "0.1.0",
      platforms: {
        "linux-x86_64": {
          url: "https://github.com/itsddvn/stm/releases/download/v0.1.0/STM_0.1.0_amd64.AppImage",
          signature: "RWTGZml4dHVyZVNpZ25hdHVyZUJ5dGVzRm9yUmVsZWFzZVZlcmlmaWNhdGlvbjAx",
        },
      },
    }));
    const success = await invoke("scripts/verify-release-artifacts.ts", [artifacts, "0.1.0", manifestPath]);
    if (success.code !== 0 || !success.stdout.includes("Verified 5 release artifacts")) {
      throw new Error(`valid release artifacts failed verification: ${success.stderr}`);
    }
    const downgrade = await invoke("scripts/verify-release-artifacts.ts", [artifacts, "0.2.0", manifestPath]);
    if (downgrade.code === 0 || !downgrade.stderr.includes("version")) {
      throw new Error("wrong-version updater manifest was not rejected");
    }

    const generated = join(temp, "tauri.release.generated.json");
    const missingKey = await invoke("scripts/prepare-release-config.ts", [generated], { TAURI_UPDATER_PUBLIC_KEY: "" });
    if (missingKey.code === 0) throw new Error("release config accepted a missing updater key");
    const publicDocument = "untrusted comment: minisign public key: 0123456789ABCDEF\nRWQ7jvN7JjQYdUjmdZmR2pAQzD5k5iER65fUn9J2b9v6YQfMTc1F8i4=\n";
    const publicKey = Buffer.from(publicDocument).toString("base64");
    const configured = await invoke("scripts/prepare-release-config.ts", [generated], { TAURI_UPDATER_PUBLIC_KEY: publicKey });
    if (configured.code !== 0 || configured.stdout.includes(publicKey)) {
      throw new Error("release config generation failed or exposed key material in output");
    }
    const config = JSON.parse(await readFile(generated, "utf8"));
    if (config.plugins?.updater?.pubkey !== publicKey || config.bundle?.createUpdaterArtifacts !== true) {
      throw new Error("generated release config omitted updater trust or artifacts");
    }
    const secretWrapper = Buffer.from(`${publicDocument}untrusted comment: minisign secret key\nRWRleGFtcGxl\n`).toString("base64");
    const secretBearing = await invoke("scripts/prepare-release-config.ts", [generated], { TAURI_UPDATER_PUBLIC_KEY: secretWrapper });
    if (secretBearing.code === 0 || !secretBearing.stderr.includes("secret material")) {
      throw new Error("release config accepted encoded secret material");
    }
    process.stdout.write("Release tooling behavioral verification passed.\n");
  } finally {
    await rm(temp, { recursive: true, force: true });
  }
}

main().catch((error) => {
  process.stderr.write(`Release tooling verification failed: ${error instanceof Error ? error.message : String(error)}\n`);
  process.exitCode = 1;
});
