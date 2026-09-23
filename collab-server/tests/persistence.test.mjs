// Verifies Stage 6's live-document persistence: a workspace's Yjs document survives (a) everyone
// disconnecting and a fresh client joining later with nobody else online to sync from, and (b) a
// full server process restart (a stand-in for a Render redeploy, minus the GitHub round-trip,
// which needs real credentials and isn't exercised here - see uploads.test.mjs's githubBackedUp
// assertion for the same boundary on that feature).
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { WebSocket } from "ws";
import * as Y from "yjs";
import * as encoding from "lib0/encoding";
import * as decoding from "lib0/decoding";

const __dirname = dirname(fileURLToPath(import.meta.url));
const SERVER_ENTRY = join(__dirname, "..", "dist", "server.js");
// Distinct from relay.test.mjs's 8799 and uploads.test.mjs's 8798, AND from each other - the two
// tests below each spawn their own server(s), and reusing one port across tests risks a race
// between one test's server process finishing shutdown and the next test's server binding it.
const LONE_TEST_PORT = 8797;
const RESTART_TEST_PORT = 8796;
const PASSWORD = "test-persistence-password";

// Mirrors src/lib/collab/syncProtocol.ts's envelope (message type 0/1/2) - reimplemented here,
// same as that module's own doc comment explains doing client-side, so this test doesn't need to
// pull in the SvelteKit app just to speak the wire format.
const SYNC_STEP1 = 0;
const SYNC_STEP2 = 1;
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
  // Wait for the process to actually exit, not just for the kill signal to be sent - the next
  // spawnServer call in the same test (or the next test, sharing nothing but Node's own
  // sequencing of after-hooks) may reuse the same port, and a synchronous kill() returns well
  // before the OS has released it.
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

function connectAndAuth(port, workspace) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`ws://127.0.0.1:${port}`);
    ws.once("error", reject);
    ws.once("message", (data) => {
      const msg = JSON.parse(data.toString("utf8"));
      if (msg.type === "connected") {
        ws.off("error", reject);
        resolve(ws);
      } else {
        reject(new Error(`unexpected first message: ${data}`));
      }
    });
    ws.once("open", () => {
      ws.send(JSON.stringify({ type: "auth", workspace, password: PASSWORD }));
    });
  });
}

function sendUpdate(ws, update) {
  const encoder = encoding.createEncoder();
  encoding.writeVarUint(encoder, SYNC_UPDATE);
  encoding.writeVarUint8Array(encoder, update);
  ws.send(encoding.toUint8Array(encoder), { binary: true });
}

/** Sends a sync step 1 (our state vector - empty, since we're a fresh doc) and waits for the
 * server's direct sync step 2 reply (handleDocMessage in src/server.ts), returning the doc it
 * was applied to. This is the actual behavior under test: the server replies from its own shadow
 * doc, not by relaying something from another peer (there may be none online). */
function syncFreshDoc(ws) {
  return new Promise((resolve, reject) => {
    const doc = new Y.Doc();
    const timeout = setTimeout(() => reject(new Error("timed out waiting for sync step 2")), 3000);
    ws.on("message", function onMessage(data, isBinary) {
      if (!isBinary) return; // ignore JSON frames
      const decoder = decoding.createDecoder(new Uint8Array(data));
      const type = decoding.readVarUint(decoder);
      if (type !== SYNC_STEP2) return;
      const diff = decoding.readVarUint8Array(decoder);
      Y.applyUpdate(doc, diff);
      clearTimeout(timeout);
      ws.off("message", onMessage);
      resolve(doc);
    });
    const encoder = encoding.createEncoder();
    encoding.writeVarUint(encoder, SYNC_STEP1);
    encoding.writeVarUint8Array(encoder, Y.encodeStateVector(doc));
    ws.send(encoding.toUint8Array(encoder), { binary: true });
  });
}

function closeAndWait(ws) {
  return new Promise((resolve) => {
    ws.once("close", resolve);
    ws.close();
  });
}

test("persistence: a lone reconnecting client is caught up from the server's saved state, not left blank", async (t) => {
  const port = LONE_TEST_PORT;
  // Isolated per test run - a shared default directory would let an old run's leftover snapshot
  // for this same workspace name bleed into a fresh run (Yjs merges two independently-built
  // docs by concatenating, since they share no history - it looks like corruption, but it's
  // really just stale test state, not a persistence bug).
  const snapshotsDir = await mkdtemp(join(tmpdir(), "helixnotes-collab-persist-lone-"));
  t.after(() => rm(snapshotsDir, { recursive: true, force: true }));

  spawnServer(t, port, { DOC_SNAPSHOTS_DIR: snapshotsDir });
  await waitForHealthz(port, 5000);

  const workspace = "persist-lone";
  const writer = await connectAndAuth(port, workspace);
  const writerDoc = new Y.Doc();
  writerDoc.getText("note").insert(0, "hello from writer");
  sendUpdate(writer, Y.encodeStateAsUpdate(writerDoc));
  // Give the server a moment to apply the update to its shadow doc before disconnecting.
  await new Promise((r) => setTimeout(r, 200));
  await closeAndWait(writer);

  // Nobody else is online in this workspace right now - a plain relay would hand a joiner
  // nothing to sync from. The reader should still get the content back from the server itself.
  const reader = await connectAndAuth(port, workspace);
  const readerDoc = await syncFreshDoc(reader);
  assert.equal(readerDoc.getText("note").toString(), "hello from writer");
  await closeAndWait(reader);
});

test("persistence: a workspace's document survives a full server restart", async (t) => {
  const port = RESTART_TEST_PORT;
  const snapshotsDir = await mkdtemp(join(tmpdir(), "helixnotes-collab-persist-restart-"));
  t.after(() => rm(snapshotsDir, { recursive: true, force: true }));

  const first = spawnServer(t, port, { DOC_SNAPSHOTS_DIR: snapshotsDir });
  await waitForHealthz(port, 5000);

  const workspace = "persist-restart";
  const writer = await connectAndAuth(port, workspace);
  const writerDoc = new Y.Doc();
  writerDoc.getText("note").insert(0, "still here after a restart");
  sendUpdate(writer, Y.encodeStateAsUpdate(writerDoc));
  await new Promise((r) => setTimeout(r, 200));
  await closeAndWait(writer);
  // leaveWorkspace's forced flush (src/server.ts) runs async and isn'''t awaited by the close
  // handler itself - give it a moment to actually finish writing before we kill the process.
  await new Promise((r) => setTimeout(r, 500));
  first.kill();
  await new Promise((resolve) => first.once("exit", resolve));

  const second = spawnServer(t, port, { DOC_SNAPSHOTS_DIR: snapshotsDir });
  await waitForHealthz(port, 5000);

  const reader = await connectAndAuth(port, workspace);
  const readerDoc = await syncFreshDoc(reader);
  assert.equal(readerDoc.getText("note").toString(), "still here after a restart");
  await closeAndWait(reader);
});
