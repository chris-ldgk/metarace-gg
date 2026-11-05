# RRE dedicated server image

Our Linux image for the RaceRoom dedicated server. One container runs one
manager and one game server:

- **`metarace-rre-manager`** (Rust, native) replaces the Windows
  `RRRE_Dedicated.exe` manager (D-081). It registers a slave with the RaceRoom
  portal at boot, creates the server settings there and announces the server so
  it appears in the **in-game server browser** (D-087). It owns the game
  process, state and result callbacks; see [manager/API.md](manager/API.md).
- **`DedicatedServer.exe`** still runs under **GE-Proton Wine**, extracted from
  the game payload embedded in the manager at build time.
- **No SteamCMD, .NET 4.8 prefix, nginx or patched `httpapi.dll` at runtime.**
  The image is built from pinned upstreams and starts in seconds.

For the two-host production layout see [PRODUCTION.md](PRODUCTION.md).

## What the build does

1. **`server`** — installs Steam app 354060 anonymously with SteamCMD and keeps
   only `RRRE_Dedicated.exe` (the manager) for its embedded game payload.
2. **`manager-build`** — extracts `CommunityMultiplayer.Game.game.zip` from that
   executable and builds the Rust manager with the payload embedded.
3. **`runtime`** — installs GE-Proton (SHA-512 verified) and the Wine libraries,
   bakes a plain `wineboot` prefix, and copies the manager. Nothing is
   downloaded at startup.

## Build

```bash
docker build -t metarace/rre-server infra/rre-server
```

The build needs network access for SteamCMD, the server and GE-Proton. The
SteamCMD layer is cached until the Dockerfile changes.

## Run

One container per split (D-083). The manager auto-starts from the supplied
settings and the orchestrator drives it over the control API.

```bash
docker run --rm -t --name rre \
  -e METARACE_PORTAL_SECRET='...' \
  -e METARACE_CONTROL_TOKEN='...' \
  -e METARACE_RUN_ID='race-42-split-1' \
  -e METARACE_SERVER_ID='split-1' \
  -e METARACE_RESULT_URL='https://api.metarace.gg/internal/rre/results' \
  -e METARACE_RESULT_TOKEN='...' \
  -e METARACE_SETTINGS_FILE=/run/secrets/settings.json \
  -v "$PWD/settings.json:/run/secrets/settings.json:ro" \
  -p 127.0.0.1:8088:8088 \
  -p 60000-60014:60000-60014/udp \
  -p 60000-60014:60000-60014/tcp \
  metarace/rre-server
```

- A unique `METARACE_PORTAL_SECRET` per container gives an independent quota.
  The manager takes the portal's port allocation; `METARACE_GAME_PORT` overrides
  it only when the container mapping matches the returned port.
- The game server takes three consecutive ports (game UDP, game TCP = +1, time
  UDP = +2) inside the portal-allocated block. The block is not necessarily
  60000 — publish the 60000-60014 range (or the allocated block) and let the
  gateway DNAT the same range.
- The manager passes the container's IPv4 as `-ip` automatically; set
  `METARACE_DEDI_IP` if the container has several addresses. Without it the game
  server binds IPv6-only and the IPv4 Docker/gateway path is unreachable.
- Players join through the server browser with the password in the settings.
  The direct `rrre://` URI is not a supported join path (S-7 failed).
- `-t` is retained from the previous image. The Rust manager does not need a
  console; keep the TTY until the game server has been verified without one.
- `/data` is ephemeral: the game payload is re-extracted in about a second per
  container, and results are pushed out before teardown.
- A crashed game server is **not** restarted. The orchestrator detects it via
  `/state` and voids the split (D-008).

## Environment

| Variable | Purpose |
|---|---|
| `METARACE_SETTINGS` / `METARACE_SETTINGS_FILE` | RRE server settings JSON (required) |
| `METARACE_PORTAL_SECRET` | slave secret; required unless the portal is disabled |
| `METARACE_PORTAL_DISABLED` | `true` for a private server with no registration |
| `METARACE_CONTROL_TOKEN` | bearer token for `/state`, `/stop`, `/results` |
| `METARACE_GAME_PORT` | overrides the portal's port allocation |
| `METARACE_RESULT_URL` / `METARACE_RESULT_TOKEN` | result ingest endpoint |
| `METARACE_RUN_ID` / `METARACE_SERVER_ID` | forwarded with each result |
| `METARACE_LOG_LEVEL` | defaults to `info` |

The full list is in [manager/API.md](manager/API.md).

## Verification status

**Browser listing and join verified on `autumn` on 2026-11-05** with the Rust
manager: portal slave registration, settings creation, announcement and a real
client joining with the per-split password. The evidence, schema and failure
modes are in [`../.product/research/rre-rust-manager.md`](../../.product/research/rre-rust-manager.md).

The **slimmed runtime** still needs one clean production-Dockerfile run on
`autumn` (the test image reused the previously baked `RRRE_Dedicated.exe`
because SteamCMD anonymous login kept retrying). In particular, confirm the game
server starts without the .NET prefix, the `httpapi` patch, nginx or Xvfb, and
measure the clean T-5 boot-to-listed time.

See `THIRD-PARTY.md` for attribution.
