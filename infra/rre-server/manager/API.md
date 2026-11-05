# Manager contract

One container runs one manager and one `DedicatedServer.exe`. At boot the
manager registers a slave with the RaceRoom portal (D-087), creates its settings
there, takes the game-port allocation and announces the server so it appears in
the in-game server browser. Players join through the browser with the per-split
password; the direct `rrre://` URI is not a supported path.

The manager starts the server once and never restarts it: a game-server crash is
terminal and the orchestrator voids the split (D-008, D-083).

The manager binds `METARACE_BIND` (default `0.0.0.0:8088`). Control routes
require `Authorization: Bearer $METARACE_CONTROL_TOKEN`; the `/slave/*` routes
are unauthenticated because the game server sends no credentials, and must only
be reachable on the container network.

## Control routes

| Method | Path | Response |
|---|---|---|
| `GET` | `/healthz` | `ok` (no auth) |
| `GET` | `/state` | game state and portal status |
| `POST` | `/stop` | `{"stopped": true}`; announces an empty list first |
| `GET` | `/results` | `[{ "name", "bytes", "modified" }]` |
| `GET` | `/results/{name}` | the stored result JSON |

`GET /state`:

```json
{
  "game": {
    "running": true,
    "pid": 1234,
    "port": 60003,
    "timePort": 60005,
    "playersOnServer": 12,
    "timeLeft": 321000,
    "sessionTime": 300000,
    "sessionTimeLeft": 210000,
    "currentSession": 1,
    "players": [{ "UserId": 1, "Name": "driver" }],
    "startedAt": 1793905703,
    "lastStateAt": 1793905703,
    "exitCode": null,
    "resultReceivedAt": 1793905703,
    "resultDeliveredAt": null
  },
  "portal": {
    "registered": true,
    "slaveId": 168857,
    "slaveIp": "162.55.63.22",
    "quota": 5,
    "settingsId": 511680,
    "port": 60003
  }
}
```

`currentSession`: `0` practice, `1` qualifying, `2` race (server-reported).
Times are reported by the game server in milliseconds. `lastStateAt` is the last
time `/slave/DedicatedServerState` arrived, `startedAt` is when the manager
spawned the process, and `exitCode` is set once the process is gone.

Without `-ip`, Wine binds the game server to IPv6 wildcard sockets only and the
IPv4 Docker/gateway path is unreachable; the manager therefore always passes the
auto-detected IPv4 (D-088). A zero-player server advances through its configured
sessions and exits cleanly; set `StayInPracticeSessionIfEmpty = 1` when the
server must stay joinable while empty (test/practice), accepting that it delays
the configured start.

Health rule for the orchestrator (D-034): treat the server as healthy only when
`game.running` is true, `game.pid` is stable, `game.lastStateAt` is recent (a few
seconds) and `game.sessionTimeLeft` advances. Do not trust the spawn result
alone.

## Portal flow

1. `POST` the registration endpoint with the `secret` header. The response gives
   `SlaveId`, `SlaveIp`, `DediQuota` and every endpoint used below.
2. `POST SetSettingsEndpoint` with the **complete** `MultiplayerServerSettings`
   object (`Id` is forced to 0). The response id is written back as
   `GameSettings.Id`. A trimmed payload is rejected by the portal with
   `Saving of Multiplayer settings failed (250)`.
3. `GET GetPortEndpoint`. The returned port is used for the game server and the
   announcement. `METARACE_GAME_PORT` overrides it only when the container's
   port mapping matches; otherwise the listed port is unreachable.
4. Spawn the game server, then announce `MpServerList` every
   `AnnounceInterval` and player coordinates every
   `AnnouncePlayerCoordinatesInterval`.
5. Re-register every six hours to keep the slave alive.
6. On `/stop` or SIGTERM, announce an empty `MpServerList` so the browser drops
   the entry, then stop the process.

### Announcement contract

`AnnounceEndpoint` accepts:

```json
{
  "SlaveId": 168882,
  "AnnouncedTime": 1793908000000,
  "MpServerList": [
    {
      "Id": 511826,
      "GameSettings": { "...": "complete MultiplayerServerSettings" },
      "Created": "2026-11-05T21:03:31.000Z",
      "ServerIp": "162.55.63.22",
      "Port": 60003,
      "PortTimeCritical": 60005,
      "CurrentSession": 0,
      "TimeLeft": 1800000,
      "PlayersOnServer": 0,
      "Players": [],
      "Settings": { "...": "compact MpServerSettings, including SlaveId" }
    }
  ]
}
```

Requirements (learned from the live portal, `research/rre-rust-manager.md`):

- `Created` must be an ISO 8601 **string**. A Unix-millisecond number returns
  HTTP 400 (`The JSON value could not be converted to System.DateTime`); the
  accompanying `server is required` error is secondary.
- Each list entry needs `Id` (the settings id) and the compact `Settings` object
  needs `SlaveId`. Without them the announcement may answer `true` but the entry
  is not usable.
- `Settings` carries the compact fields `ServerName`, `HasPassword`,
  `Thumbnail`, `ExperienceId`, `TrackLayoutId`, `LiveryId`, `Difficulty`,
  `MaxNumberOfPlayers`, `FuelUsage`, `TireWear`, `MechanicalDamage`, `FlagRules`,
  the session durations/start modes, `CutRules`, `MandatoryPitStop`,
  `ReverseGridPlaces`, `DriveAssist`, `IsRanked`, the rating/reputation bounds,
  `PreventFalseStart`, `AllowQualJoin`, `BallastCarIds`,
  `PowerRestrictionCarIds` and `SlaveId`.
- An empty `MpServerList` is accepted and removes the slave's listing.

## Game-server routes

The game server reads `MilkyWay.dedicated.config` from its working directory and
POSTs encrypted envelopes to:

| Method | Path | Response |
|---|---|---|
| `POST` | `/slave/DedicatedServerState` | encrypted `{ "Ranked": bool }` |
| `POST` | `/slave/MultiplayerEventResult` | encrypted `{ "MultiplayerEventResultResult": bool }` |
| `POST` | `/slave/GridRequest` | encrypted portal grid-request result; `501` when unregistered |

All accept the encrypted `EncryptedMessage` envelope or plain JSON (useful for
tests). `Ranked` follows `RankedServer` in the settings.

`MultiplayerEventResultResult` is `true` only once the result has been stored
and delivered to `METARACE_RESULT_URL`. A duplicate callback with byte-identical
content is answered immediately from the stored delivery.

## Result push

The manager writes the decrypted result to `/data/results/<utc>-<sha256>.json`
and then POSTs the exact bytes to `METARACE_RESULT_URL`:

```
Authorization: Bearer $METARACE_RESULT_TOKEN
Content-Type: application/json
X-Metarace-Run-Id: $METARACE_RUN_ID
X-Metarace-Server-Id: $METARACE_SERVER_ID
X-Metarace-Result-Sha256: <hex sha256 of the body>
User-Agent: metarace-rre-manager/<version>
```

A non-2xx response or a transport error is retried with exponential backoff
(1, 2, 4, 8, 16 seconds; `METARACE_RESULT_ATTEMPTS` attempts, default 5). After
the last failure the game server is told `false`, the file stays on disk and
`resultDeliveredAt` stays `null`, so the orchestrator can still pull it with
`GET /results` before destroying the container. The ingest endpoint must be
idempotent on the hash (D-048).

The manager also posts the enriched result (`SlaveId`, `SlaveName`, `Created`,
`CorrelationId`, `SpinNumber`, `GameSettings`) to the portal's
`WebPortalRaceResultsEndpoint`. The metarace push is authoritative; a portal
failure is logged and does not fail the game-server acknowledgement.

## Environment

| Variable | Default | Meaning |
|---|---|---|
| `METARACE_SETTINGS` / `METARACE_SETTINGS_FILE` | — | RRE `MultiplayerServerSettings` JSON (one is required) |
| `METARACE_BIND` | `0.0.0.0:8088` | control listener |
| `METARACE_CONTROL_TOKEN` | — | bearer token for control routes |
| `METARACE_ALLOW_INSECURE` | `false` | permit an empty control token (local tests only) |
| `METARACE_PORTAL_SECRET` | — | slave secret; **required** unless the portal is disabled |
| `METARACE_PORTAL_DISABLED` | `false` | private/local mode without registration |
| `METARACE_PORTAL_REGISTRATION_URL` | v112 production endpoint | registration override |
| `METARACE_GAME_PORT` | portal allocation | fixed game port; overrides `GetPort` (must match the container mapping) |
| `METARACE_DEDI_IP` | auto-detected container IPv4 | passed to the game server as `-ip`; **required for IPv4 reachability** |
| `METARACE_PUBLIC_URL` | loopback | base URL written into `MilkyWay.dedicated.config` |
| `METARACE_DATA_DIR` | `/data` | extraction, logs and results root |
| `METARACE_RESULTS_DIR` | `$DATA/results` | stored result files |
| `METARACE_GAME_DIR` | embedded ZIP | use an extracted game tree |
| `METARACE_GAME_ZIP` | embedded ZIP | use a different game ZIP |
| `METARACE_GAME_LAUNCHER` | `wine` | launcher for `DedicatedServer.exe` |
| `METARACE_GAME_LAUNCHER_PREFIX_ARGS` | empty | shell-quoted launcher arguments |
| `METARACE_WINE_PREFIX` | `$DATA/wineprefix` | Wine prefix |
| `METARACE_WINEBOOT_BIN` / `METARACE_WINESERVER_BIN` | `wineboot` / `wineserver` | Wine helpers; `wine` is accepted and runs `wineboot` itself |
| `METARACE_STOP_GRACE_SECS` | `20` | SIGTERM grace before SIGKILL |
| `METARACE_RESULT_URL` | unset | result ingest endpoint |
| `METARACE_RESULT_TOKEN` | — | bearer token for the ingest endpoint |
| `METARACE_RUN_ID` / `METARACE_SERVER_ID` | unset | forwarded as result headers |
| `METARACE_RESULT_ATTEMPTS` | `5` | upload attempts |
| `METARACE_RESULT_TIMEOUT_SECS` | `15` | per-attempt timeout |
| `METARACE_LOG_LEVEL` | `info` | tracing filter (`RUST_LOG` wins) |

`METARACE_DEDI_ARGS` replaces the generated game-server arguments and
`METARACE_DEDI_EXE` replaces the executable; both exist for bring-up only.
