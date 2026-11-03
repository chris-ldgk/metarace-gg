# Local RRE dedicated-server testbed (Linux, no Docker)

Runs the prebuilt RaceRoom manager on a real x86 Linux host using the same runtime as
the production image: **GE-Proton 11 Wine + a .NET 4.8 prefix**, inside an
unprivileged user namespace and chroot. No root and no Docker required.

> This testbed was originally bootstrapped from the `procinger/r3e-oci` layers as a
> runtime reference. The production image is now built by our own Dockerfile
> (`infra/rre-server/Dockerfile`, D-075); once that image is built, extract *its*
> layers into `$RRE_ROOT` instead of the old ones. The runtime is the same.

## Layout

| Path | Contents |
|---|---|
| `$RRE_ROOT` (default `~/r3e-root`) | extracted `ghcr.io/procinger/r3e-oci` layers (GE-Proton, dotnet48 prefix tar, nginx, Xvfb, 32-bit libs) |
| `$RRE_DATA` (default `~/r3e-data`) | `dotnet48/` prefix, `server/RRRE_Dedicated.exe`, `start.sh`, logs |
| `~/rre-server` | SteamCMD install of app 354060 (only needed to refresh the exe) |

## Setup

```bash
# 1. Rootfs from the pinned image (anonymous GHCR pull works)
#    Resolve the amd64 manifest, then for each layer:
#    curl -H "Authorization: Bearer $TOKEN" \
#      "https://ghcr.io/v2/procinger/r3e-oci/blobs/$DIGEST" | tar -xz -C ~/r3e-root
#    See .product/research/rre-dedicated-server.md for the exact commands.

# 2. SteamCMD (anonymous install of app 354060 works)
mkdir -p ~/steamcmd && cd ~/steamcmd
curl -sSLo steamcmd.tar.gz https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz
tar -xzf steamcmd.tar.gz
./steamcmd.sh +@ShutdownOnFailedCommand 1 +@NoPromptForPassword 1 \
  +@sSteamCmdForcePlatformType windows +@sSteamCmdForcePlatformBitness 64 \
  +force_install_dir "$HOME/rre-server" +login anonymous +app_info_update 1 \
  +app_update 354060 validate +quit

# 3. Data dir: prefix + server + start script
mkdir -p ~/r3e-data/{server,home,compatdata}
tar --no-same-owner -xf ~/r3e-root/opt/raceroom/dotnet48-prefix.tar -C ~/r3e-data/dotnet48
cp ~/rre-server/RRRE_Dedicated.exe ~/r3e-data/server/
cp testbed/start.sh ~/r3e-data/
```

## Run

```bash
./testbed/run.sh          # manager on 127.0.0.1:8088 (loopback only)
python3 testbed/dedi.py list
python3 testbed/dedi.py create --count 5 --prefix test
python3 testbed/dedi.py start-all
```

The manager's `HttpListener` only matches `Host: localhost:8088`; use `dedi.py` (it sets
the header) or `curl -H 'Host: localhost:8088'`. The production nginx proxy sets the same
header.

## Observed behavior (2026-11-03)

- Five live servers per manager (KW quota per public IP + secret). Ports are allocated
  monotonically in blocks of three from 60000: game UDP, game TCP = +1, time UDP = +2.
  A sixth start returns `303 {"Message":"Quota exceeded!"}`.
- Ports reset to 60000 on manager restart. Multiple managers on one host all start at
  60000, so separate containers need separate IPs (macvlan/ipvlan) or a base-port setting.
- Killing a game server makes the manager restart it on the same port; `/dedi` is briefly
  stale (Running=true with the old TimeLeft). Track the game PID and session reset (D-034).
- Starting a server persisted from a previous manager run crashes the manager
  (`AccessViolationException` after `httpapi.dll.HttpCancelHttpRequest` is unimplemented).
  Delete persisted IDs on boot (`dedi.py delete-all`) and recreate (D-042).
- Idle cost with five servers on a 24-core desktop: ~9.6% of one core and 73 MB RSS per
  game server, plus ~37% in `wineserver` and ~1.4% / 193 MB for the manager.

## Caveats

- The user's home network is behind carrier NAT, so this cannot verify public listing or
  joining. Real clients must reach the host directly (LAN/VPN) for S-4.
- The manager's game logs are encrypted; use `GET /dedi` and the manager log at
  `drive_c/users/steamuser/AppData/Local/Sector3_Studios_AB/logs/output.log`.
