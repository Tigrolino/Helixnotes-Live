// Verifies the server's Stage 3 relay behavior end-to-end against a real running instance:
// binary frames (Yjs sync/update messages) broadcast to a client's workspace peers and never
// back to the sender, workspaces are isolated from each other, and text frames still echo to
// the sender only (Stage 2's original debug behavior, preserved deliberately - see server.ts's
// module doc comment). Spawns `dist/server.js` itself so `npm test` is self-contained; run
// `npm run build` first if `dist/` is stale or missing.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import WebSocket from "ws";

const __dirname = dirname(fileURLToPath(import.meta.url));
const SERVER_ENTRY = join(__dirname, "..", "dist", "server.js");
const PORT = 8799; // distinct from the default 8787 so a manually-running dev server doesn't collide
const PASSWORD = "test-smoke-password";
const URL = `ws://127.0.0.1:${PORT}`;

function waitForHealthz(timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const attempt = () => {
      fetch(`http://127.0.0.1:${PORT}/healthz`)
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

function connectAndAuth(workspace) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(URL);
    const received = [];
    ws.on("message", (data, isBinary) => {
      received.push({ isBinary, text: isBinary ? null : data.toString("utf8"), bytes: isBinary ? [...data] : null });
    });
    ws.on("error", reject);
    ws.on("open", () => {
      ws.send(JSON.stringify({ type: "auth", workspace, password: PASSWORD }));
    });
    const checkConnected = setInterval(() => {
      if (received.some((m) => m.text === JSON.stringify({ type: "connected" }))) {
        clearInterval(checkConnected);
        received.length = 0; // drop the connected-ack from the log we hand back
        resolve({ ws, received });
      }
    }, 20);
    setTimeout(() => {
      clearInterval(checkConnected);
      reject(new Error("auth timeout"));
    }, 5000);
  });
}

function wait(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

test("relay: binary frames broadcast within a workspace, text frames echo to sender", async (t) => {
  const server = spawn(process.execPath, [SERVER_ENTRY], {
    env: { ...process.env, PORT: String(PORT), COLLAB_PASSWORD: PASSWORD },
    stdio: "pipe",
  });
  t.after(() => {
    server.kill();
  });

  await waitForHealthz(5000);

  const { ws: a, received: recvA } = await connectAndAuth("team-notes");
  const { ws: b, received: recvB } = await connectAndAuth("team-notes");
  const { ws: c, received: recvC } = await connectAndAuth("other-workspace");
  t.after(() => {
    a.close();
    b.close();
    c.close();
  });

  // Binary frame from A: broadcasts to B (same workspace), never echoes to A, never reaches C
  // (different workspace).
  const update1 = new Uint8Array([2, 5, 1, 2, 3, 4, 5]);
  a.send(update1, { binary: true });
  await wait(300);

  assert.equal(recvA.length, 0, "sender receives no echo of its own binary frame");
  assert.equal(recvB.length, 1, "workspace peer receives exactly one broadcast frame");
  assert.equal(recvB[0].isBinary, true, "broadcast frame is binary");
  assert.deepEqual(recvB[0].bytes, [...update1], "broadcast frame bytes match what was sent");
  assert.equal(recvC.length, 0, "a different workspace receives nothing");

  recvA.length = 0;
  recvB.length = 0;
  recvC.length = 0;

  // Text frame from B: Stage 2's original echo-to-sender behavior, not broadcast.
  b.send("hello");
  await wait(300);
  assert.equal(recvB.length, 1, "sender of a text frame gets exactly one message back");
  assert.equal(recvB[0].text, "hello", "echoed text frame is unchanged");
  assert.equal(recvA.length, 0, "workspace peer receives nothing from a text frame");

  recvA.length = 0;
  recvB.length = 0;

  // Broadcast is bidirectional: B -> A works the same way as A -> B did above.
  const update2 = new Uint8Array([2, 3, 9, 9, 9]);
  b.send(update2, { binary: true });
  await wait(300);
  assert.equal(recvA.length, 1, "the other peer receives the reverse-direction broadcast");
  assert.deepEqual(recvA[0].bytes, [...update2], "reverse-direction broadcast bytes match");
  assert.equal(recvB.length, 0, "sender still receives no echo of its own binary frame");
});
