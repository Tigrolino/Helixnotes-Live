// Verifies Stage 7's per-workspace passwords, layered on top of the server-wide password tested
// elsewhere (relay.test.mjs, uploads.test.mjs): the first client to ever authenticate into a given
// workspace "claims" its password state (with one, or explicitly without one), that state is then
// fixed and enforced for later joiners, a workspace that started open can never be retroactively
// locked, and the claimed state survives a server restart the same way Stage 6's document snapshots
// do (see persistence.test.mjs). Spawns `dist/server.js` itself so `npm test` is self-contained;
// run `npm run build` first if `dist/` is stale or missing.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { WebSocket } from "ws";

const __dirname = dirname(fileURLToPath(import.meta.url));
const SERVER_ENTRY = join(__dirname, "..", "dist", "server.js");
// Distinct from every port already claimed by relay.test.mjs (8799), uploads.test.mjs (8798), and
// persistence.test.mjs (8797/8796) - each test below spawns its own server(s), and reusing a port
// across tests risks a race between one test's shutdown and the next test's bind.
const CLAIM_TEST_PORT = 8795;
const OPEN_STAYS_OPEN_PORT = 8794;
const HTTP_TEST_PORT = 8793;
const RESTART_TEST_PORT = 8792;
const PASSWORD = "test-workspace-password-server-secret";

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
  // Wait for actual process exit, not just the kill signal - a later spawnServer call reusing the
  // same port could otherwise race the OS releasing it.
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

/** Authenticates into `workspace` with the server-wide password plus an optional
 * `workspacePassword`, and resolves once the outcome is known - either a `{type:"connected"}`
 * message (ok: true) or the socket closing before that (ok: false, with the close code/reason) -
 * rather than throwing on the rejection path, since a wrong workspace password is an expected,
 * assertable outcome here, not a test-infrastructure failure. */
function connectWithAuth(port, workspace, workspacePassword) {
  return new Promise((resolve) => {
    const ws = new WebSocket(`ws://127.0.0.1:${port}`);
    let settled = false;
    ws.once("open", () => {
      const payload = { type: "auth", workspace, password: PASSWORD };
      if (workspacePassword !== undefined) payload.workspacePassword = workspacePassword;
      ws.send(JSON.stringify(payload));
    });
    ws.once("message", (data) => {
      const msg = JSON.parse(data.toString("utf8"));
      if (msg.type === "connected" && !settled) {
        settled = true;
        resolve({ ok: true, ws });
      }
    });
    ws.once("close", (code, reasonBuf) => {
      if (!settled) {
        settled = true;
        resolve({ ok: false, code, reason: reasonBuf ? reasonBuf.toString("utf8") : "" });
      }
    });
    ws.once("error", () => {
      // A connection-level error surfaces as a close event right after; let that settle the
      // promise instead of racing to reject here.
    });
  });
}

function closeAndWait(ws) {
  return new Promise((resolve) => {
    ws.once("close", resolve);
    ws.close();
  });
}

test("workspace password: first joiner claims it, later joiners need the right one", async (t) => {
  const port = CLAIM_TEST_PORT;
  const authDir = await mkdtemp(join(tmpdir(), "helixnotes-collab-wspw-claim-"));
  t.after(() => rm(authDir, { recursive: true, force: true }));

  spawnServer(t, port, { WORKSPACE_AUTH_DIR: authDir });
  await waitForHealthz(port, 5000);

  const workspace = "secure-project";

  const first = await connectWithAuth(port, workspace, "letmein");
  assert.equal(first.ok, true, "the first-ever joiner claims the workspace password and gets in");
  await closeAndWait(first.ws);

  const wrong = await connectWithAuth(port, workspace, "nope");
  assert.equal(wrong.ok, false, "a later joiner with the wrong workspace password is rejected");
  assert.equal(wrong.code, 4001, "rejection closes with the dedicated invalid-password code");

  const right = await connectWithAuth(port, workspace, "letmein");
  assert.equal(right.ok, true, "a later joiner with the correct workspace password gets in");
  await closeAndWait(right.ws);
});

test("workspace password: a workspace that starts open stays open, even if someone later supplies a password", async (t) => {
  const port = OPEN_STAYS_OPEN_PORT;
  const authDir = await mkdtemp(join(tmpdir(), "helixnotes-collab-wspw-open-"));
  t.after(() => rm(authDir, { recursive: true, force: true }));

  spawnServer(t, port, { WORKSPACE_AUTH_DIR: authDir });
  await waitForHealthz(port, 5000);

  const workspace = "open-project";

  const first = await connectWithAuth(port, workspace, undefined);
  assert.equal(first.ok, true, "the first-ever joiner with no workspace password claims it as open");
  await closeAndWait(first.ws);

  const triedToLock = await connectWithAuth(port, workspace, "trying-to-lock-it-after-the-fact");
  assert.equal(
    triedToLock.ok,
    true,
    "a workspace that started open can't be retroactively locked by a later client supplying a password",
  );
  await closeAndWait(triedToLock.ws);

  const stillOpen = await connectWithAuth(port, workspace, undefined);
  assert.equal(stillOpen.ok, true, "the workspace is still open for a joiner who also supplies nothing");
  await closeAndWait(stillOpen.ws);
});

test("workspace password: the upload and download HTTP routes enforce it too", async (t) => {
  const port = HTTP_TEST_PORT;
  const authDir = await mkdtemp(join(tmpdir(), "helixnotes-collab-wspw-http-"));
  t.after(() => rm(authDir, { recursive: true, force: true }));

  spawnServer(t, port, { WORKSPACE_AUTH_DIR: authDir });
  await waitForHealthz(port, 5000);

  const BASE = `http://127.0.0.1:${port}`;
  const workspace = "http-secure";
  const body = new Uint8Array([9, 8, 7, 6, 5]);

  // The first upload into this workspace claims its password, same as a WS first-join would.
  const uploadRes = await fetch(`${BASE}/upload/${workspace}`, {
    method: "POST",
    headers: {
      "X-Collab-Password": PASSWORD,
      "X-Collab-Workspace-Password": "secret-upload",
      "X-File-Name": "ok.bin",
    },
    body,
  });
  assert.equal(uploadRes.status, 200, "an upload claiming a new workspace password succeeds");
  const uploaded = await uploadRes.json();

  const wrongUpload = await fetch(`${BASE}/upload/${workspace}`, {
    method: "POST",
    headers: {
      "X-Collab-Password": PASSWORD,
      "X-Collab-Workspace-Password": "wrong-one",
      "X-File-Name": "nope.bin",
    },
    body,
  });
  assert.equal(wrongUpload.status, 401, "a later upload with the wrong workspace password is rejected");

  const downloadNoWorkspacePassword = await fetch(`${BASE}${uploaded.url}?password=${encodeURIComponent(PASSWORD)}`);
  assert.equal(
    downloadNoWorkspacePassword.status,
    401,
    "downloading with the server password but no workspace password is rejected once one is set",
  );

  const downloadWrongWorkspacePassword = await fetch(
    `${BASE}${uploaded.url}?password=${encodeURIComponent(PASSWORD)}&workspacePassword=${encodeURIComponent("wrong-one")}`,
  );
  assert.equal(downloadWrongWorkspacePassword.status, 401, "downloading with the wrong workspace password is rejected");

  const download = await fetch(
    `${BASE}${uploaded.url}?password=${encodeURIComponent(PASSWORD)}&workspacePassword=${encodeURIComponent("secret-upload")}`,
  );
  assert.equal(download.status, 200, "downloading with the correct workspace password succeeds");
  const downloaded = new Uint8Array(await download.arrayBuffer());
  assert.deepEqual([...downloaded], [...body], "downloaded bytes match what was uploaded");
});

test("workspace password: a claimed password survives a full server restart", async (t) => {
  const port = RESTART_TEST_PORT;
  const authDir = await mkdtemp(join(tmpdir(), "helixnotes-collab-wspw-restart-"));
  t.after(() => rm(authDir, { recursive: true, force: true }));

  const first = spawnServer(t, port, { WORKSPACE_AUTH_DIR: authDir });
  await waitForHealthz(port, 5000);

  const workspace = "restart-ws";
  const claim = await connectWithAuth(port, workspace, "restart-secret");
  assert.equal(claim.ok, true, "the workspace password is claimed before the restart");
  await closeAndWait(claim.ws);

  first.kill();
  await new Promise((resolve) => first.once("exit", resolve));

  spawnServer(t, port, { WORKSPACE_AUTH_DIR: authDir });
  await waitForHealthz(port, 5000);

  const wrongAfterRestart = await connectWithAuth(port, workspace, "wrong-guess");
  assert.equal(wrongAfterRestart.ok, false, "the wrong password is still rejected after a restart");
  assert.equal(wrongAfterRestart.code, 4001);

  const rightAfterRestart = await connectWithAuth(port, workspace, "restart-secret");
  assert.equal(rightAfterRestart.ok, true, "the correct claimed password still works after a restart");
  await closeAndWait(rightAfterRestart.ws);
});
