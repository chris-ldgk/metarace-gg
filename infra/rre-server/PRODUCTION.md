# Production verification host: `autumn`

`autumn` is the dedicated RaceRoom host on the tailnet. It runs the old port
container and the new Rust manager in parallel for verification. Read this
before changing anything on the host.

Last verified: **2026-11-05**.

## Access

```bash
ssh ckolb@10.0.0.30     # LAN, preferred
ssh autumn              # tailnet; key installed
```

| Host | Tailnet IPv4 | Public IPv4 | Role |
|---|---|---|---|
| `autumn` | `100.69.190.52` | — | Docker box |
| `autumn-gateway` | `100.69.229.78` | `162.55.63.22` | nftables DNAT, Tailscale exit node |

- `autumn` is x86_64 CachyOS, 31 GiB RAM, Docker + Compose.
- Container egress leaves through the gateway (`162.55.63.22`), which matters
  for KW portal registration.
- `ckolb` is in the `docker` group; there is **no passwordless sudo**. Reading
  `.env` files or editing systemd units needs an interactive `sudo`.
- `ssh autumn-gateway` is not configured from this workstation (host key /
  public key). Gateway changes need separate access.

## Current state

| Item | Value |
|---|---|
| Port test container | `rrre-rs`, image `rrre-manager:local`, compose `/home/ckolb/rrre-rs-docker` |
| Port portal slave | `168857`; its smoke game server has exited |
| Rust manager container | `metarace-rs`, image `metarace/rre-server:rs-test` (test build) |
| Rust manager network | shares `rrre-rs`'s netns (`--network container:rrre-rs`) |
| Rust manager portal slave | `168882`, settings `511826`, quota 5 |
| Rust manager game port | portal-allocated `60003` (game UDP, TCP 60004, time UDP 60005) |
| Rust manager control API | `172.21.0.2:18088` inside the shared netns (bearer token) |
| Browser listing | `metarace-rs test`, password `<test-password>`; join verified 2026-11-05 |
| Host game ports | `100.69.190.52:60000-60014` TCP+UDP, published by `rrre-rs` |
| Public game ports | `162.55.63.22:60000-60014` via gateway DNAT |
| Old image/bundle | `metarace/rre-server:local` and `/opt/rre` are not running; historical |

Credentials are **not** in this repo:

- `/opt/rre/.env` — old Windows-manager deployment (root-owned, `0600`).
- `/home/ckolb/rrre-rs-docker/.env` — port container.
- `/home/ckolb/metarace-rre-server/{portal_secret,control_token,manager.env}` —
  Rust-manager test deployment.

> `100.69.190.52:8080` is qBittorrent. The old manager UI was on
> `127.0.0.1:8080` inside `rre`; the current Rust manager is not UI-based.

## Rust manager test deployment (verified)

The test build reused `RRRE_Dedicated.exe` from the previous image because
SteamCMD anonymous login was retrying. The repository `Dockerfile` is unchanged
and still performs the SteamCMD install; only the exe source differed.

Build:

```bash
# Dockerfile.test replaces the SteamCMD stage with:
#   FROM metarace/rre-server:local AS server
# and copies /opt/raceroom/server/RRRE_Dedicated.exe.
cd /home/ckolb/metarace-rre-server/rre-server
docker build -f Dockerfile.test -t metarace/rre-server:rs-test .
```

Run (share the running port container's network namespace so the existing
`60000-60014` publishing and gateway DNAT cover the portal-allocated block):

```bash
docker run -d --name metarace-rs \
  --network container:rrre-rs \
  --restart unless-stopped -t \
  --env-file /home/ckolb/metarace-rre-server/manager.env \
  -v metarace-rs-data:/data \
  -v /home/ckolb/metarace-rre-server/settings.json:/run/metarace/settings.json:ro \
  metarace/rre-server:rs-test
```

`manager.env` carries the settings path, bind `0.0.0.0:18088`, control token,
portal secret, public URL and log level. Do **not** set `METARACE_GAME_PORT`
unless the container publishes exactly that port; let the portal allocate.

Inspect:

```bash
TOKEN=$(cat /home/ckolb/metarace-rre-server/control_token)
docker exec metarace-rs curl -fsS -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:18088/state
docker logs --tail 60 metarace-rs
```

Remove the test deployment:

```bash
docker rm -f metarace-rs
docker volume rm metarace-rs-data
```

The portal settings and slave behind the test secret remain on KW's side; use a
fresh secret for the next deployment (D-091) rather than reusing it.

## Live findings that shape the production deployment

1. **Portal registration is mandatory** (D-087). Direct `rrre://` join is dead.
   The Rust manager registers the slave, creates settings, announces the server
   and keeps the listing alive.
2. **The settings payload must be complete** (D-090). The portal returns
   `Saving of Multiplayer settings failed (250)` for a trimmed object.
3. **Announcement contract** (D-089): `MpServerList[].Id`, ISO-8601 `Created`,
   `Settings.SlaveId`, the portal-allocated `Port`. A numeric `Created` returns
   HTTP 400; a forced port that differs from `GetPort` lists an unreachable
   address.
4. **The game server must bind IPv4** (D-088). Without `-ip` it binds IPv6-only
   and every join fails with "multiplayer session is closed". The manager now
   auto-detects the container IP; `METARACE_DEDI_IP` overrides it.
5. **Session lifetime**: with no players the server ran practice + qualifying
   and exited cleanly at ~40 minutes. `-sessionFreeze 1`
   (`StayInPracticeSessionIfEmpty = 1`) keeps it joinable while empty; that is
   a test/practice setting and delays the scheduled start (open question).
6. **Results**: the test manager has no `METARACE_RESULT_URL`, so results stay
   in `/data/results`. The API/R2 ingest is the next production dependency.

## Routing invariants to re-check after changes

```bash
# box and container egress must be the gateway public IP
ssh ckolb@10.0.0.30 'curl -4fsS https://api.ipify.org; echo'
ssh ckolb@10.0.0.30 'docker exec rrre-rs curl -4fsS https://api.ipify.org; echo'

# Docker subnets stay on the main table
ssh ckolb@10.0.0.30 'ip rule show | grep -E "5269|5270"'

# published ports must be the tailnet address, not 0.0.0.0
ssh ckolb@10.0.0.30 'docker port rrre-rs | sort -V'

# public TCP path (game TCP = game UDP + 1); test from a public host, not from
# autumn, because the gateway DNAT does not hairpin from its own host
timeout 4 bash -c 'cat < /dev/null > /dev/tcp/162.55.63.22/60004'
```

Observed 2026-11-05: egress `162.55.63.22`; rule `5269` present; all
`60000-60014` mappings on `100.69.190.52`; public TCP `60004` open from the
workstation (and `60001` for the old port server).

## Production migration checklist

1. Build the repository `Dockerfile` on a Docker host (SteamCMD + GE-Proton;
   anonymous SteamCMD login may need retries).
2. Decide how to map the **portal-allocated** port before starting the
   container: Docker cannot add a publish mapping after start. Options are host
   networking with one manager port per container, or one public IP per
   container; the shared-netns trick used here is test-only.
3. Give each container a fresh portal secret (D-031/D-091), the full settings
   JSON, a control token and `METARACE_RESULT_URL`.
4. Publish the allocated 3-port block (or the 60000-60014 range) 1:1 on the host
   and through the gateway.
5. Verify `/state`, then join through the browser with the per-split password.

## Historical: old Windows-manager image

The old `/opt/rre` deployment ran `RRRE_Dedicated.exe` under Wine + .NET 4.8
with nginx basic auth, `/dedi` and `Host: localhost:8088`. It is superseded by
the Rust manager (D-081). Its commands (`docker exec rre ... /dedi`) apply only
to that image and no longer describe the running host.
