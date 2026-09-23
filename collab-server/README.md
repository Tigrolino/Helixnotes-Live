# HelixNotes collaboration server

**Stages 2–4 of the collaboration feature** (see the "HelixNotes Collaboration — Technical
Analysis" doc). Stage 2 proved the transport the Rust client in
`HelixNotes/src-tauri/src/collab.rs` uses — authentication, connection status, and
reconnect-with-backoff. Stage 3 turned it into a real (if deliberately dumb) relay: binary frames
are now broadcast to a client's other authenticated peers in the same workspace, instead of being
echoed back to the sender. Stage 4 (live cursors, selections, presence, typing) needed **no
changes here at all** - it's carried as another binary-frame message type
(`MESSAGE_AWARENESS`, see the client's `syncProtocol.ts`) inside the exact same broadcast path
Stage 3 already built, which is the point of keeping this relay opaque to what it's relaying.

This server still has no idea what Yjs, awareness, a note, or a document is - it does not parse
the binary frames it relays, it just moves them between the right sockets. The client's own
`src/lib/collab/syncProtocol.ts` is the only place that understands the protocol carried inside
those frames. The GitHub bridge and real persistence are separate, later additions to this same
project (Stage 5), not a rewrite.

## Protocol

1. Client connects, then sends as its first message:
   ```json
   { "type": "auth", "workspace": "<workspace id>", "password": "<shared secret>" }
   ```
2. Server replies `{"type":"connected"}` on success, or closes the socket with a close reason
   describing why (invalid credentials, malformed first message, or an auth timeout).
3. After that:
   - **Binary frames** (Yjs sync/update messages) are broadcast to every other authenticated
     client in the same `workspace` - never back to the sender, never to a different workspace.
   - **Text frames** are still echoed back to the sender only, unchanged - this is Stage 2's
     original debug/proof-of-transport behavior, kept as-is since nothing relies on relaying text
     between peers yet.

The password is a single shared secret for the whole server (`COLLAB_PASSWORD`), matching the
"Collaboration password" field in HelixNotes's Settings → Collaboration tab — there is no
per-user account system, by design (see the analysis doc, §8).

## File and image uploads

Pasting or dropping an image/file into a note in the Live Notebook uploads it here over plain
HTTP, alongside the WebSocket:

- `POST /upload/<workspace>` - the file's raw bytes as the request body (one file per request,
  no multipart), with headers `X-Collab-Password: <shared secret>` and
  `X-File-Name: <percent-encoded original filename>`. Replies
  `{"id", "url", "name", "size", "githubBackedUp"}` on success, where `url` is a path like
  `/uploads/<workspace>/<id>-<name>` to fetch it back from.
- `GET /uploads/<workspace>/<stored-name>?password=<shared secret>` - serves the file back. The
  password is a query parameter here (not a header) because this exact URL is what gets embedded
  as the note's `<img src>` / `<a href>`, and a plain GET (an `<img>` tag, a browser navigation)
  can't attach custom headers.

Both are capped by `UPLOAD_MAX_BYTES` (default 95 MB - see `.env.example`), enforced against the
actual bytes received, not just a client-supplied `Content-Length`.

### GitHub backup for uploads

This server's own disk is **not durable** - see "Deploying to Render" below, its free/starter
filesystem is wiped on every redeploy/restart. Set `GITHUB_TOKEN` and `GITHUB_REPO` (see
`.env.example`) to also push every upload to a GitHub repo via the Contents API as a best-effort
backup, at `attachments/<workspace>/<stored-name>` in that repo. Leave them unset and uploads
still work, they just don't survive a redeploy - fine for local testing, not for anything you'd
rely on.

A GitHub push failure never fails the upload itself (the file already made it to local disk and
`GET /uploads/...` still serves it) - it's logged server-side and `githubBackedUp: false` comes
back in the response instead.

This backs up **uploaded files only**. The live document content itself (the Yjs CRDT state
everyone's typing into) still isn't persisted anywhere - it lives only in each connected client's
memory and this server's in-flight relay. That's a separate, larger piece of future work, not
something this endpoint does as a side effect.

## Running locally

```sh
npm install
cp .env.example .env   # then edit COLLAB_PASSWORD
npm run dev            # tsx watch src/server.ts, restarts on save
```

`.env` is loaded automatically (via `dotenv`, imported first thing in `src/server.ts`) - no need to
`export` the variables yourself for local dev. If you rebuild and run the compiled output directly
(`npm run build && npm start`, or `node dist/server.js`), the same `.env` in the current directory
is picked up the same way; running a stale `dist/server.js` from before a source change (skipping
`npm run build`) is the one way this can silently not reflect your latest edits.

The server listens on `PORT` (default `8787`) and answers `GET /healthz` with `200 ok` for a
quick liveness check.

### Testing with two simultaneous local clients

HelixNotes normally allows only one running instance (`tauri-plugin-single-instance` in
`src-tauri/src/lib.rs`) - launching it again just focuses the existing window. That's the right
behavior for a note-taking app (two instances writing the same vault files is how you corrupt
them), but it also means you can't just double-click the app twice to simulate two collaborators.

To test with two simultaneous local "users":

1. Start `collab-server` (`npm run dev`, as above).
2. In `HelixNotes/`, start the app as usual: `pnpm tauri:dev`. This launches the Vite dev server
   and window 1. Open Settings → Collaboration, connect, and open the test document.
3. For window 2, run the already-built debug binary directly (bypassing the `tauri dev` CLI, so
   it doesn't try to start a second Vite dev server on the same port - it just connects to the one
   already running from step 2), with a debug-only escape hatch set:
   - Windows: `set HELIXNOTES_ALLOW_SECOND_INSTANCE=1 && src-tauri\target\debug\helixnotes.exe`
   - macOS/Linux: `HELIXNOTES_ALLOW_SECOND_INSTANCE=1 ./src-tauri/target/debug/helixnotes`

   If that binary doesn't exist yet, run `pnpm tauri:dev` once first (or `cargo build --manifest-path
   src-tauri/Cargo.toml`) to produce it, then use step 3 for the second window from then on.
4. Set a different display name for each window in Settings → Collaboration, open the test
   document in both, and type in each - changes, cursors, and presence should show up live in the
   other.

`HELIXNOTES_ALLOW_SECOND_INSTANCE` only has any effect in a debug build - it's compiled out
entirely (not just ignored) in a release build, so it can never affect real users. It only bypasses
the single-instance *lock*, not vault safety, so avoid editing real notes in both windows at once;
the collaboration test document itself is fine to use in both; it's isolated from vault files by
design (see the panel's own "Not saved to any note" hint).

## Deploying to Render

1. Push this `collab-server/` directory to a Git repository (it can live in the same repo as
   HelixNotes or its own — Render just needs a root directory to build from).
2. Create a new **Web Service** on Render pointing at that repo/directory.
3. Build command: `npm install && npm run build`
4. Start command: `npm start`
5. Environment variables (Render dashboard → Environment):
   - `COLLAB_PASSWORD` — the shared workspace secret. Generate one, don't reuse a real password:
     `openssl rand -base64 24`
   - `PORT` — Render sets this automatically; don't override it.
   - `GITHUB_TOKEN` / `GITHUB_REPO` (optional, both together) — see "GitHub backup for uploads"
     above. Without these, uploads work but don't survive a redeploy.
6. Render terminates TLS at its edge, so the public URL is `wss://<your-service>.onrender.com` —
   that's what goes in HelixNotes's Settings → Collaboration → Server URL. The same host, with
   `https://` instead of `wss://`, is what the client uses for uploads - it derives that itself
   from the same Server URL setting, nothing extra to configure there.
7. Render's free/starter plan has **no persistent disk** by default - a redeploy or restart wipes
   anything `UPLOADS_DIR` saved locally. If you want uploads to survive that without relying on
   GitHub backup, attach a paid Render Disk mounted at `UPLOADS_DIR`'s path instead (or in
   addition) - GitHub backup and a persistent disk aren't mutually exclusive, either is enough on
   its own.

Render's free/starter web services spin down after a period of no HTTP traffic and cold-start on
the next request; the first WebSocket connect after an idle period may take a few seconds longer
than usual while the instance wakes up. That's expected at this stage and not a bug in the client's
timeout/reconnect handling.

## Testing

```sh
npm test   # builds, then spawns the server and exercises the relay: broadcast, sender
           # exclusion, workspace isolation, bidirectionality (tests/relay.test.mjs)
```

## What's deliberately not here yet

- No persistence for the live document content itself - the Yjs CRDT state is still only ever
  in-memory (both here and in each client); only file/image uploads (above) have any durable
  backup, and only once GITHUB_TOKEN/GITHUB_REPO are configured
- No per-note routing within a workspace — a workspace is currently one shared broadcast group,
  not yet split per document (fine while there's only ever one test document, Stage 3/4's scope)
- No per-user accounts — see the analysis doc §8 for why that's an explicit, later choice
