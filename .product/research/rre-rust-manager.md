# Rust manager live verification (2026-11-05)

Setup: the pruned Rust manager (`infra/rre-server/manager`) was built on `autumn`
and run **in parallel** with the existing `rrre-rs` container, sharing its
network namespace (`--network container:rrre-rs`). The new instance used a fresh
portal secret (slave `168882`), the live server settings copied from the running
smoke server, manager port `18088`, and the portal-allocated game port `60003`.

**Result: joined successfully from the in-game server browser** as
`metarace-rs test` with the per-split password. This is the first end-to-end
join through the Rust manager.

## 1. Direct join is dead (S-7)

The `rrre://run?data=...` URI does not put a client on a private server. The
portal/server-browser path is the only working join path, so D-087 supersedes
D-082: every manager registers a slave, creates its settings on the portal and
announces the server.

## 2. The portal requires the complete settings payload

A trimmed `MultiplayerServerSettings` object (only the fields the argument
builder reads) was rejected:

```
portal settings failed: Saving of Multiplayer settings failed ........ (code 250)
```

Replaying the live settings **verbatim**, changing only `Id`, `Name`, `Password`
and `MaxNumOfPlayers`, succeeded and returned a settings id. The orchestrator
and CMS must therefore build the complete RRE settings object, not a minimal
one. The live settings had all six `TrackLayouts` and `Practice`/`Qualify`/
`Race1..3` sessions.

## 3. Announcement schema

The manager's announcement was accepted only after these corrections:

- `Created` must be an **ISO 8601 string**, not a Unix timestamp. A number
  returns HTTP 400 with
  `$.MpServerList[0].Created: The JSON value could not be converted to System.DateTime`.
  The accompanying `server: The server field is required.` error is secondary;
  no root `server` field is needed once `Created` is valid.
- Each `MpServerList` entry needs **`Id`** (the portal settings id), not just the
  settings inside `GameSettings`.
- The compact `Settings` object needs **`SlaveId`** in addition to the fields
  copied from the original manager.

Shape that the live portal accepted (`true`):

```json
{
  "SlaveId": 168882,
  "AnnouncedTime": 1793908000000,
  "MpServerList": [
    {
      "Id": 511826,
      "GameSettings": { "...": "complete MultiplayerServerSettings, Id set" },
      "Created": "2026-11-05T21:03:31.000Z",
      "ServerIp": "162.55.63.22",
      "Port": 60003,
      "PortTimeCritical": 60005,
      "CurrentSession": 0,
      "TimeLeft": 1800000,
      "PlayersOnServer": 0,
      "Players": [],
      "Settings": {
        "Id": 511826,
        "ServerName": "metarace-rs test",
        "HasPassword": true,
        "Thumbnail": null,
        "ExperienceId": 2481,
        "TrackLayoutId": [263, 264, 265, 266, 267, 10414],
        "LiveryId": [257, 1934, "..."],
        "Difficulty": 0,
        "MaxNumberOfPlayers": 8,
        "FuelUsage": 0,
        "TireWear": 0,
        "MechanicalDamage": 0,
        "FlagRules": 0,
        "PracticeDuration": 30,
        "QualifyDuration": 10,
        "QualifyStartMode": 1,
        "Race1Duration": 10,
        "Race2Duration": 10,
        "Race3Duration": 10,
        "CutRules": 0,
        "MandatoryPitStop": 0,
        "ReverseGridPlaces": 0,
        "DriveAssist": { "...": "Preset..Autopit" },
        "IsRanked": false,
        "MinRating": 0,
        "MaxRating": 0,
        "MinReputation": 0,
        "MaxReputation": 0,
        "PreventFalseStart": false,
        "AllowQualJoin": false,
        "BallastCarIds": [],
        "PowerRestrictionCarIds": [],
        "SlaveId": 168882
      }
    }
  ]
}
```

The CLR metadata of `RRRE_Dedicated.exe` confirms the field sets: `MpServerList`
has `Created`, `ServerIp`, `Port`, `PortTimeCritical`, `CurrentSession`,
`TimeLeft`, `PlayersOnServer`, `Players`, `Settings`, `Id`; `MpServerSettings`
carries the compact fields plus `SlaveId`. An empty `MpServerList` is accepted
and removes the entry (used on `/stop`).

## 4. Use the portal's port allocation

`GetPort` is per slave/settings and advances as new settings are created on a
reused secret: observations were `60000`, `60003`, `60006`, then `60003` again
after stale settings were reclaimed. Forcing `60003` while the portal allocated
`60006` produced a listing/listener mismatch. The manager now uses the returned
port unless `METARACE_GAME_PORT` is explicitly set **and matches the container
mapping**. A fresh secret per container (D-031) avoids stale settings and ports.

## 5. The game server binds IPv6 unless told otherwise

With no `-ip`, `DedicatedServer.exe` under GE-Proton binds IPv6 wildcard
sockets only:

```
/proc/net/udp6: ... :EA63 (60003), :EA65 (60005)
/proc/net/tcp6: ... :EA64 (60004)
```

Docker publishes IPv4 and the gateway DNAT is IPv4, so public TCP `60004` was
refused and the browser join failed with "multiplayer session is closed".

Passing `-ip <container-ipv4>` made the server bind `::ffff:172.21.0.2` and the
public TCP path opened; the browser join then succeeded. The manager now
auto-detects the local IPv4 at boot and passes `-ip`; `METARACE_DEDI_IP`
overrides it (D-088). Auto-detection must still be verified in a normal bridged
container, not only the shared-netns test.

## 6. Session lifetime and the join window

A zero-player server ran practice (30 min) + qualifying (10 min) and then exited
cleanly (`exitCode: 0`) at about 40 minutes. The portal still listed it briefly,
but a join returned "multiplayer session is closed".

`-sessionFreeze 1` (from `StayInPracticeSessionIfEmpty = 1`) kept practice open
and the join succeeded. This is a **test setting**: a frozen practice conflicts
with the fixed T-0 start in D-016/D-018. Ranked splits need either the 5-minute
practice to run from T-5 (late joiners are no-shows, D-015) or a forced session
advance, which D-085 deferred. Tracked in `open-questions.md`.

## 7. The running port image's announcement is stale

The live `rrre-rs` container announced with `Created` as a number and was
rejected with the same 400 error; its image predates the ISO-string fix in the
sibling source. This explains why the smoke server was not listed. Our manager
sends the ISO string.

## Evidence

- Portal registration: `slave_id=168882`, `slave_ip=162.55.63.22`, quota 5.
- Settings ids observed: `511798`, `511812`, `511813`, `511814`, `511825`,
  `511826`.
- Game server args included `-ip 172.21.0.2 -port 60003 -timePort 60005
  -password *** -practice1Minutes 30 -sessionFreeze 1`.
- Public TCP `60004` open from the workstation; browser join confirmed.
- Manager `/state` reported `running: true`, `currentSession: 0`,
  `timeLeft: 1800000`, `portal.port: 60003`.

## Open items

- S-4: run a short race and capture the result JSON (the test server is ready).
- S-8: clean T-5 boot-to-listed measurement; the join itself is verified.
- Session timing vs freeze (above).
- Replace the local result storage with `METARACE_RESULT_URL` once the ingest
  endpoint exists.
- Clean up the `metarace-rs` test container and its portal settings when done.
