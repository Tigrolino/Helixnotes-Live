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

This server still has no idea what a note or a notebook *means* - it never parses note structure,
titles, or the tree. Stage 6 (see "Live document persistence" below) does decode the same binary
frames' envelope enough to keep a per-workspace shadow copy of the raw Yjs state and save it -
still without understanding anything about what's inside it. The client's own
`src/lib/collab/syncProtocol.ts` remains the only place that understands note/notebook structure.
The GitHub upload backup (Stage 5) and live-document persistence (Stage 6) are later additions to
this same project, not a rewrite.

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
everyone's typing into) is persisted separately - see "Live document persistence" below.

## Live document persistence

Every workspace's live Yjs document is kept durable, not just relayed:

- The server holds one in-memory shadow copy of each workspace's document, built purely by
  decoding the same sync/update frames it's already relaying (it still never looks at note
  titles, tree structure, or anything else about what's *in* the document).
- That shadow copy is saved to local disk a couple of seconds after the last edit (debounced), and
  - if `GITHUB_TOKEN`/`GITHUB_REPO` are set - also pushed to that repo, throttled to at most once
  a minute per workspace, at `snapshots/<workspace>.ydoc`. Whoever leaves a workspace last, or a
  server shutdown, forces an immediate save instead of waiting out the throttle.
- Whoever next connects to a workspace - a reconnect, everyone having left and come back later, a
  fresh Render instance after a redeploy - gets synced from that saved state, even if nobody else
  happens to be online at that exact moment to sync from directly. Without this, a workspace with
  no peers currently connected would hand a joining client a blank document.

Same durability story as uploads: this server's own disk isn't guaranteed to survive a redeploy
(see "Deploying to Render" below), so `GITHUB_TOKEN`/`GITHUB_REPO` is what makes a snapshot
outlive one. Without them, persistence still works locally - across reconnects and restarts on
the same instance - just not across a redeploy that wipes the disk. `DOC_SAVE_DEBOUNCE_MS`,
`DOC_SAVE_MAX_DELAY_MS`, `DOC_GITHUB_SAVE_MIN_INTERVAL_MS`, and `DOC_SNAPSHOTS_DIR` are all
optional tuning - see `.env.example` for their defaults.

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

This directory lives inside the same repo as the rest of HelixNotes
(`github.com/Tigrolino/Helixnotes-Live`), so there's nothing extra to push — Render just needs to
be pointed at this subdirectory of that one repo.

1. On [render.com](https://render.com), create a new **Web Service** and connect the
   `Tigrolino/Helixnotes-Live` GitHub repo (Render will ask to install its GitHub App and pick
   which repos it can see, if this is the first service you've connected).
2. **Root Directory**: `collab-server` — this is what tells Render to build/run only this
   subdirectory instead of the whole monorepo.
3. **Build Command**: `npm install && npm run build`
4. **Start Command**: `npm start`
5. Environment variables (Render dashboard → Environment, after the service is created — or the
   "Advanced" section while creating it):
   - `COLLAB_PASSWORD` — the shared workspace secret. Generate one, don't reuse a real password:
     `openssl rand -base64 24`
   - `PORT` — Render sets this automatically; don't override it.
   - `GITHUB_TOKEN` / `GITHUB_REPO` (optional, both together) — back up both uploads (see
     "GitHub backup for uploads" above) and live-document snapshots (see "Live document
     persistence" above) to this repo. `GITHUB_REPO` can be this same `Tigrolino/Helixnotes-Live`
     repo (uploads land under `attachments/<workspace>/...`, snapshots under
     `snapshots/<workspace>.ydoc`, both well clear of the source tree) or a separate one — either
     works. Without these, uploads and document persistence still work, just don't survive a
     redeploy.
6. Render terminates TLS at its edge, so the public URL is `wss://<your-service>.onrender.com` —
   that's what goes in HelixNotes's Settings → Collaboration → Server URL. The same host, with
   `https://` instead of `wss://`, is what the client uses for uploads - it derives that itself
   from the same Server URL setting, nothing extra to configure there.
7. Render's free/starter plan has **no persistent disk** by default - a redeploy or restart wipes
   anything `UPLOADS_DIR` or `DOC_SNAPSHOTS_DIR` saved locally. If you want uploads and live
   documents to survive that without relying on GitHub backup, attach a paid Render Disk mounted
   to cover both paths instead (or in addition) - GitHub backup and a persistent disk aren't
   mutually exclusive, either is enough on its own.
8. Every `git push` to `main` auto-deploys this service by default (Render watches the whole
   repo, not just `collab-server/`, so a HelixNotes-app-only commit will also trigger a redeploy
   of this service even though nothing here changed) - that's harmless, just a few seconds of
   unnecessary rebuild. Render's dashboard has a per-service "Auto-Deploy" toggle if you'd rather
   deploy manually instead.

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

- No per-note routing within a workspace — a workspace is currently one shared broadcast group,
  not yet split per document (fine while there's only ever one test document, Stage 3/4's scope)
- No per-user accounts — see the analysis doc §8 for why that's an explicit, later choice
