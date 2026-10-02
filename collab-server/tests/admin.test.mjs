// Verifies the admin panel (ADMIN_PASSWORD-gated workspace management, see the module doc
// comment's "Admin panel" section in src/server.ts): the panel is completely invisible (404, not
// 401) when ADMIN_PASSWORD isn't set; HTTP Basic Auth gates every /admin* route when it is set
// (any username, only the password is checked); and a delete actually wipes a workspace - its
// connected clients are disconnected, its on-disk snapshot/auth/uploads are removed, and the
// workspace is left fully unclaimed so a fresh connect can claim a brand new password, the same
// as a workspace that had simply never existed. Spawns `dist/server.js` itself so `npm test` is
// self-contained; run `npm run build` first if `dist/` is stale or missing.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { mkdtemp, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { WebSocket } from "ws";
import * as encoding from "lib0/encoding";
import * as Y from "yjs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const SERVER_ENTRY = join(__dirname, "..", "dist", "server.js");
// Distinct from every port already claimed by the other test files (8799/8798/8797/8796/8795/
// 8794/8793/8792) - each test below spawns its own server, and reusing a port risks a race
// between one test's shutdown and the next test's bind.
const DISABLED_PORT = 8791;
const AUTH_GATE_PORT = 8790;
const DELETE_PORT = 8789;
const PASSWORD = "test-admin-server-password";
const ADMIN_PASSWORD = "test-admin-panel-secret";
const SYNC_UPDATE = 2;

function waitForHealthz(port, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const attempt = () => {
      fetch(`http://127.0.0.1:${port}/healthz`)
        .then((res) => (res.ok ? resolve() : retry()))
        .catch(retry);
    };
    const retry = () => {
      if (Date.now() > deadline) {
        reject(new Error("server did not become healthy in time"));
        return;
      }
      setTimeout(attempt, 50);
    };
    attempt();
  });
}

function spawnServer(t, port, extraEnv = {}) {
  const server = spawn(process.execPath, [SERVER_ENTRY], {
    env: { ...process.env, PORT: String(port), COLLAB_PASSWORD: PASSWORD, ...extraEnv },
    stdio: "pipe",
  });
  // Wait for actual process exit, not just the kill signal - a later test reusing the same port
  // could otherwise race the OS releasing it.
  t.after(
    () =>
      new Promise((resolve) => {
        if (server.exitCode !== null || server.signalCode !== null) {
          resolve();
          return;
        }
        server.once("exit", resolve);
        server.kill();
      }),
  );
  return server;
}

function basicAuthHeader(username, password) {
  return `Basic ${Buffer.from(`${username}:${password}`, "utf8").toString("base64")}`;
}

/** Authenticates into `workspace` with the server-wide password plus an optional
 * `workspacePassword`, resolving once the server confirms the connection (or rejecting on a
 * socket error / a close before that point, since every call site here expects success). */
function connectAndAuth(port, workspace, workspacePassword) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`ws://127.0.0.1:${port}`);
    ws.once("error", reject);
    ws.once("close", (code, reasonBuf) =>
      reject(new Error(`socket closed before authenticating (code ${code}, reason: ${reasonBuf.toString()})`)),
    );
    ws.once("message", (data) => {
      const msg = JSON.parse(data.toString("utf8"));
      if (msg.type === "connected") {
        ws.off("close", reject);
        resolve(ws);
      } else {
        reject(new Error(`unexpected first message: ${data}`));
      }
    });
    ws.once("open", () => {
      const payload = { type: "auth", workspace, password: PASSWORD };
      if (workspacePassword !== undefined) payload.workspacePassword = workspacePassword;
      ws.send(JSON.stringify(payload));
    });
  });
}

function sendUpdate(ws, update) {
  const encoder = encoding.createEncoder();
  encoding.writeVarUint(encoder, SYNC_UPDATE);
  encoding.writeVarUint8Array(encoder, update);
  ws.send(encoding.toUint8Array(encoder), { binary: true });
}

/** Waits for `ws` to close, resolving with the close code/reason instead of rejecting - used here
 * specifically to observe the admin delete forcing a connected client off, which is the behavior
 * under test rather than an error. */
function waitForClose(ws) {
  return new Promise((resolve) => {
    ws.once("close", (code, reasonBuf) => resolve({ code, reason: reasonBuf.toString() }));
  });
}

test("admin panel: disabled entirely (404, not 401) when ADMIN_PASSWORD isn't set", async (t) => {
  spawnServer(t, DISABLED_PORT); // no ADMIN_PASSWORD in extraEnv
  await waitForHealthz(DISABLED_PORT, 5000);
  const base = `http://127.0.0.1:${DISABLED_PORT}`;

  const getRes = await fetch(`${base}/admin`);
  assert.equal(getRes.status, 404, "GET /admin 404s, not 401s, when the panel is off");

  const postRes = await fetch(`${base}/admin/workspaces/whatever/delete`, { method: "POST" });
  assert.equal(postRes.status, 404, "the delete route 404s too, even with a well-formed workspace id");
});

test("admin panel: HTTP Basic Auth gates every route once ADMIN_PASSWORD is set", async (t) => {
  spawnServer(t, AUTH_GATE_PORT, { ADMIN_PASSWORD });
  await waitForHealthz(AUTH_GATE_PORT, 5000);
  const base = `http://127.0.0.1:${AUTH_GATE_PORT}`;

  const noAuth = await fetch(`${base}/admin`);
  assert.equal(noAuth.status, 401, "no Authorization header at all is rejected");

  const wrongPassword = await fetch(`${base}/admin`, { headers: { authorization: basicAuthHeader("admin", "nope") } });
  assert.equal(wrongPassword.status, 401, "the wrong password is rejected");

  // The username is ignored entirely - only the password is checked.
  const rightPassword = await fetch(`${base}/admin`, { headers: { authorization: basicAuthHeader("anyone-at-all", ADMIN_PASSWORD) } });
  assert.equal(rightPassword.status, 200, "the right password succeeds regardless of username");
  assert.match(await rightPassword.text(), /HelixNotes Collaboration - Admin/, "it's actually the admin page");

  const deleteNoAuth = await fetch(`${base}/admin/workspaces/some-workspace/delete`, { method: "POST" });
  assert.equal(deleteNoAuth.status, 401, "the delete route enforces the same auth");
});

test("admin panel: deleting a workspace disconnects it, wipes its files, and leaves it fully unclaimed", async (t) => {
  const snapshotsDir = await mkdtemp(join(tmpdir(), "helixnotes-collab-admin-snapshots-"));
  const authDir = await mkdtemp(join(tmpdir(), "helixnotes-collab-admin-auth-"));
  const uploadsDir = await mkdtemp(join(tmpdir(), "helixnotes-collab-admin-uploads-"));
  t.after(() => Promise.all([snapshotsDir, authDir, uploadsDir].map((d) => rm(d, { recursive: true, force: true }))));

  spawnServer(t, DELETE_PORT, {
    ADMIN_PASSWORD,
    DOC_SNAPSHOTS_DIR: snapshotsDir,
    WORKSPACE_AUTH_DIR: authDir,
    UPLOADS_DIR: uploadsDir,
    // The normal 2s debounce (src/server.ts's DOC_SAVE_DEBOUNCE_MS) would otherwise make this
    // test either slow or racy about whether the snapshot has actually hit disk yet before the
    // "exists before deletion" assertion below - a short debounce here keeps it fast and exact.
    DOC_SAVE_DEBOUNCE_MS: "50",
  });
  await waitForHealthz(DELETE_PORT, 5000);
  const base = `http://127.0.0.1:${DELETE_PORT}`;
  const authHeader = { authorization: basicAuthHeader("admin", ADMIN_PASSWORD) };
  const workspace = "admin-delete-target";
  const originalWorkspacePassword = "original-workspace-secret";

  // Claim the workspace with a password, write some content to it, and upload a file - so there's
  // a real auth record, snapshot, and upload on disk for delete to actually have to remove.
  const client = await connectAndAuth(DELETE_PORT, workspace, originalWorkspacePassword);
  const doc = new Y.Doc();
  doc.getText("note").insert(0, "about to be deleted");
  sendUpdate(client, Y.encodeStateAsUpdate(doc));
  await new Promise((r) => setTimeout(r, 300)); // apply + the 50ms debounce above, with margin

  const uploadRes = await fetch(`${base}/upload/${workspace}`, {
    method: "POST",
    headers: {
      "X-Collab-Password": PASSWORD,
      "X-Collab-Workspace-Password": originalWorkspacePassword,
      "X-File-Name": "note.txt",
    },
    body: new Uint8Array([1, 2, 3]),
  });
  assert.equal(uploadRes.status, 200, "setup: the upload used to seed this workspace succeeds");

  const snapshotPath = join(snapshotsDir, `${workspace}.ydoc`);
  const authPath = join(authDir, `${workspace}.json`);
  const workspaceUploadsPath = join(uploadsDir, workspace);
  await assert.doesNotReject(stat(snapshotPath), "setup: the snapshot file exists before deletion");
  await assert.doesNotReject(stat(authPath), "setup: the auth record exists before deletion");
  await assert.doesNotReject(stat(workspaceUploadsPath), "setup: the uploads directory exists before deletion");

  // The admin page lists it, with a live connection counted.
  const pageBeforeRes = await fetch(`${base}/admin`, { headers: authHeader });
  const pageBefore = await pageBeforeRes.text();
  assert.match(pageBefore, new RegExp(workspace), "the workspace shows up in the admin listing");

  const closed = waitForClose(client);

  const deleteRes = await fetch(`${base}/admin/workspaces/${workspace}/delete`, { method: "POST", headers: authHeader, redirect: "manual" });
  assert.equal(deleteRes.status, 303, "delete redirects back to the admin page");
  assert.match(deleteRes.headers.get("location") ?? "", new RegExp(`/admin\\?deleted=${workspace}`), "redirect names the deleted workspace");

  const { code } = await closed;
  assert.equal(code, 4004, "the connected client is disconnected as part of the delete");

  await assert.rejects(stat(snapshotPath), "the snapshot file is removed");
  await assert.rejects(stat(authPath), "the auth record is removed");
  await assert.rejects(stat(workspaceUploadsPath), "the uploads directory is removed");

  const pageAfterRes = await fetch(`${base}/admin`, { headers: authHeader });
  const pageAfter = await pageAfterRes.text();
  assert.doesNotMatch(pageAfter, new RegExp(workspace), "the workspace is gone from the admin listing");

  // Fully unclaimed: a fresh connect can claim an entirely different password, same as if the
  // workspace had never existed. (Before the delete, this same workspacePassword would have been
  // rejected, since `originalWorkspacePassword` had already claimed it.)
  const reclaimed = await connectAndAuth(DELETE_PORT, workspace, "a-brand-new-password");
  reclaimed.close();
});
