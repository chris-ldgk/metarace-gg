# Next Steps (handoff, 2026-11-03)

Read first: `vision.md`, `decisions.md` (D-001…D-080), `open-questions.md`, `research/rre-dedicated-server.md`.
Milestone (D-027): **infra spikes only**. Plan the product build only after the spikes are resolved.

## State
- S-1 (Wine), S-2b (density/ports), S-3 (REST control) pass. S-6 decided (D-028). The image is now our own multi-stage build (D-075): it compiles the patched `httpapi.dll`, installs SteamCMD and the server, and bakes the .NET 4.8 prefix. The previous derivative image was built and verified; the new one needs its first build on a Docker host.
- Real-x86 testbed (WSL2): the `r3e-oci` image layers are extracted to `~/r3e-root` (5.7 GB) and run in an unprivileged chroot with GE-Proton 11 + the image's dotnet48 prefix at `~/r3e-data`; the SteamCMD install is `~/rre-server`. Helper: `infra/rre-server/testbed/dedi.py`. The old Colima/QEMU profile is obsolete for performance conclusions.
- Server REST API (loopback :8088, plus nginx :8080 with basic auth). Direct calls must send `Host: localhost:8088`:
  `POST /dedi/register {Secret}` → `GET /dedi/default` → `POST /dedi/ {GameSettings}` → `POST /dedi/start {ProcessId}` → poll `GET /dedi` (ProcessState). The template is `research/rre-dedi-default-gamesetting.json`.
- Gotchas:
  - One manager runs **up to 5 live servers**; each takes a 3-port block (game UDP, game TCP = +1, time UDP = +2), allocated monotonically from 60000 (D-041). A 6th start returns `303 {"Message":"Quota exceeded!"}`.
  - Ports reset to 60000 on manager restart; every manager starts there, so multiple containers need separate IPs or a base-port mechanism.
  - Never trust `IsStarted` or exit codes; poll health (D-034). The manager auto-restarts crashed game servers on the same port and `/dedi` is briefly stale.
  - Starting a server persisted from a previous manager run crashes the manager; delete persisted IDs on boot (D-042).
  - Game logs are encrypted. The home network is behind carrier NAT with ports closed, so public listing and joining can't be tested locally.

## Blocked on production box (public IP, open UDP/TCP 60000+)
1. Listing and joining with host-port remapping. If that fails, use macvlan with one IP per container (D-041).
2. Load test with real clients: idle cost is measured (~0.17 core / ~110 MB per server), loaded cost is not.
3. Re-measure idle cost on the production host CPU (the current numbers are from the Windows desktop CPU under WSL).

## Blocked on a Windows PC with RRE (installed on this PC)
4. Direct-join the WSL test server by IP (try the WSL address, UDP 60000) and run a short race with contacts and off-tracks. Capture the result JSON (S-4) and verify incident codes 0–9. See `research/rre-dedicated-server.md` → “Client direct join” for the URI format, the `Game\x64` working-directory gotcha and the unresolved startup-argv question.
5. Point the result upload (`POST /result/rule/{id}`) at a local test sink while doing (4) to capture the exact JSON schema.
6. SteamID ↔ RaceRoom `UserId` mapping (D-003 at risk): read `user_id` from shared memory `$R3E` and/or the `game.raceroom.com/users/<id>/?json` endpoint.
7. Ownership detection (D-021/S-5): Steam `GetOwnedGames` with DLC app IDs vs the RaceRoom portal JSON.

## Can be done now (no hardware)
8. Draft the KW Studios email (user said "not yet"; ask first): instance limit, hosting ToS for a matchmaking service, result schema.
9. Product grilling: the first pass is complete (D-035…D-080) across ratings, storage topology, race lifecycle, practice/void rules, desktop app, admin/ops and the Cloudflare stack. A second pass has already overturned four stack calls: SSR for the website (D-076), freezing definitions into occurrences (D-077), Payload authenticating via Better Auth (D-078) and Tunnel + Access replacing Workers VPC as the control path (D-079); Hyperdrive result caching is disabled outright (D-080). Next: parameter tuning once real result JSONs exist, and the spikes listed in `open-questions.md` (Better Auth/Steam, Payload custom auth, definition-freeze failure).
10. The desktop app has no background service (D-058): it is launcher + registration + results, and shared-memory telemetry is out of scope (D-039). Presence comes from the server player list; only revisit `cut_track_warnings`/`incident_points` (verify against the official `r3e.h`) if a client signal is added later.

## Working conventions (from the user)
- Ask questions via AskUserQuestion, with the recommended option first.
- Record decisions in `decisions.md` (D-NNN) and spike evidence in `research/`.
- Ask before outward-facing actions (registering with KW, emails).
