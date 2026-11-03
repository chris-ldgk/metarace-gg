# RRE dedicated server image

Our own Linux image for the RaceRoom dedicated server (Steam app 354060). It does
**not** build on `procinger/r3e-oci`; the multi-stage Dockerfile produces every
piece from pinned upstream inputs.

## What the build does

1. **`httpapi-build` stage** — downloads Valve's Wine at the revision GE-Proton 11
   ships, applies the synchronous-send `httpapi` patch, and compiles only
   `dlls/httpapi/x86_64-windows/httpapi.dll`.
2. **`runtime` stage** — installs a pinned GE-Proton release (SHA-512 checked),
   overlays the patched `httpapi.dll`, installs SteamCMD and Winetricks, then:
   - installs app 354060 anonymously into `/opt/raceroom/server`;
   - creates the .NET Framework 4.8 Wine prefix with `winetricks -q dotnet48`
     and bakes it as `/opt/raceroom/dotnet48-prefix.tar`.

At startup nothing is downloaded. The entrypoint copies the baked server into
`/data/server` if the volume is empty, extracts the prefix on first boot, and
starts nginx plus the manager under Xvfb.

## Build

```bash
docker build -t metarace/rre-server infra/rre-server
```

The build needs network access for the Wine source, GE-Proton (565 MB), SteamCMD,
the server and the .NET installer. Use `--build-arg RRE_BUILD_STAMP=$(date +%F)`
to force a fresh SteamCMD download without `--no-cache`.

The image has not been built on this workstation (no Docker in WSL). The
individual steps were executed and verified by hand against the same pinned
versions; the first image build should be done on a Docker host and smoke-tested.

## Run

```bash
docker run --rm -t --name rre \
  -e ADMIN_USERNAME=admin -e ADMIN_PASSWORD=admin \
  -p 127.0.0.1:8080:8080 \
  -p 60000-60014:60000-60014/udp \
  -p 60000-60014:60000-60014/tcp \
  -v rre-data:/data \
  metarace/rre-server
```

- `-t` is required: RaceRoom with native .NET needs a console.
- The manager is on `http://localhost:8080` (nginx, basic auth). The API is
  `:8088` inside the container.
- One manager runs **up to 5 live servers**, the quota per (public IP, secret).
  Each server consumes three consecutive ports: game UDP, game TCP = +1, time
  UDP = +2. The manager allocates them monotonically from 60000 and resets the
  counter on restart, so give each container its own IP or use a base-port
  mechanism if you run several on one host.
- Direct API clients must send `Host: localhost:8088`; the manager's HttpListener
  matches that prefix. nginx already sets it.

## Environment

| Variable | Default | Purpose |
|---|---|---|
| `UPDATE_ON_START` | `false` | `true` re-runs SteamCMD at boot; leave `false` for immutable splits |
| `SERVER_EXE` | `RRRE_Dedicated.exe` | manager executable name |
| `ADMIN_USERNAME` / `ADMIN_PASSWORD` | unset | enables nginx basic auth |
| `RRE_SERVER_DIR` | `/data/server` | where the server is provisioned/updated |

## Operational caveats

- Starting a server persisted from a previous manager run crashes the manager
  (`AccessViolationException` after unimplemented `httpapi.HttpCancelHttpRequest`).
  The orchestrator must delete persisted IDs on boot and recreate (D-042).
- Killing a game server makes the manager restart it on the same port; `/dedi`
  is briefly stale, so health checks need PID stability plus session progress
  (D-034).
- Game logs are encrypted; use `GET /dedi` and
  `drive_c/users/steamuser/AppData/Local/Sector3_Studios_AB/logs/output.log`.

See `THIRD-PARTY.md` for attribution and `testbed/` for a no-Docker runner.
