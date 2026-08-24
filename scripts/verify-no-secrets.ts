import { lstat, readdir, readFile } from "node:fs/promises";
import { extname, relative, resolve, sep } from "node:path";

const root = resolve(".");
const skippedDirectories = new Set([".git", ".artifacts", ".pnpm-store", "dist", "node_modules", "target"]);
const textExtensions = new Set(["", ".json", ".md", ".mjs", ".rs", ".toml", ".ts", ".tsx", ".txt", ".yml", ".yaml"]);
const maximumBytes = 2 * 1024 * 1024;
const findings: string[] = [];

const secretPatterns: Array<[string, RegExp]> = [
  ["private key", /-----BEGIN (?:EC |OPENSSH |PGP |RSA )?PRIVATE KEY-----/],
  ["GitHub token", /\bgh[pousr]_[A-Za-z0-9]{30,}\b/],
  ["GitHub fine-grained token", /\bgithub_pat_[A-Za-z0-9_]{40,}\b/],
  ["AWS access key", /\b(?:AKIA|ASIA)[A-Z0-9]{16}\b/],
  ["Slack token", /\bxox[baprs]-[A-Za-z0-9-]{20,}\b/],
  ["npm token", /\bnpm_[A-Za-z0-9]{30,}\b/],
];

async function scan(path: string): Promise<void> {
  const metadata = await lstat(path);
  if (metadata.isSymbolicLink()) return;
  if (metadata.isDirectory()) {
    for (const entry of await readdir(path)) {
      if (skippedDirectories.has(entry)) continue;
      await scan(resolve(path, entry));
    }
    return;
  }
  if (!metadata.isFile() || metadata.size > maximumBytes || !textExtensions.has(extname(path))) return;
  const content = await readFile(path, "utf8");
  for (const [label, pattern] of secretPatterns) {
    if (pattern.test(content)) findings.push(`${relative(root, path).split(sep).join("/")}: ${label}`);
  }
}

scan(root)
  .then(() => {
    if (findings.length > 0) throw new Error(`potential secrets found:\n${findings.join("\n")}`);
    process.stdout.write("Repository secret pattern verification passed.\n");
  })
  .catch((error: unknown) => {
    process.stderr.write(`Secret verification failed: ${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
  });
