# Next Steps (handoff, 2026-11-03)

Read first: `vision.md`, `decisions.md` (D-001…D-034), `open-questions.md`, `research/rre-dedicated-server.md`.
Milestone (D-027): **infra spikes only**. Plan the product build only after the spikes are resolved.

## State
- S-1 (Wine), S-3 (REST control) pass. S-6 decided (D-028). The prebuilt server image `infra/rre-server/Dockerfile` is built and verified.
- Local test env: Colima profile `rre` (x86_64/QEMU, **stopped**). Start: `colima start -p rre`, then `docker --context colima-rre …`. It holds the image `metarace/rre-server:spike`. Emulation is CPU-starved: **don't run more than 2 server containers here.** Perf numbers from it aren't representative.
- Server REST API (inside the container on :8088, exposed via nginx :8080, basic auth `ADMIN_USERNAME/ADMIN_PASSWORD`):
  `POST /dedi/register {Secret}` → `GET /dedi/default` → `POST /dedi/ {GameSettings}` → `POST /dedi/start {ProcessId}` → poll `GET /dedi` (ProcessState). The template is `research/rre-dedi-default-gamesetting.json`.
- Gotchas:
  - One live server per container (port allocation is broken under Wine).
  - Never trust `IsStarted` or exit codes; poll health (D-034).
  - Game logs are encrypted.
  - The user's home network is behind carrier NAT with ports closed, so public listing and joining can't be tested locally.

## Blocked on production box (real x86, public IP, open UDP/TCP 60000+)
1. Measure the idle manager's CPU/RAM natively. That's the capacity cost per container (D-033).
2. More than 5 live servers: one shared secret vs a secret per container (D-031).
3. Listing and joining with host-port remapping. If that fails, use macvlan with one IP per container (D-033).
4. Point the result upload (`/result/rule/`, HttpUpload) at a test endpoint and capture a real result JSON.

## Blocked on a Windows PC with RRE
5. Join a test server and run a short race with contacts and off-tracks. Capture the result JSON and verify the incident type codes (S-4; the code scheme varies by build).
6. SteamID ↔ RaceRoom `UserId` mapping (D-003 at risk): read `user_id` from shared memory `$R3E` and/or the `game.raceroom.com/users/<id>/?json` endpoint.
7. Ownership detection (D-021/S-5): Steam `GetOwnedGames` with DLC app IDs vs the RaceRoom portal JSON.

## Can be done now (no hardware)
8. Draft the KW Studios email (user said "not yet"; ask first): instance limit, hosting ToS for a matchmaking service, result schema.
9. Resume product grilling on open topics: SR/ELO math and weights (D-005/D-012), D1-vs-Postgres data split (D-025), orchestrator lifecycle, desktop app details.
10. Verify `cut_track_warnings`/`incident_points` against the official `r3e.h` (kwstudios-sweden/r3e-api) when designing desktop-app telemetry. Note D-012 may shrink to heartbeat + cut warnings.

## Working conventions (from the user)
- Ask questions via AskUserQuestion, with the recommended option first.
- Record decisions in `decisions.md` (D-NNN) and spike evidence in `research/`.
- Ask before outward-facing actions (registering with KW, emails).
