// Verifies the Stage 5 upload/download HTTP routes end-to-end against a real running instance:
// password gating on both routes, the size cap enforced against actual bytes received (not just
// a client-supplied Content-Length), and that a stored file is scoped under its own workspace.
// Spawns `dist/server.js` itself so `npm test` is self-contained; run `npm run build` first if
// `dist/` is stale or missing.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const SERVER_ENTRY = join(__dirname, "..", "dist", "server.js");
const PORT = 8798; // distinct from relay.test.mjs's 8799 and the default 8787
const PASSWORD = "test-upload-password";
const BASE = `http://127.0.0.1:${PORT}`;

function waitForHealthz(timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const attempt = () => {
      fetch(`${BASE}/healthz`)
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

function spawnServer(t, extraEnv = {}) {
  const server = spawn(process.execPath, [SERVER_ENTRY], {
    env: { ...process.env, PORT: String(PORT), COLLAB_PASSWORD: PASSWORD, ...extraEnv },
    stdio: "pipe",
  });
  t.after(() => server.kill());
  return server;
}

test("uploads: round-trip, password gating, workspace scoping", async (t) => {
  spawnServer(t);
  await waitForHealthz(5000);

  const body = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]);

  // Wrong password on upload is rejected before anything is written.
  const badAuth = await fetch(`${BASE}/upload/team-a`, {
    method: "POST",
    headers: { "X-Collab-Password": "nope", "X-File-Name": "test.bin" },
    body,
  });
  assert.equal(badAuth.status, 401, "wrong password on upload is rejected");

  // A real upload succeeds and reports the right size.
  const uploadRes = await fetch(`${BASE}/upload/team-a`, {
    method: "POST",
    headers: {
      "X-Collab-Password": PASSWORD,
      "X-File-Name": encodeURIComponent("my file.bin"),
      "Content-Type": "application/octet-stream",
    },
    body,
  });
  assert.equal(uploadRes.status, 200, "upload with the right password succeeds");
  const uploaded = await uploadRes.json();
  assert.equal(uploaded.size, body.length, "reported size matches the bytes sent");
  assert.equal(uploaded.name, "my file.bin", "original filename round-trips through the header");
  assert.match(uploaded.url, /^\/uploads\/team-a\//, "the returned url is scoped under its workspace");
  assert.equal(uploaded.githubBackedUp, false, "no GITHUB_TOKEN configured, so no backup happened");

  // Downloading without the password is rejected.
  const noPassword = await fetch(`${BASE}${uploaded.url}`);
  assert.equal(noPassword.status, 401, "download without a password is rejected");

  // Downloading with the right password returns the original bytes.
  const download = await fetch(`${BASE}${uploaded.url}?password=${encodeURIComponent(PASSWORD)}`);
  assert.equal(download.status, 200, "download with the right password succeeds");
  const downloaded = new Uint8Array(await download.arrayBuffer());
  assert.deepEqual([...downloaded], [...body], "downloaded bytes match what was uploaded");

  // A different workspace's password-correct request for the same stored filename 404s - the
  // stored path is namespaced by workspace, not just gated by the shared password.
  const storedName = uploaded.url.split("/").pop();
  const wrongWorkspace = await fetch(`${BASE}/uploads/team-b/${storedName}?password=${encodeURIComponent(PASSWORD)}`);
  assert.equal(wrongWorkspace.status, 404, "a file isn't reachable under a different workspace");
});

test("uploads: oversized upload is rejected against actual bytes, not just Content-Length", async (t) => {
  // A tiny limit so the test doesn't need to push tens of megabytes to exercise the cap.
  spawnServer(t, { UPLOAD_MAX_BYTES: "10" });
  await waitForHealthz(5000);

  const tooBig = new Uint8Array(11).fill(9);
  const res = await fetch(`${BASE}/upload/team-a`, {
    method: "POST",
    headers: { "X-Collab-Password": PASSWORD, "X-File-Name": "big.bin" },
    body: tooBig,
  });
  assert.equal(res.status, 413, "a file over the configured limit is rejected");

  const withinLimit = new Uint8Array(10).fill(9);
  const ok = await fetch(`${BASE}/upload/team-a`, {
    method: "POST",
    headers: { "X-Collab-Password": PASSWORD, "X-File-Name": "ok.bin" },
    body: withinLimit,
  });
  assert.equal(ok.status, 200, "a file exactly at the limit is accepted");
});
