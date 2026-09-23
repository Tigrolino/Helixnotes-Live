// HelixNotes collaboration server - Stage 2 transport + Stage 3 relay + Stage 5 file uploads +
// Stage 6 live-document persistence.
//
// Scope (see the "HelixNotes Collaboration - Technical Analysis" doc, section 10): this server
// authenticates a connection against a shared workspace password, then relays messages between
// clients:
//
//   - Binary frames (Yjs sync-step/update messages, Stage 3+) are broadcast to every OTHER
//     client currently authenticated into the same workspace - this part is still a dumb,
//     content-agnostic relay, unchanged since Stage 3. As of Stage 6, the server ALSO keeps one
//     in-memory shadow Y.Doc per workspace up to date from those same frames (see "Live document
//     persistence" further down) purely to save and restore state - it still has no idea what a
//     "note" or a "notebook" is; a Yjs update is just an opaque, content-agnostic byte blob to it
//     either way.
//   - Text frames are still echoed back to the sender only, unchanged. This is Stage 2's original
//     debug/proof-of-transport behavior (auth aside), kept as-is since nothing currently depends
//     on relaying text between peers and collapsing it into the same broadcast path would change
//     its meaning for no benefit yet.
//
// Protocol (matches src-tauri/src/collab.rs on the client):
//   1. Client connects and, as its first message, sends:
//        {"type":"auth","workspace":"<id>","password":"<shared secret>"}
//   2. Server replies {"type":"connected"} on success, or closes the socket (with a close reason
//      the client surfaces as its error detail) on failure or timeout.
//   3. After that: binary frames are broadcast to other clients in the same workspace; text
//      frames are echoed back to the sender.
//
// Stage 5 (file/image attachments) adds two plain HTTP routes alongside the WebSocket, gated by
// the same shared workspace password rather than a new auth scheme:
//   - POST /upload/<workspace>  - raw file bytes as the body (no multipart - one file per
//     request), headers `X-Collab-Password` and `X-File-Name` (the original filename,
//     percent-encoded the same way `encodeURIComponent` would). Capped at UPLOAD_MAX_BYTES
//     (default 95 MB) - enforced against a lying/missing Content-Length too, not just the header.
//     Saved to local disk under UPLOADS_DIR; if GITHUB_TOKEN + GITHUB_REPO are set, also pushed
//     to that repo via the Contents API as a best-effort backup (a GitHub failure doesn't fail
//     the upload - the file is still on disk and still usable, just not backed up yet).
//   - GET /uploads/<workspace>/<stored-name>?password=<shared secret> - serves an uploaded file
//     back. The password travels in the query string (not a header) because this URL is what
//     gets embedded as an <img src> / <a href> in the note itself, where only a plain GET is
//     possible - consistent with this server's existing one-shared-secret-per-workspace model
//     (see §8 of the analysis doc for why there's no per-user auth here at all).
//
// Stage 6 (live document persistence) keeps one in-memory Yjs document per workspace, fed purely
// from the same binary frames the relay above already sees (decoding just the envelope - message
// type 0/1/2 in src/lib/collab/syncProtocol.ts - never anything about notes or notebooks), and
// saves it: debounced to local disk on every change, and - if GITHUB_TOKEN/GITHUB_REPO are set -
// throttled and also pushed to that repo, at snapshots/<workspace>.ydoc. Whoever next connects to
// that workspace (a reconnect, a fresh Render instance after a redeploy, everyone having left and
// come back later) gets synced from that saved state, even if nobody else happens to be online at
// that moment to sync from directly.
//
// This server's own disk is NOT durable (Render's free/starter filesystem is wiped on every
// redeploy/restart) - GITHUB_TOKEN/GITHUB_REPO is what makes an upload, or a workspace's saved
// document snapshot, survive that. Without them, both still work locally, they just don't outlive
// the next deploy.

// Load variables from a local .env file (see .env.example) into process.env, if one exists.
// This must run before anything below reads process.env - nothing else in the module graph
// loads it, and neither tsx (dev) nor a plain `node dist/server.js` (start) does this on their
// own. On Render (and other real hosts) there is no .env file; this call is then a harmless
// no-op and the environment variables set in the host's dashboard are used as-is.
import "dotenv/config";
import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { timingSafeEqual, randomUUID } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { WebSocketServer, WebSocket, type RawData } from "ws";
import * as Y from "yjs";
import * as encoding from "lib0/encoding";
import * as decoding from "lib0/decoding";

const PORT = Number(process.env.PORT ?? 8787);
const COLLAB_PASSWORD = process.env.COLLAB_PASSWORD ?? "";

if (!COLLAB_PASSWORD) {
  // Fail loudly at startup rather than silently accepting every connection.
  console.error("COLLAB_PASSWORD is not set - refusing to start. Set it in the environment (see .env.example).");
  process.exit(1);
}

const AUTH_TIMEOUT_MS = 10_000;
const HEARTBEAT_INTERVAL_MS = 30_000;

// --- Stage 5: file/image uploads ---
const UPLOAD_MAX_BYTES = Number(process.env.UPLOAD_MAX_BYTES ?? 95 * 1024 * 1024);
const UPLOADS_DIR = process.env.UPLOADS_DIR ?? "uploads";
const GITHUB_TOKEN = process.env.GITHUB_TOKEN ?? "";
const GITHUB_REPO = process.env.GITHUB_REPO ?? ""; // "owner/repo"
const GITHUB_BRANCH = process.env.GITHUB_BRANCH ?? "main";
const GITHUB_BACKUP_ENABLED = Boolean(GITHUB_TOKEN && GITHUB_REPO);
if (!GITHUB_BACKUP_ENABLED) {
  console.warn(
    "[collab] GITHUB_TOKEN/GITHUB_REPO not set - uploads are saved to local disk only, which " +
      "does not survive a redeploy. See README.md's \"GitHub backup for uploads\" section.",
  );
}

interface AuthMessage {
  type: "auth";
  workspace: string;
  password: string;
}

function isAuthMessage(value: unknown): value is AuthMessage {
  return (
    typeof value === "object" &&
    value !== null &&
    (value as { type?: unknown }).type === "auth" &&
    typeof (value as { workspace?: unknown }).workspace === "string" &&
    typeof (value as { password?: unknown }).password === "string"
  );
}

/** Constant-time password comparison so response timing doesn't leak how much of it matched. */
function passwordMatches(candidate: string): boolean {
  const expected = Buffer.from(COLLAB_PASSWORD, "utf8");
  const actual = Buffer.from(candidate, "utf8");
  if (expected.length !== actual.length) {
    // Still run a same-length comparison so this branch takes comparable time to the real one.
    timingSafeEqual(expected, expected);
    return false;
  }
  return timingSafeEqual(expected, actual);
}

/** A workspace id becomes a directory name on disk - restrict it to a safe charset rather than
 * trusting whatever the client sends (the WebSocket auth path doesn't need this restriction
 * since a workspace there is just a Map key, never a filesystem path). */
function isSafeWorkspaceSegment(segment: string): boolean {
  return /^[A-Za-z0-9_-]{1,128}$/.test(segment);
}

/** Same idea for a stored filename read back on GET - defense in depth even though every stored
 * name was already produced by sanitizeFilename() below, never taken verbatim from a client. */
function isSafeStoredFilename(segment: string): boolean {
  return /^[A-Za-z0-9._-]{1,220}$/.test(segment) && !segment.includes("..");
}

/** Strip an original filename down to something safe to put on disk and in a GitHub path: no
 * separators (so it can't escape its directory or be read as nested paths), no leading dots (so
 * it can't collide with a hidden/config file), bounded length. */
function sanitizeFilename(name: string): string {
  const base = name.split(/[\\/]/).pop() ?? "file";
  const cleaned = base.replace(/[^A-Za-z0-9._-]/g, "_").replace(/^\.+/, "");
  return (cleaned || "file").slice(0, 120);
}

const CONTENT_TYPES: Record<string, string> = {
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".gif": "image/gif",
  ".webp": "image/webp",
  ".svg": "image/svg+xml",
  ".bmp": "image/bmp",
  ".ico": "image/x-icon",
  ".pdf": "application/pdf",
  ".txt": "text/plain",
  ".md": "text/markdown",
  ".csv": "text/csv",
  ".json": "application/json",
  ".zip": "application/zip",
  ".mp4": "video/mp4",
  ".mov": "video/quicktime",
  ".mp3": "audio/mpeg",
  ".wav": "audio/wav",
  ".doc": "application/msword",
  ".docx": "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  ".xls": "application/vnd.ms-excel",
  ".xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  ".ppt": "application/vnd.ms-powerpoint",
  ".pptx": "application/vnd.openxmlformats-officedocument.presentationml.presentation",
};
function guessContentType(filename: string): string {
  return CONTENT_TYPES[extname(filename).toLowerCase()] ?? "application/octet-stream";
}

/** Buffers the request body, refusing (by destroying the underlying stream - `for await` calls
 * the async iterator's `return()` on an early throw, which for a Node Readable tears it down)
 * as soon as more than `limit` bytes have arrived. Content-Length is checked separately before
 * this is even called, but a client can lie about or omit it entirely, so the actual byte count
 * is what's enforced here, not the header. */
async function readBodyWithLimit(req: IncomingMessage, limit: number): Promise<Buffer> {
  const chunks: Buffer[] = [];
  let received = 0;
  for await (const chunk of req as AsyncIterable<Buffer>) {
    received += chunk.length;
    if (received > limit) {
      throw new Error("upload-too-large");
    }
    chunks.push(chunk);
  }
  return Buffer.concat(chunks);
}

/** Best-effort backup of one uploaded file to the configured GitHub repo via the Contents API -
 * a plain "create this file" PUT, since every upload gets a fresh generated path and so never
 * collides with (and never needs the current sha of) an existing file. Throws on failure; callers
 * treat that as non-fatal since the upload already succeeded to local disk. */
async function pushToGitHub(repoPath: string, content: Buffer): Promise<void> {
  const res = await fetch(`https://api.github.com/repos/${GITHUB_REPO}/contents/${repoPath}`, {
    method: "PUT",
    headers: {
      Authorization: `Bearer ${GITHUB_TOKEN}`,
      "Content-Type": "application/json",
      Accept: "application/vnd.github+json",
      "X-GitHub-Api-Version": "2022-11-28",
      "User-Agent": "helixnotes-collab-server",
    },
    body: JSON.stringify({
      message: `Add attachment ${repoPath}`,
      content: content.toString("base64"),
      branch: GITHUB_BRANCH,
    }),
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`GitHub API ${res.status}: ${text.slice(0, 300)}`);
  }
}

async function handleUpload(req: IncomingMessage, res: ServerResponse, workspaceRaw: string): Promise<void> {
  const password = req.headers["x-collab-password"];
  if (typeof password !== "string" || !passwordMatches(password)) {
    res.writeHead(401, { "content-type": "application/json" }).end(JSON.stringify({ error: "Invalid workspace password" }));
    return;
  }
  let workspace: string;
  try {
    workspace = decodeURIComponent(workspaceRaw);
  } catch {
    res.writeHead(400, { "content-type": "application/json" }).end(JSON.stringify({ error: "Invalid workspace id" }));
    return;
  }
  if (!isSafeWorkspaceSegment(workspace)) {
    res.writeHead(400, { "content-type": "application/json" }).end(JSON.stringify({ error: "Invalid workspace id" }));
    return;
  }

  const contentLength = Number(req.headers["content-length"] ?? NaN);
  if (Number.isFinite(contentLength) && contentLength > UPLOAD_MAX_BYTES) {
    res
      .writeHead(413, { "content-type": "application/json" })
      .end(JSON.stringify({ error: `File exceeds the ${UPLOAD_MAX_BYTES}-byte limit` }));
    req.destroy();
    return;
  }

  let body: Buffer;
  try {
    body = await readBodyWithLimit(req, UPLOAD_MAX_BYTES);
  } catch {
    res
      .writeHead(413, { "content-type": "application/json" })
      .end(JSON.stringify({ error: `File exceeds the ${UPLOAD_MAX_BYTES}-byte limit` }));
    return;
  }
  if (body.length === 0) {
    res.writeHead(400, { "content-type": "application/json" }).end(JSON.stringify({ error: "Empty upload" }));
    return;
  }

  const rawName = req.headers["x-file-name"];
  let originalName = "file";
  if (typeof rawName === "string" && rawName) {
    try {
      originalName = decodeURIComponent(rawName);
    } catch {
      originalName = rawName;
    }
  }

  const id = randomUUID();
  const storedName = `${id}-${sanitizeFilename(originalName)}`;
  const workspaceDir = join(UPLOADS_DIR, workspace);
  await mkdir(workspaceDir, { recursive: true });
  await writeFile(join(workspaceDir, storedName), body);

  let githubBackedUp = false;
  if (GITHUB_BACKUP_ENABLED) {
    try {
      await pushToGitHub(`attachments/${workspace}/${storedName}`, body);
      githubBackedUp = true;
    } catch (e) {
      console.error(`[collab] GitHub backup failed for ${storedName} (kept on local disk):`, e);
    }
  }

  console.log(
    `[collab] upload: workspace="${workspace}" file="${storedName}" size=${body.length} github=${githubBackedUp}`,
  );
  res.writeHead(200, { "content-type": "application/json" }).end(
    JSON.stringify({
      id,
      url: `/uploads/${workspace}/${storedName}`,
      name: originalName,
      size: body.length,
      githubBackedUp,
    }),
  );
}

async function handleDownload(
  req: IncomingMessage,
  res: ServerResponse,
  workspaceRaw: string,
  filenameRaw: string,
  query: URLSearchParams,
): Promise<void> {
  const password = query.get("password") ?? "";
  if (!passwordMatches(password)) {
    res.writeHead(401, { "content-type": "text/plain" }).end("Invalid workspace password\n");
    return;
  }
  let workspace: string;
  let filename: string;
  try {
    workspace = decodeURIComponent(workspaceRaw);
    filename = decodeURIComponent(filenameRaw);
  } catch {
    res.writeHead(400, { "content-type": "text/plain" }).end("Invalid path\n");
    return;
  }
  if (!isSafeWorkspaceSegment(workspace) || !isSafeStoredFilename(filename)) {
    res.writeHead(400, { "content-type": "text/plain" }).end("Invalid path\n");
    return;
  }

  let data: Buffer;
  try {
    data = await readFile(join(UPLOADS_DIR, workspace, filename));
  } catch {
    res.writeHead(404, { "content-type": "text/plain" }).end("Not found\n");
    return;
  }
  res.writeHead(200, {
    "content-type": guessContentType(filename),
    "content-length": data.length,
    // Each stored filename is content-addressed by a fresh random id, never reused - safe to
    // cache forever.
    "cache-control": "private, max-age=31536000, immutable",
  });
  res.end(data);
}

// --- Stage 6: live document persistence ---
//
// See the module doc comment up top for the overview. This section only understands the same
// envelope src/lib/collab/syncProtocol.ts already defines client-side (message type 0/1/2 are
// sync step 1 / sync step 2 / update; type 3 is awareness - presence, not document content, and
// is never touched here) - just enough to keep a shadow Y.Doc in sync and reply to a sync step 1
// directly when nobody else is online to.
const SYNC_STEP1 = 0;
const SYNC_STEP2 = 1;
const SYNC_UPDATE = 2;

const DOC_SNAPSHOTS_DIR = process.env.DOC_SNAPSHOTS_DIR ?? "doc-snapshots";
const DOC_SAVE_DEBOUNCE_MS = Number(process.env.DOC_SAVE_DEBOUNCE_MS ?? 2_000);
const DOC_SAVE_MAX_DELAY_MS = Number(process.env.DOC_SAVE_MAX_DELAY_MS ?? 15_000);
const DOC_GITHUB_SAVE_MIN_INTERVAL_MS = Number(process.env.DOC_GITHUB_SAVE_MIN_INTERVAL_MS ?? 60_000);

/** One shadow Y.Doc per workspace, held for the life of the process once loaded. */
const docs = new Map<string, Y.Doc>();
const docLoadPromises = new Map<string, Promise<Y.Doc>>();
const dirtySince = new Map<string, number>();
const saveTimers = new Map<string, NodeJS.Timeout>();
const lastGithubPush = new Map<string, number>();

function docSnapshotPath(workspace: string): string {
  return join(DOC_SNAPSHOTS_DIR, `${workspace}.ydoc`);
}

function githubApiHeaders(): Record<string, string> {
  return {
    Authorization: `Bearer ${GITHUB_TOKEN}`,
    Accept: "application/vnd.github+json",
    "X-GitHub-Api-Version": "2022-11-28",
    "User-Agent": "helixnotes-collab-server",
  };
}

/** Fetches the currently-saved snapshot for `workspace` from the GitHub backup repo, or `null`
 * if none exists there yet. Used both to hydrate a workspace on first join when local disk has
 * nothing (e.g. a fresh Render instance right after a redeploy) and, inside
 * pushSnapshotToGitHub, to find the sha an update has to reference. */
async function fetchSnapshotFromGitHub(workspace: string): Promise<{ content: Buffer; sha: string } | null> {
  const repoPath = `snapshots/${workspace}.ydoc`;
  const res = await fetch(
    `https://api.github.com/repos/${GITHUB_REPO}/contents/${repoPath}?ref=${encodeURIComponent(GITHUB_BRANCH)}`,
    { headers: githubApiHeaders() },
  );
  if (res.status === 404) return null;
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`GitHub API ${res.status}: ${text.slice(0, 300)}`);
  }
  const json = (await res.json()) as { content?: string; sha?: string };
  if (!json.content || !json.sha) return null;
  return { content: Buffer.from(json.content, "base64"), sha: json.sha };
}

/** Create-or-update the saved snapshot for `workspace` in the GitHub backup repo. Unlike an
 * upload (pushToGitHub above), a snapshot lives at the same path every time it's saved, so
 * updating it needs the existing file's sha - a plain create-only PUT would just fail once the
 * file already exists there. */
async function pushSnapshotToGitHub(workspace: string, content: Buffer): Promise<void> {
  const repoPath = `snapshots/${workspace}.ydoc`;
  const existing = await fetchSnapshotFromGitHub(workspace).catch(() => null);
  const res = await fetch(`https://api.github.com/repos/${GITHUB_REPO}/contents/${repoPath}`, {
    method: "PUT",
    headers: { ...githubApiHeaders(), "Content-Type": "application/json" },
    body: JSON.stringify({
      message: `Update live document snapshot for workspace "${workspace}"`,
      content: content.toString("base64"),
      branch: GITHUB_BRANCH,
      ...(existing ? { sha: existing.sha } : {}),
    }),
  });
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`GitHub API ${res.status}: ${text.slice(0, 300)}`);
  }
}

async function loadWorkspaceDoc(workspace: string): Promise<Y.Doc> {
  const doc = new Y.Doc();
  let snapshot: Buffer | null = null;
  try {
    snapshot = await readFile(docSnapshotPath(workspace));
  } catch {
    // No local snapshot - first time this workspace has been opened on this running instance,
    // or a fresh disk after a Render redeploy. Fall through to GitHub below.
  }
  if (!snapshot && GITHUB_BACKUP_ENABLED) {
    try {
      const remote = await fetchSnapshotFromGitHub(workspace);
      snapshot = remote?.content ?? null;
    } catch (e) {
      console.warn(
        `[collab] could not check GitHub for a saved snapshot of workspace "${workspace}" (starting empty):`,
        e,
      );
    }
  }
  if (snapshot && snapshot.length > 0) {
    try {
      Y.applyUpdate(doc, new Uint8Array(snapshot));
      console.log(`[collab] restored workspace "${workspace}" from a saved snapshot (${snapshot.length} bytes)`);
    } catch (e) {
      console.error(`[collab] saved snapshot for workspace "${workspace}" failed to apply (starting empty):`, e);
    }
  }
  return doc;
}

/** Returns the in-memory shadow doc for `workspace`, loading (and hydrating it from a saved
 * snapshot, local or GitHub) first if this is the first time anyone's joined it on this running
 * instance. Concurrent joins of the same brand-new workspace share one load instead of racing. */
function ensureWorkspaceDocLoaded(workspace: string): Promise<Y.Doc> {
  const existing = docs.get(workspace);
  if (existing) return Promise.resolve(existing);
  let pending = docLoadPromises.get(workspace);
  if (!pending) {
    pending = loadWorkspaceDoc(workspace).then((doc) => {
      docs.set(workspace, doc);
      docLoadPromises.delete(workspace);
      return doc;
    });
    docLoadPromises.set(workspace, pending);
  }
  return pending;
}

/** Saves `workspace`'s current shadow-doc state to local disk, and - if GITHUB_BACKUP_ENABLED -
 * also to the GitHub backup repo, throttled to at most once per DOC_GITHUB_SAVE_MIN_INTERVAL_MS
 * unless `force` is set (used when the last peer leaves a workspace, and on shutdown, so a clean
 * departure doesn't have to wait out the throttle window before it's actually safe). */
async function flushWorkspace(workspace: string, opts: { force?: boolean } = {}): Promise<void> {
  const timer = saveTimers.get(workspace);
  if (timer) {
    clearTimeout(timer);
    saveTimers.delete(workspace);
  }
  dirtySince.delete(workspace);
  const doc = docs.get(workspace);
  if (!doc) return;

  const snapshot = Buffer.from(Y.encodeStateAsUpdate(doc));
  await mkdir(DOC_SNAPSHOTS_DIR, { recursive: true });
  await writeFile(docSnapshotPath(workspace), snapshot);

  if (GITHUB_BACKUP_ENABLED) {
    const last = lastGithubPush.get(workspace) ?? 0;
    if (opts.force || Date.now() - last >= DOC_GITHUB_SAVE_MIN_INTERVAL_MS) {
      lastGithubPush.set(workspace, Date.now());
      try {
        await pushSnapshotToGitHub(workspace, snapshot);
      } catch (e) {
        console.error(`[collab] GitHub snapshot backup failed for workspace "${workspace}" (kept on local disk):`, e);
      }
    }
  }
}

/** Debounces a save after a change: waits for DOC_SAVE_DEBOUNCE_MS of quiet, but never longer
 * than DOC_SAVE_MAX_DELAY_MS after the first unsaved change, so a workspace under continuous
 * editing (where a naive debounce timer would just keep getting reset and never fire) still gets
 * saved periodically instead of only once editing finally pauses. */
function scheduleSave(workspace: string): void {
  if (!dirtySince.has(workspace)) dirtySince.set(workspace, Date.now());
  const existingTimer = saveTimers.get(workspace);
  if (existingTimer) clearTimeout(existingTimer);
  const elapsed = Date.now() - dirtySince.get(workspace)!;
  const delay = elapsed >= DOC_SAVE_MAX_DELAY_MS ? 0 : DOC_SAVE_DEBOUNCE_MS;
  saveTimers.set(
    workspace,
    setTimeout(() => {
      saveTimers.delete(workspace);
      flushWorkspace(workspace).catch((e) => console.error(`[collab] failed to save workspace "${workspace}":`, e));
    }, delay),
  );
}

/** `ws` hands binary messages back as a Buffer by default (this server sets no streaming/
 * fragmentation options) - ArrayBuffer/Buffer[] are handled too, defensively. */
function toUint8Array(data: RawData): Uint8Array {
  if (Buffer.isBuffer(data)) return new Uint8Array(data.buffer, data.byteOffset, data.length);
  if (Array.isArray(data)) return new Uint8Array(Buffer.concat(data));
  return new Uint8Array(data as ArrayBuffer);
}

/** Applies one incoming binary frame to `workspace`'s shadow doc for persistence, and - for a
 * sync step 1 - replies to the sender directly from that shadow doc. This second part is what
 * makes a lone client (nobody else currently online in the workspace) actually get their content
 * back instead of starting blank: the broadcastToWorkspace() call the caller already made right
 * before this only reaches other *currently connected* peers, which is nobody the moment you're
 * the only one there. The caller wraps this in try/catch - a malformed or unrecognized frame here
 * must never break the raw relay above it. */
function handleDocMessage(workspace: string, sender: CollabSocket, bytes: Uint8Array): void {
  const doc = docs.get(workspace);
  if (!doc) return;
  const decoder = decoding.createDecoder(bytes);
  const messageType = decoding.readVarUint(decoder);
  switch (messageType) {
    case SYNC_STEP1: {
      const remoteStateVector = decoding.readVarUint8Array(decoder);
      const diff = Y.encodeStateAsUpdate(doc, remoteStateVector);
      if (diff.length > 0 && sender.readyState === WebSocket.OPEN) {
        const encoder = encoding.createEncoder();
        encoding.writeVarUint(encoder, SYNC_STEP2);
        encoding.writeVarUint8Array(encoder, diff);
        sender.send(encoding.toUint8Array(encoder), { binary: true });
      }
      break;
    }
    case SYNC_STEP2:
    case SYNC_UPDATE: {
      const update = decoding.readVarUint8Array(decoder);
      Y.applyUpdate(doc, update);
      scheduleSave(workspace);
      break;
    }
    default:
      // Awareness (presence/cursors) or an unrecognized future type - not document content,
      // nothing to persist.
      break;
  }
}

function requestHandler(req: IncomingMessage, res: ServerResponse) {
  const url = new URL(req.url ?? "/", `http://${req.headers.host ?? "localhost"}`);

  if (url.pathname === "/healthz" || url.pathname === "/") {
    res.writeHead(200, { "content-type": "text/plain" });
    res.end("helixnotes-collab-server: ok\n");
    return;
  }

  const uploadMatch = req.method === "POST" && url.pathname.match(/^\/upload\/([^/]+)\/?$/);
  if (uploadMatch) {
    handleUpload(req, res, uploadMatch[1]).catch((e) => {
      console.error("[collab] unhandled upload error:", e);
      if (!res.headersSent) res.writeHead(500, { "content-type": "application/json" }).end(JSON.stringify({ error: "Internal error" }));
    });
    return;
  }

  const downloadMatch = req.method === "GET" && url.pathname.match(/^\/uploads\/([^/]+)\/([^/]+)\/?$/);
  if (downloadMatch) {
    handleDownload(req, res, downloadMatch[1], downloadMatch[2], url.searchParams).catch((e) => {
      console.error("[collab] unhandled download error:", e);
      if (!res.headersSent) res.writeHead(500, { "content-type": "text/plain" }).end("Internal error\n");
    });
    return;
  }

  res.writeHead(404, { "content-type": "text/plain" });
  res.end("not found\n");
}

const httpServer = createServer(requestHandler);
const wss = new WebSocketServer({ server: httpServer });

/** `ws` sockets carry no liveness flag of their own; the standard heartbeat pattern stashes one. */
type CollabSocket = WebSocket & { isAlive?: boolean };

interface ConnectionState {
  authenticated: boolean;
  workspace: string | null;
}

/** Every authenticated socket, grouped by workspace, so a binary frame from one client can be
 * broadcast to exactly its peers and nobody else's workspace. Populated on successful auth,
 * cleaned up on close/error so a dead or never-authenticated socket never lingers in a set. */
const workspaces = new Map<string, Set<CollabSocket>>();

function joinWorkspace(workspace: string, socket: CollabSocket) {
  let members = workspaces.get(workspace);
  if (!members) {
    members = new Set();
    workspaces.set(workspace, members);
  }
  members.add(socket);
}

function leaveWorkspace(workspace: string | null, socket: CollabSocket) {
  if (!workspace) return;
  const members = workspaces.get(workspace);
  if (!members) return;
  members.delete(socket);
  if (members.size === 0) {
    workspaces.delete(workspace);
    // Nobody's left to keep this workspace's document moving via live edits - get whatever's
    // unsaved onto disk (and GitHub) now rather than waiting out the normal debounce/throttle.
    flushWorkspace(workspace, { force: true }).catch((e) =>
      console.error(`[collab] failed to flush workspace "${workspace}" after the last peer left:`, e),
    );
  }
}

/** Broadcast a binary frame to every other authenticated client in `workspace`. */
function broadcastToWorkspace(workspace: string, sender: CollabSocket, data: RawData) {
  const members = workspaces.get(workspace);
  if (!members) return;
  for (const member of members) {
    if (member === sender) continue;
    if (member.readyState !== WebSocket.OPEN) continue;
    member.send(data, { binary: true });
  }
}

wss.on("connection", (socket: CollabSocket, req: IncomingMessage) => {
  const remote = req.socket.remoteAddress ?? "unknown";
  const state: ConnectionState = { authenticated: false, workspace: null };
  socket.isAlive = true;

  const authTimer = setTimeout(() => {
    if (!state.authenticated) {
      socket.close(4001, "Authentication timed out");
    }
  }, AUTH_TIMEOUT_MS);

  socket.on("pong", () => {
    socket.isAlive = true;
  });

  socket.on("message", (data: RawData, isBinary: boolean) => {
    if (!state.authenticated) {
      let parsed: unknown;
      try {
        parsed = JSON.parse(isBinary ? "" : data.toString("utf8"));
      } catch {
        socket.close(4002, "First message must be JSON auth");
        return;
      }
      if (!isAuthMessage(parsed) || !parsed.workspace.trim() || !passwordMatches(parsed.password)) {
        console.warn(`[collab] auth failed from ${remote}`);
        socket.close(4001, "Invalid workspace or password");
        return;
      }
      const workspace = parsed.workspace;
      clearTimeout(authTimer);
      // Load (or hydrate from a saved snapshot) this workspace's shadow document before telling
      // the client they're connected, so it's ready the moment their first sync step 1 arrives.
      ensureWorkspaceDocLoaded(workspace)
        .then(() => {
          if (socket.readyState !== WebSocket.OPEN) return; // client gave up while we were loading
          state.authenticated = true;
          state.workspace = workspace;
          joinWorkspace(workspace, socket);
          console.log(`[collab] ${remote} authenticated for workspace "${workspace}"`);
          socket.send(JSON.stringify({ type: "connected" }));
        })
        .catch((e) => {
          console.error(`[collab] failed to load workspace "${workspace}":`, e);
          socket.close(1011, "Failed to load workspace state");
        });
      return;
    }

    if (isBinary) {
      // Stage 3: relay Yjs sync/update frames to this client's workspace peers - still a dumb,
      // content-agnostic broadcast, unchanged.
      broadcastToWorkspace(state.workspace!, socket, data);
      // Stage 6: also feed the same frame to this workspace's shadow doc for persistence (and
      // reply directly if it's a sync step 1 - see handleDocMessage's doc comment for why).
      try {
        handleDocMessage(state.workspace!, socket, toUint8Array(data));
      } catch (e) {
        console.error(`[collab] failed to process a sync message for workspace "${state.workspace}":`, e);
      }
    } else {
      // Stage 2's original debug/proof-of-transport behavior: echo text frames to the sender.
      socket.send(data, { binary: false });
    }
  });

  socket.on("close", (code, reason) => {
    clearTimeout(authTimer);
    leaveWorkspace(state.workspace, socket);
    console.log(`[collab] ${remote} disconnected (workspace: ${state.workspace ?? "n/a"}, code: ${code}, reason: ${reason.toString() || "none"})`);
  });

  socket.on("error", (err) => {
    console.error(`[collab] socket error from ${remote}:`, err);
  });
});

// Heartbeat: ping every connection periodically; any socket that did not pong back since the
// previous sweep is presumed dead and terminated. This surfaces a real drop for the Rust client's
// reconnect logic to react to, instead of a connection lingering open-but-unresponsive. Note this
// relies on the "close" handler above (fired by `terminate()`) to remove the socket from its
// workspace set - there is no separate cleanup path here.
const heartbeat = setInterval(() => {
  wss.clients.forEach((client) => {
    const socket = client as CollabSocket;
    if (socket.isAlive === false) {
      socket.terminate();
      return;
    }
    socket.isAlive = false;
    socket.ping();
  });
}, HEARTBEAT_INTERVAL_MS);

wss.on("close", () => clearInterval(heartbeat));

httpServer.listen(PORT, () => {
  console.log(`[collab] listening on :${PORT}`);
});

for (const signal of ["SIGINT", "SIGTERM"] as const) {
  process.on(signal, () => {
    console.log(`[collab] received ${signal}, shutting down`);
    clearInterval(heartbeat);
    // Best-effort: save every workspace's current state before closing, so a restart or redeploy
    // doesn't lose whatever hadn't hit the debounced save yet. Bounded by the force-exit fallback
    // below either way, so this can't hang the shutdown indefinitely.
    const flushes = Array.from(docs.keys()).map((workspace) =>
      flushWorkspace(workspace, { force: true }).catch((e) =>
        console.error(`[collab] failed to flush workspace "${workspace}" on shutdown:`, e),
      ),
    );
    Promise.allSettled(flushes).finally(() => {
      wss.close(() => httpServer.close(() => process.exit(0)));
    });
    // Force-exit if graceful shutdown (including the flush above) hangs.
    setTimeout(() => process.exit(0), 8000).unref();
  });
}
