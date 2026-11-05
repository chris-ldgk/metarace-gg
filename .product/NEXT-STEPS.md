# Next Steps (handoff, 2026-11-03)

Read first: `vision.md`, `decisions.md` (D-001…D-080), `open-questions.md`, `research/rre-dedicated-server.md`.
Milestone (D-027): **infra spikes only**. Plan the product build only after the spikes are resolved.

## State
- **Production verification host is live**: `autumn` (tailnet `100.69.190.52`, `ssh autumn`) runs the image in Docker behind the `autumn-gateway` exit node, public IPv4 `162.55.63.22`, with `60000-60014` DNAT'd 1:1. A smoke server was running on `60000/60002` and public TCP was reachable. Read `infra/rre-server/PRODUCTION.md` before using it; credentials stay in `/opt/rre/.env` on the host.
- S-1 (Wine), S-2b (density/ports), S-3 (REST control) pass. S-6 decided (D-028). The image was our own multi-stage build (D-075) with the patched `httpapi.dll` and a baked .NET 4.8 prefix; the Rust manager replaces those parts (D-081). The previous derivative image was built and verified; the Rust-manager image needs its first build on a Docker host.
- Real-x86 testbed (WSL2): the `r3e-oci` image layers are extracted to `~/r3e-root` (5.7 GB) and run in an unprivileged chroot with GE-Proton 11 + the image's dotnet48 prefix at `~/r3e-data`; the SteamCMD install is `~/rre-server`. The old Colima/QEMU profile is obsolete for performance conclusions. The `infra/rre-server/testbed` helper was removed with the Windows manager; the production host is the verification path now.
- (old image) Server REST API (loopback :8088, plus nginx :8080 with basic auth). Direct calls must send `Host: localhost:8088`:
  `POST /dedi/register {Secret}` → `GET /dedi/default` → `POST /dedi/ {GameSettings}` → `POST /dedi/start {ProcessId}` → poll `GET /dedi` (ProcessState). The template is `research/rre-dedi-default-gamesetting.json`.
- Gotchas (old Windows-manager image; the Rust manager below replaces this):
  - One manager runs **up to 5 live servers**; each takes a 3-port block (game UDP, game TCP = +1, time UDP = +2), allocated monotonically from 60000 (D-041). A 6th start returns `303 {"Message":"Quota exceeded!"}`.
  - Ports reset to 60000 on manager restart; every manager starts there, so multiple containers need separate IPs or a base-port mechanism.
  - Never trust `IsStarted` or exit codes; poll health (D-034). The manager auto-restarts crashed game servers on the same port and `/dedi` is briefly stale.
  - Starting a server persisted from a previous manager run crashes the manager; delete persisted IDs on boot (D-042).
  - Game logs are encrypted. The home network is behind carrier NAT with ports closed, so public listing and joining can't be tested locally.

## Manager migration (2026-11-05)

- The Windows manager is replaced by the Rust port, vendored and pruned at `infra/rre-server/manager` (D-081…D-087). One container runs one manager and one server; no scheduler, SQLite, UI or named-pipe relay. Settings arrive at boot, results push to the API, `/state` carries the PID and callback timestamps for D-034.
- **Direct join failed (S-7, 2026-11-05)**: the `rrre://` URI does not work, so D-087 reverses D-082 and the manager now registers a slave with the KW portal, creates its settings there, takes the port allocation and announces the server. Players join through the **server browser** with the per-split password. `METARACE_PORTAL_SECRET` is required per container.
- Because the manager is native, the image no longer needs nginx, the .NET 4.8 prefix or the patched `httpapi.dll`. `infra/rre-server/manager/API.md` is the contract; `infra/rre-server/README.md` documents the image. The crate's tests pass locally.
- **Deployed in parallel on `autumn` as `metarace-rs`** and **browser join verified on 2026-11-05**: it shares the `rrre-rs` network namespace, uses the portal-allocated game port `60003` (portal slave `168882`, settings `511826`, quota 5), is listed as **`metarace-rs test`**, and a client joined with the per-split password. The existing `rrre-rs` container is untouched (its own game server has since exited).
- The test image was built from `Dockerfile.test` on `autumn`, which reuses the `RRRE_Dedicated.exe` already baked into the previous image because SteamCMD anonymous login kept retrying. The repository Dockerfile still does the SteamCMD install; the only difference is the exe source. The prefix bake needed `wine wineboot -u` (GE-Proton has no `wineboot` wrapper) and i386 libc; both are now in the Dockerfile.
- Live findings (details in `research/rre-rust-manager.md`): the portal requires the **complete** settings payload (trimmed returns 250); the announcement needs `MpServerList[].Id`, an ISO-8601 `Created` and `Settings.SlaveId`; the portal's `GetPort` allocation advances per settings create and must be used; the game server binds **IPv6-only** without `-ip`, so the manager now auto-detects the container IPv4 (D-088); a zero-player server ran 30 min practice + 10 min qualifying and exited cleanly, while `-sessionFreeze 1` kept it joinable.
- Next on the host: run a short race on `metarace-rs` to capture the result JSON (S-4), then point `METARACE_RESULT_URL` at a local sink to verify the push. Clean up the test container and its portal settings when done. The production image needs a rebuild to pick up the IPv4 auto-detect and announcement fixes (the running test used the explicit `METARACE_DEDI_IP`).

## Runnable on the production host (`autumn`; was blocked on a public box)
1. Listing and joining with host-port remapping. If that fails, use macvlan with one IP per container (D-041).
2. Load test with real clients: idle cost is measured (~0.17 core / ~110 MB per server), loaded cost is not.
3. Re-measure idle cost on the production host CPU (the current numbers are from the Windows desktop CPU under WSL).

See `infra/rre-server/PRODUCTION.md`. The host is a shared box (it also runs Jellyfin, Pi-hole and qBittorrent); stop the `rre` container before experiments that need `60000-60014` or the public IP.

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
