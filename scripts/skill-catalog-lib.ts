import {
  createHash,
  createPublicKey,
  timingSafeEqual,
  verify as verifySignature,
} from "node:crypto";

export const CATALOG_SCHEMA_VERSION = 1;
export const CATALOG_CHANNEL = "stable";
export const CATALOG_KEY_ID = "stm-skill-catalog-2026-7445f242";
export const CATALOG_PUBLIC_KEY_PEM = `-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAqcsZpyWQLiK2SdtyTgr1A6QozidPGMw1mahSUQQefd8=
-----END PUBLIC KEY-----
`;
export const MAX_CATALOG_BYTES = 1_048_576;
export const MAX_MANIFEST_BYTES = 16_384;
export const MAX_SIGNATURE_BYTES = 4_096;
export const MAX_SOURCE_FILES = 256;
export const MAX_SOURCE_FILE_BYTES = 1_048_576;
export const MAX_SOURCE_TOTAL_BYTES = 8_388_608;

const SAFE_SEGMENT = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;
const SAFE_RELATIVE_PATH = /^[A-Za-z0-9][A-Za-z0-9._-]*(\/[A-Za-z0-9][A-Za-z0-9._-]*)*$/;
const HEX_40 = /^[0-9a-f]{40}$/;
const HEX_64 = /^[0-9a-f]{64}$/;
const BASE64_SIGNATURE = /^[A-Za-z0-9+/]{86}==$/;
const CLIENT_ORDER: Readonly<Record<string, number>> = {
  Codex: 0,
  "Claude Code": 1,
  AgentKit: 2,
};

export type SkillClientName = "Codex" | "Claude Code" | "AgentKit";

export interface TrustedSkillSource {
  repository: string;
  subpath: string;
  commit: string;
  treeSha256: string;
}

export interface TrustedSkillTarget {
  client: SkillClientName;
  relativePath: string;
}

export interface TrustedSkillEntry {
  id: string;
  name: string;
  description: string;
  publisher: string;
  purposes: string[];
  riskFlags: string[];
  source: TrustedSkillSource;
  targets: TrustedSkillTarget[];
}

export interface AuthenticatedSkillCatalog {
  schemaVersion: 1;
  catalogVersion: number;
  channel: "stable";
  skills: TrustedSkillEntry[];
}

export interface SkillCatalogManifest {
  schemaVersion: 1;
  catalogVersion: number;
  channel: "stable";
  createdAt: string;
  expiresAt: string;
  payloadSha256: string;
  payloadLength: number;
}

export interface SkillCatalogSignature {
  schemaVersion: 1;
  algorithm: "Ed25519";
  keyId: string;
  signature: string;
}

export interface VerifyOptions {
  now?: Date;
  minimumVersion?: number;
  acceptedPayloadSha256AtMinimumVersion?: string;
}

export interface PinnedSourceTree {
  files: Array<{ path: string; bytes: Buffer }>;
  treeSha256: string;
}

function fail(message: string): never {
  throw new Error(message);
}

function record(value: unknown, context: string): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    fail(`${context} must be an object`);
  }
  return value as Record<string, unknown>;
}

function exactKeys(
  value: Record<string, unknown>,
  keys: readonly string[],
  context: string,
): void {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index])) {
    fail(`${context} must contain exactly: ${expected.join(", ")}`);
  }
}

function stringValue(value: unknown, context: string, maximum = 1_000): string {
  if (typeof value !== "string" || value.length === 0 || value.length > maximum) {
    fail(`${context} must be a non-empty string of at most ${maximum} characters`);
  }
  return value;
}

function safeSegment(value: unknown, context: string): string {
  const parsed = stringValue(value, context, 100);
  if (!SAFE_SEGMENT.test(parsed) || parsed === "." || parsed === "..") {
    fail(`${context} is not a safe segment`);
  }
  return parsed;
}

export function assertSafeRelativePath(value: unknown, context: string): string {
  const parsed = stringValue(value, context, 512);
  if (
    !SAFE_RELATIVE_PATH.test(parsed) ||
    parsed.includes("\\") ||
    parsed.includes("\0") ||
    parsed.split("/").some((part) => part === "." || part === "..") ||
    Buffer.byteLength(parsed, "utf8") > 512 ||
    parsed.split("/").length > 16
  ) {
    fail(`${context} is not a safe relative UTF-8 path`);
  }
  return parsed;
}

function integer(value: unknown, context: string, minimum: number, maximum: number): number {
  if (!Number.isSafeInteger(value) || (value as number) < minimum || (value as number) > maximum) {
    fail(`${context} must be an integer from ${minimum} through ${maximum}`);
  }
  return value as number;
}

function uniqueSegments(value: unknown, context: string, minimum: number): string[] {
  if (!Array.isArray(value) || value.length < minimum || value.length > 32) {
    fail(`${context} must contain ${minimum} through 32 values`);
  }
  const parsed = value.map((item, index) => safeSegment(item, `${context}[${index}]`));
  if (new Set(parsed).size !== parsed.length) fail(`${context} contains a duplicate`);
  return parsed;
}

function parseRepository(value: unknown, context: string): { canonical: string; owner: string; repo: string } {
  const raw = stringValue(value, context, 300);
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    fail(`${context} must be an absolute URL`);
  }
  if (
    url.protocol !== "https:" ||
    url.hostname !== "github.com" ||
    url.port !== "" ||
    url.username !== "" ||
    url.password !== "" ||
    url.search !== "" ||
    url.hash !== ""
  ) {
    fail(`${context} must be a credential-free https://github.com URL`);
  }
  const match = /^\/([A-Za-z0-9_.-]+)\/([A-Za-z0-9_.-]+)\.git$/.exec(url.pathname);
  if (!match || match[1] === "." || match[1] === ".." || match[2] === "." || match[2] === "..") {
    fail(`${context} must end in /owner/repository.git`);
  }
  const canonical = `https://github.com/${match[1]}/${match[2]}.git`;
  if (raw !== canonical) fail(`${context} must use canonical spelling`);
  return { canonical, owner: match[1], repo: match[2] };
}

function parseTimestamp(value: unknown, context: string): { raw: string; milliseconds: number } {
  const raw = stringValue(value, context, 40);
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(raw)) {
    fail(`${context} must be a UTC RFC 3339 timestamp with second precision`);
  }
  const milliseconds = Date.parse(raw);
  if (!Number.isFinite(milliseconds) || new Date(milliseconds).toISOString() !== raw.replace("Z", ".000Z")) {
    fail(`${context} is not a valid timestamp`);
  }
  return { raw, milliseconds };
}

export function parseCatalog(bytes: Uint8Array): AuthenticatedSkillCatalog {
  if (bytes.byteLength === 0 || bytes.byteLength > MAX_CATALOG_BYTES) fail("catalog payload size is out of bounds");
  let raw: unknown;
  try {
    raw = JSON.parse(Buffer.from(bytes).toString("utf8"));
  } catch {
    fail("catalog payload is not valid UTF-8 JSON");
  }
  const catalog = record(raw, "catalog");
  exactKeys(catalog, ["schemaVersion", "catalogVersion", "channel", "skills"], "catalog");
  if (catalog.schemaVersion !== CATALOG_SCHEMA_VERSION) fail("catalog schemaVersion is unsupported");
  const catalogVersion = integer(catalog.catalogVersion, "catalog.catalogVersion", 1, Number.MAX_SAFE_INTEGER);
  if (catalog.channel !== CATALOG_CHANNEL) fail("catalog channel must be stable");
  if (!Array.isArray(catalog.skills) || catalog.skills.length === 0 || catalog.skills.length > 1_000) {
    fail("catalog.skills must contain 1 through 1000 entries");
  }

  const ids = new Set<string>();
  const names = new Set<string>();
  const sourceIdentities = new Set<string>();
  const targetIdentities = new Set<string>();
  const skills = catalog.skills.map((item, index): TrustedSkillEntry => {
    const context = `catalog.skills[${index}]`;
    const entry = record(item, context);
    exactKeys(entry, ["id", "name", "description", "publisher", "purposes", "riskFlags", "source", "targets"], context);
    const id = safeSegment(entry.id, `${context}.id`);
    const name = stringValue(entry.name, `${context}.name`, 100);
    const description = stringValue(entry.description, `${context}.description`, 1_000);
    const publisher = safeSegment(entry.publisher, `${context}.publisher`);
    const purposes = uniqueSegments(entry.purposes, `${context}.purposes`, 1);
    const riskFlags = uniqueSegments(entry.riskFlags, `${context}.riskFlags`, 0);

    const sourceRaw = record(entry.source, `${context}.source`);
    exactKeys(sourceRaw, ["repository", "subpath", "commit", "treeSha256"], `${context}.source`);
    const repository = parseRepository(sourceRaw.repository, `${context}.source.repository`);
    const subpath = assertSafeRelativePath(sourceRaw.subpath, `${context}.source.subpath`);
    const commit = stringValue(sourceRaw.commit, `${context}.source.commit`, 40);
    const treeSha256 = stringValue(sourceRaw.treeSha256, `${context}.source.treeSha256`, 64);
    if (!HEX_40.test(commit)) fail(`${context}.source.commit must be a lowercase full commit hash`);
    if (!HEX_64.test(treeSha256)) fail(`${context}.source.treeSha256 must be lowercase SHA-256`);
    if (publisher.toLowerCase() !== repository.owner.toLowerCase()) {
      fail(`${context}.publisher must match the GitHub repository owner`);
    }
    if (id !== subpath.split("/").at(-1)) fail(`${context}.id must match the source subpath basename`);

    if (!Array.isArray(entry.targets) || entry.targets.length === 0 || entry.targets.length > 3) {
      fail(`${context}.targets must contain 1 through 3 targets`);
    }
    const localClients = new Set<string>();
    const targets = entry.targets.map((item, targetIndex): TrustedSkillTarget => {
      const targetContext = `${context}.targets[${targetIndex}]`;
      const target = record(item, targetContext);
      exactKeys(target, ["client", "relativePath"], targetContext);
      if (CLIENT_ORDER[target.client as string] === undefined) fail(`${targetContext}.client is unsupported`);
      const client = target.client as SkillClientName;
      const relativePath = assertSafeRelativePath(target.relativePath, `${targetContext}.relativePath`);
      if (relativePath !== id) fail(`${targetContext}.relativePath must equal the catalog skill id`);
      if (!localClients.add(client)) fail(`${context} contains duplicate target client ${client}`);
      const targetIdentity = `${client}\0${relativePath}`;
      if (!targetIdentities.add(targetIdentity)) fail(`duplicate catalog target identity ${client}:${relativePath}`);
      return { client, relativePath };
    });

    if (!ids.add(id)) fail(`duplicate skill id ${id}`);
    const foldedName = name.normalize("NFKC").toLocaleLowerCase("en-US");
    if (!names.add(foldedName)) fail(`duplicate normalized skill name ${name}`);
    const sourceIdentity = `${repository.canonical.toLowerCase()}\0${subpath}`;
    if (!sourceIdentities.add(sourceIdentity)) fail(`duplicate source identity ${repository.canonical}:${subpath}`);

    return {
      id,
      name,
      description,
      publisher,
      purposes,
      riskFlags,
      source: { repository: repository.canonical, subpath, commit, treeSha256 },
      targets,
    };
  });

  for (let index = 1; index < skills.length; index += 1) {
    if (Buffer.compare(Buffer.from(skills[index - 1].id), Buffer.from(skills[index].id)) >= 0) {
      fail("catalog skills must be strictly sorted by UTF-8 id bytes");
    }
  }
  for (const entry of skills) {
    for (let index = 1; index < entry.targets.length; index += 1) {
      if (CLIENT_ORDER[entry.targets[index - 1].client] >= CLIENT_ORDER[entry.targets[index].client]) {
        fail(`catalog skill ${entry.id} targets must use canonical client order`);
      }
    }
  }

  return { schemaVersion: 1, catalogVersion, channel: "stable", skills };
}

export function parseManifest(bytes: Uint8Array): SkillCatalogManifest {
  if (bytes.byteLength === 0 || bytes.byteLength > MAX_MANIFEST_BYTES) fail("manifest size is out of bounds");
  let raw: unknown;
  try {
    raw = JSON.parse(Buffer.from(bytes).toString("utf8"));
  } catch {
    fail("manifest is not valid UTF-8 JSON");
  }
  const manifest = record(raw, "manifest");
  exactKeys(manifest, ["schemaVersion", "catalogVersion", "channel", "createdAt", "expiresAt", "payloadSha256", "payloadLength"], "manifest");
  if (manifest.schemaVersion !== 1) fail("manifest schemaVersion is unsupported");
  const catalogVersion = integer(manifest.catalogVersion, "manifest.catalogVersion", 1, Number.MAX_SAFE_INTEGER);
  if (manifest.channel !== "stable") fail("manifest channel must be stable");
  const createdAt = parseTimestamp(manifest.createdAt, "manifest.createdAt");
  const expiresAt = parseTimestamp(manifest.expiresAt, "manifest.expiresAt");
  if (expiresAt.milliseconds <= createdAt.milliseconds) fail("manifest expiry must follow creation");
  if (expiresAt.milliseconds - createdAt.milliseconds > 366 * 24 * 60 * 60 * 1_000) {
    fail("manifest validity window exceeds 366 days");
  }
  const payloadSha256 = stringValue(manifest.payloadSha256, "manifest.payloadSha256", 64);
  if (!HEX_64.test(payloadSha256)) fail("manifest.payloadSha256 must be lowercase SHA-256");
  const payloadLength = integer(manifest.payloadLength, "manifest.payloadLength", 1, MAX_CATALOG_BYTES);
  return {
    schemaVersion: 1,
    catalogVersion,
    channel: "stable",
    createdAt: createdAt.raw,
    expiresAt: expiresAt.raw,
    payloadSha256,
    payloadLength,
  };
}

export function parseDetachedSignature(bytes: Uint8Array): SkillCatalogSignature {
  if (bytes.byteLength === 0 || bytes.byteLength > MAX_SIGNATURE_BYTES) fail("signature document size is out of bounds");
  let raw: unknown;
  try {
    raw = JSON.parse(Buffer.from(bytes).toString("utf8"));
  } catch {
    fail("signature document is not valid UTF-8 JSON");
  }
  const signature = record(raw, "signature");
  exactKeys(signature, ["schemaVersion", "algorithm", "keyId", "signature"], "signature");
  if (signature.schemaVersion !== 1) fail("signature schemaVersion is unsupported");
  if (signature.algorithm !== "Ed25519") fail("signature algorithm must be Ed25519");
  const keyId = stringValue(signature.keyId, "signature.keyId", 100);
  if (!/^[a-z0-9][a-z0-9-]{0,99}$/.test(keyId)) fail("signature.keyId is malformed");
  const encoded = stringValue(signature.signature, "signature.signature", 88);
  if (!BASE64_SIGNATURE.test(encoded)) fail("signature.signature is not canonical padded base64");
  const decoded = Buffer.from(encoded, "base64");
  if (decoded.length !== 64 || decoded.toString("base64") !== encoded) fail("signature.signature must encode exactly 64 bytes");
  return { schemaVersion: 1, algorithm: "Ed25519", keyId, signature: encoded };
}

export function sha256Hex(bytes: Uint8Array): string {
  return createHash("sha256").update(bytes).digest("hex");
}

export function verifyAuthenticatedCatalog(
  catalogBytes: Uint8Array,
  manifestBytes: Uint8Array,
  signatureBytes: Uint8Array,
  options: VerifyOptions = {},
): { catalog: AuthenticatedSkillCatalog; manifest: SkillCatalogManifest } {
  const signature = parseDetachedSignature(signatureBytes);
  if (signature.keyId !== CATALOG_KEY_ID) fail(`unknown catalog signing key ${signature.keyId}`);
  const valid = verifySignature(
    null,
    Buffer.from(manifestBytes),
    createPublicKey(CATALOG_PUBLIC_KEY_PEM),
    Buffer.from(signature.signature, "base64"),
  );
  if (!valid) fail("catalog manifest signature is invalid");

  const manifest = parseManifest(manifestBytes);
  const now = (options.now ?? new Date()).getTime();
  if (!Number.isFinite(now)) fail("verification clock is invalid");
  if (Date.parse(manifest.createdAt) > now + 5 * 60 * 1_000) fail("catalog manifest is not yet valid");
  if (Date.parse(manifest.expiresAt) <= now) fail("catalog manifest has expired");
  if (catalogBytes.byteLength !== manifest.payloadLength) fail("catalog payload length does not match manifest");
  const payloadHash = sha256Hex(catalogBytes);
  const actualHash = Buffer.from(payloadHash, "hex");
  const expectedHash = Buffer.from(manifest.payloadSha256, "hex");
  if (!timingSafeEqual(actualHash, expectedHash)) fail("catalog payload hash does not match manifest");

  const catalog = parseCatalog(catalogBytes);
  if (catalog.catalogVersion !== manifest.catalogVersion || catalog.channel !== manifest.channel) {
    fail("catalog identity does not match manifest");
  }
  if (options.minimumVersion !== undefined) {
    integer(options.minimumVersion, "minimumVersion", 1, Number.MAX_SAFE_INTEGER);
    if (catalog.catalogVersion < options.minimumVersion) fail("catalog version is a downgrade");
    if (
      catalog.catalogVersion === options.minimumVersion &&
      options.acceptedPayloadSha256AtMinimumVersion !== undefined &&
      payloadHash !== options.acceptedPayloadSha256AtMinimumVersion
    ) {
      fail("catalog content drifted at the accepted version");
    }
  }
  return { catalog, manifest };
}

export function computeTreeSha256(files: ReadonlyArray<{ path: string; bytes: Uint8Array }>): string {
  if (files.length === 0 || files.length > MAX_SOURCE_FILES) fail("source file count is out of bounds");
  const sorted = files.map(({ path, bytes }) => ({
    path: assertSafeRelativePath(path, "source file path"),
    bytes: Buffer.from(bytes),
  })).sort((left, right) => Buffer.compare(Buffer.from(left.path), Buffer.from(right.path)));
  let total = 0;
  const seen = new Set<string>();
  const hash = createHash("sha256");
  for (const file of sorted) {
    if (!seen.add(file.path)) fail(`duplicate source file path ${file.path}`);
    if (file.bytes.length > MAX_SOURCE_FILE_BYTES) fail(`source file ${file.path} exceeds the size limit`);
    total += file.bytes.length;
    if (total > MAX_SOURCE_TOTAL_BYTES) fail("source tree exceeds the total size limit");
    const pathBytes = Buffer.from(file.path, "utf8");
    const pathLength = Buffer.allocUnsafe(4);
    pathLength.writeUInt32BE(pathBytes.length);
    const contentLength = Buffer.allocUnsafe(8);
    contentLength.writeBigUInt64BE(BigInt(file.bytes.length));
    hash.update(pathLength);
    hash.update(pathBytes);
    hash.update(contentLength);
    hash.update(file.bytes);
  }
  if (!seen.has("SKILL.md")) fail("source tree is missing required SKILL.md");
  return hash.digest("hex");
}

async function boundedFetch(url: URL, maximumBytes: number): Promise<Buffer> {
  const response = await fetch(url, {
    redirect: "error",
    headers: { Accept: "application/vnd.github+json", "User-Agent": "stm-skill-catalog-verifier" },
    signal: AbortSignal.timeout(15_000),
  });
  if (!response.ok || !response.body) fail(`source request failed for ${url.origin} with HTTP ${response.status}`);
  const declared = response.headers.get("content-length");
  if (declared !== null && Number(declared) > maximumBytes) fail(`source response from ${url.origin} exceeds the size limit`);
  const chunks: Buffer[] = [];
  let length = 0;
  for await (const chunk of response.body) {
    const bytes = Buffer.from(chunk);
    length += bytes.length;
    if (length > maximumBytes) fail(`source response from ${url.origin} exceeds the size limit`);
    chunks.push(bytes);
  }
  return Buffer.concat(chunks, length);
}

export async function fetchPinnedSourceTree(source: TrustedSkillSource): Promise<PinnedSourceTree> {
  const repository = parseRepository(source.repository, "source.repository");
  if (!HEX_40.test(source.commit)) fail("source.commit must be a full lowercase commit hash");
  const subpath = assertSafeRelativePath(source.subpath, "source.subpath");
  const treeUrl = new URL(
    `https://api.github.com/repos/${encodeURIComponent(repository.owner)}/${encodeURIComponent(repository.repo)}/git/trees/${source.commit}?recursive=1`,
  );
  const treeBytes = await boundedFetch(treeUrl, 4_194_304);
  let raw: unknown;
  try {
    raw = JSON.parse(treeBytes.toString("utf8"));
  } catch {
    fail("GitHub tree response is not valid JSON");
  }
  const root = record(raw, "GitHub tree response");
  if (root.truncated === true) fail("GitHub tree response was truncated");
  if (root.sha !== source.commit) fail("GitHub did not resolve the requested immutable commit");
  if (!Array.isArray(root.tree)) fail("GitHub tree response has no tree");
  const prefix = `${subpath}/`;
  const candidates: Array<{ relativePath: string; fullPath: string }> = [];
  let foundRoot = false;
  for (const [index, rawEntry] of root.tree.entries()) {
    const entry = record(rawEntry, `GitHub tree[${index}]`);
    if (entry.path === subpath && entry.type === "tree") foundRoot = true;
    if (typeof entry.path !== "string" || !entry.path.startsWith(prefix)) continue;
    const relativePath = assertSafeRelativePath(entry.path.slice(prefix.length), `GitHub tree[${index}].path`);
    if (entry.type === "tree") continue;
    if (entry.type !== "blob" || (entry.mode !== "100644" && entry.mode !== "100755")) {
      fail(`source contains unsupported, symlink, or special entry ${relativePath}`);
    }
    const size = integer(entry.size, `GitHub tree[${index}].size`, 0, MAX_SOURCE_FILE_BYTES);
    if (size > MAX_SOURCE_FILE_BYTES) fail(`source file ${relativePath} exceeds the size limit`);
    candidates.push({ relativePath, fullPath: entry.path });
  }
  if (!foundRoot) fail("source subpath is not a directory at the pinned commit");
  if (candidates.length === 0 || candidates.length > MAX_SOURCE_FILES) fail("source file count is out of bounds");
  candidates.sort((left, right) => Buffer.compare(Buffer.from(left.relativePath), Buffer.from(right.relativePath)));
  const files: Array<{ path: string; bytes: Buffer }> = [];
  for (const candidate of candidates) {
    const encodedPath = candidate.fullPath.split("/").map(encodeURIComponent).join("/");
    const rawUrl = new URL(
      `https://raw.githubusercontent.com/${encodeURIComponent(repository.owner)}/${encodeURIComponent(repository.repo)}/${source.commit}/${encodedPath}`,
    );
    files.push({ path: candidate.relativePath, bytes: await boundedFetch(rawUrl, MAX_SOURCE_FILE_BYTES) });
  }
  const treeSha256 = computeTreeSha256(files);
  if (treeSha256 !== source.treeSha256) fail(`pinned source tree digest mismatch for ${source.repository}:${source.subpath}`);
  return { files, treeSha256 };
}

export async function verifyPinnedCatalogSources(catalog: AuthenticatedSkillCatalog): Promise<void> {
  for (const entry of catalog.skills) await fetchPinnedSourceTree(entry.source);
}

export function stableJsonBytes(value: unknown): Buffer {
  return Buffer.from(`${JSON.stringify(value, null, 2)}\n`, "utf8");
}
