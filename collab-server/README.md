# HelixNotes Collaboration Server

This is the backend for **Live Notebook**, [HelixNotes Live](../README.md)'s real-time
collaboration feature. It's a small server you run yourself (Render's free tier works fine) that
lets a few people edit the same notebook together in real time - live cursors, seeing who's
online, and sharing images/files - with just a password (or two) to get in, no accounts needed.

## What it does

- Syncs edits between everyone connected to the same notebook, live
- Shows who's online and where their cursor is
- Lets people paste or drop in images and files (up to 95 MB)
- Lets you give each workspace its own password, so different groups can share one server
  without seeing each other's notebooks
- Saves the notebook automatically, so nothing is lost if everyone disconnects or the server
  restarts
- Can optionally back up uploads and notebooks to a GitHub repo, so they also survive a full
  redeploy (Render's free disk gets wiped on every one)

## Quick start (running it locally)

```sh
npm install
cp .env.example .env   # then open it and set COLLAB_PASSWORD to something real
npm run dev
```

The server starts on `http://localhost:8787`. Point HelixNotes Live at it from
**Settings → Collaboration** (use `ws://localhost:8787` as the Server URL for local testing).

## Deploying to Render (free hosting)

1. Push this project to a GitHub repo (if you're using the same fork as the app itself, that's
   already done - this folder just needs to be somewhere in it).
2. On [render.com](https://render.com), create a new **Web Service** and connect that repo.
3. **Root Directory**: `collab-server` (tells Render to only build/run this folder).
4. **Build Command**: `npm install && npm run build`
5. **Start Command**: `npm start`
6. Add an environment variable: `COLLAB_PASSWORD` set to a real, generated password (e.g.
   `openssl rand -base64 24`). Leave `PORT` alone - Render sets that itself.
7. Once it's live, your Server URL in HelixNotes is `wss://<your-service>.onrender.com`.

That's enough to get real-time syncing working. See **Environment variables** below for the
optional GitHub backup and other settings.

A couple of things worth knowing about Render's free tier: it has no persistent disk (a redeploy
wipes anything saved locally - GitHub backup, below, is how to avoid losing anything), and it
spins down after a period of inactivity, so the first connection after a quiet spell can take a
few extra seconds to wake back up. Both are normal, not bugs.

## Environment variables

Only `COLLAB_PASSWORD` is required - everything else has a sensible default. Full list with
defaults and comments: [`.env.example`](.env.example).

| Variable | What it's for |
|---|---|
| `COLLAB_PASSWORD` | **Required.** The shared password everyone in a group uses to connect. |
| `PORT` | Which port to listen on. Render sets this itself - don't override it there. |
| `UPLOAD_MAX_BYTES` | Max size of one uploaded file. Defaults to 95 MB. |
| `GITHUB_TOKEN` + `GITHUB_REPO` | Set both to back up uploads and notebooks to a GitHub repo, so they survive a redeploy. See `.env.example` for how to create a token. |

## How it works, in short

Anyone who connects with the right server password joins a "workspace" - just a shared room name
(set per-notebook in the app). Any change one person makes is sent to everyone else in that
workspace instantly. The server also quietly saves a copy of the notebook and any uploaded files
as they come in, so reconnecting later - or restarting the whole server - doesn't lose anything.

A workspace can also have its own password, on top of the server password. Whoever connects to a
workspace first decides this: if they supply a password, that workspace needs it from then on; if
they don't, it stays open to anyone with the server password. That lets you hand one group a
workspace and password for their project, and a different group a different workspace and
password for theirs, while still being able to connect to both yourself at once.

There's still no per-account login system, just passwords shared per group. That keeps things
simple for a small team or a group of friends, though it does mean everyone who knows a
workspace's password can see and edit everything in it.

## Testing

```sh
npm test
```

Builds the server and runs it for real, exercising the actual behavior: syncing between clients,
file uploads, and notebook persistence (across a reconnect and a full server restart).

## Known limitations

- Permissions are per-workspace, not per-notebook - give someone a workspace's password and
  they can see and edit everything in it, with no finer-grained split within one workspace.
- No per-user accounts - see the "HelixNotes Collaboration - Technical Analysis" doc for the
  reasoning behind that choice.

## License

Same license as the main project - [AGPL-3.0](../LICENSE).
