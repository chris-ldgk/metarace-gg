# Open Questions

## Spikes (blocking)
- **S-1** Does the RRE dedicated server run reliably under Wine in a Linux container? (D-019)
- **S-2** Can we get around the limit of 5 servers per instance by running one container per instance? Check port allocation, Steam auth and licensing implications. (D-019)
- **S-3** Does the RRE dedicated server expose plugins or event hooks? Otherwise, how do we detect session transitions (practice → qualifying → race → end)?
- **S-4** What incident and contact data does the RRE result JSON actually contain? This decides how much of D-012 the server can supply.

## Product
- **S-5** Can the desktop app reliably detect owned RRE content (cars and tracks)? (D-021)
- **Ratings tuning (v0 in D-038):** the numbers are a starting point, to be recalibrated on real result JSONs once S-4 lands. Two known weaknesses:
  - `target = 0.08` makes an average race SR-positive, so the population drifts toward the 100 ceiling instead of spreading out. First knobs to turn are the target, then the negative slope.
  - The percentile split score needs a tie-break: every newcomer starts at SR 50 / ELO 1500 (D-014), so the first week's splits are all ties. Suggest ELO, then registration time.
- **ELO parameter tuning (D-043…D-045):** base `K = 32`, provisional 3× for the first 5 rated races, `K` scaled by √n with a 0.5×–2× clamp. Recheck after a season; the two things most likely to feel wrong are the hard 3× step at race 6 and the 2× multiplier in a 20-car split.
- **Crash-and-quit SR hole (D-040):** accepted for now, no automatic mitigation. Revisit if it shows up in practice.
- **S-6** Workers VPC: confirm it can reach a bare-metal host (via cloudflared tunnel) and what latency and limits apply. (D-026)
- **Split DB resolved (D-046…D-049):** no D1. Neon Postgres via Hyperdrive is the only database; a Durable Object per race owns registration and the T-5 build; R2 keeps the raw result JSON; the website reads via the API. Remaining details:
  - **Roster visibility during the window (D-047):** the site must show who is registered while the roster lives in the DO — decide projection-to-Postgres vs a DO read endpoint before the UI work.
  - **Result ingest idempotency:** the server's HTTP upload can retry, so scoring must be keyed by (race, split, driver, result hash) and never applied twice.
  - **Payload 3 on Workers + Postgres/Hyperdrive:** needs verification before it becomes a build dependency; the D1 adapter path is the better-documented one we just rejected.
  - **R2 retention and PII:** raw result JSON carries Steam/`UserId` identifiers; decide retention and what an account-deletion request removes.
- **Orchestrator edge cases (D-050…D-057):**
  - **The capacity number in D-053** depends on how many concurrent servers the production box holds — blocked on the load test.
  - **Practice rotation (D-056):** replacing the practice server when the next race definition changes — decide whether it swaps immediately (kicking anyone on it) or only once empty, and how the new layout is announced.
  - **Voided races are hidden publicly (D-057)** but must stay visible to admins and to the affected drivers' internal record; confirm where admins see them.
  - **Coordinated no-shows (D-054):** voiding leaves ratings untouched, so repeat no-show groups rely on admin action. Watch for it as the pool grows.
- **Desktop app follow-ups (D-058…D-061):**
  - **D-060's "unknown" branch needs a mechanism (S-5):** Steam `GetOwnedGames` returns nothing for private profiles, so decide whether the RaceRoom portal JSON becomes the primary check and how "unknown" is distinguished from "not owned".
  - **Minimum version policy (D-061):** how `minVersion` is chosen (e.g. N−1 minor) and how an urgent rollback reaches players when a release is bad.
  - **Per-split join password:** the app must deliver it to roster players only; confirm `/dedi` exposes it so the orchestrator can read it back and distribute it.
  - **Flag queue scope (D-006 vs D-058):** with the heartbeat offence gone, the queue is admin-initiated — confirm it still covers D-017 appeals and manual reports.
- **Admin/ops follow-ups (D-062…D-065):**
  - **Kick path:** the dashboard is read-only (D-062), so a race director's kick travels our admin UI → API → orchestrator over the Access-protected tunnel (D-079). Confirm the API exposes it to that role and that each call produces an audit record.
  - **Audit trail:** every rating-affecting action (void, re-score, ban, flag resolution) needs a record in our own Postgres — decide the table shape and whether it is append-only like the rating events.
  - **Pre-race rating snapshot (D-065):** store per-split ratings at T-5, or reconstruct them exactly from the event log.
  - **Role enforcement:** the API must re-check Better Auth roles on every action rather than trusting any client UI.
- **Stack follow-ups (D-066…D-080):**
  - **Better Auth + Steam OpenID 2.0 (D-074):** the provider plugin is a custom fit against an OAuth2/OIDC-shaped API. Spike it before any app work; the fallback is our own callback route minting the session.
  - **Definition freeze failure (D-077):** if the Payload read fails at T-30, retry until T-5 — decide the last resort (cancel the occurrence, or run it from the last definition we could read).
  - **Payload custom auth strategy (D-078):** prove Payload accepts a Better Auth session before the admin work starts — either a shared cookie scope across subdomains or a bearer token passed to the admin.
  - **Control path details (D-079):** service-token rotation (50 per account), where host-selection data lives (KV vs Postgres), and a hands-on check that the Access-protected hostname reaches the orchestrator on the real box.
  - **Hyperdrive cache check (D-080):** confirm `max_age = 0` is set on every Hyperdrive config (API and Payload), and that a rating written after a race is read back fresh through the API.

## Spike setup (2026-11-03)
- The original S-1/S-2 runs used Colima + QEMU on macOS; those performance numbers are not representative, and their multi-server conclusion was an emulation artifact.
- New real-x86 testbed: WSL2 Debian on the Windows PC (24 cores, 15 GiB). The `r3e-oci` image layers are extracted to `~/r3e-root` (5.7 GB) and run in an unprivileged chroot with GE-Proton 11 + the image's dotnet48 prefix (`~/r3e-data`); `~/rre-server` holds the SteamCMD install. See `research/rre-dedicated-server.md`; the API helper is `infra/rre-server/testbed/dedi.py`.
- RRE (app 211500) is installed on this Windows PC, so the client exists for S-4/S-5; the blocker is reaching the server from it, not the client itself.

## Research impact (2026-11-03, see research/)
- **D-003 at risk:** the result JSON identifies drivers by RaceRoom `UserId`, not SteamID. We need a way to map SteamID → RaceRoom UserId (have the desktop app read `user_id` from shared memory? link through the RaceRoom portal?).
- **D-021 at risk:** there's no ownership data in shared memory. Ownership needs Steam GetOwnedGames (requires a public profile) or the RaceRoom portal JSON.
- **D-012 is now D-039:** SR uses the server result JSON only; the desktop app carries no telemetry role at all (D-039, D-058). Incident codes 0–9 are confirmed from the UI, and the server's reputation keys are known, but the exact point values are encrypted in `ReputationSystem.gdb` (need a runtime dump or a real result JSON).
- **S-2 resolved:** the limit is per (public IP, secret) and applies to *live* servers (5). One manager runs 5 concurrently; a 6th start is rejected with “Quota exceeded”. More than 5 needs more containers/secrets, and each host still needs a separate IP because every manager starts at port 60000 (D-041).
- **S-3:** the result push to an HTTP URL can be the race-end signal (point it at the orchestrator). Session-start detection is still unknown; a server with no clients loops practice and never advances.

## Spike status (2026-11-03)
- **S-1 PASS:** runs under Wine in Docker, anonymous SteamCMD works (see research/rre-dedicated-server.md)
- **S-2 PASS, including S-2b on real x86:** the quota of 5 applies per (public IP, secret) to *live* servers; one manager ran 5 concurrently (ports 60000, 60003, 60006, 60009, 60012), and a 6th start returned `303 {"Message":"Quota exceeded!"}` while creation still succeeded. Ports are monotonic and reset on manager restart. The earlier “one live server per container” was a QEMU/CPU-starvation artifact (D-041).
- **S-3 PASS:** full REST control of create/start/stop/session/bans/results.
- **S-4 open:** needs a real race with a client to get a sample result JSON. Incident codes 0–9 are confirmed from the manager UI, and zero-player servers loop practice without producing a result.
- **S-5 open:** research only. There's no ownership data in shared memory.
- **S-6 decided:** D-028.
- **Image (D-029, superseded by D-075):** the old derivative image was built and verified; the new self-contained multi-stage image still needs its first build on a Docker host.
- **Manager caveats (D-042):** starting a server persisted from a previous manager run crashes the manager (`AccessViolationException` after unimplemented `httpapi.HttpCancelHttpRequest`); delete persisted IDs on boot and recreate. Killing a game server makes the manager auto-restart it on the same port, and `/dedi` is briefly stale (D-034).

## Needs the production box (public IP + open ports)
- Listing and joining with host-port remapping; if that fails, one IP per container (macvlan/ipvlan). Every manager starts allocating at 60000, so multiple containers need separate IPs or a base-port mechanism (D-041).
- Load test with real clients: idle cost is measured (~0.17 core / ~110 MB per server), loaded cost is not.
- Re-measure idle cost on the real host CPU (the WSL numbers are from a desktop CPU).
## Needs a Windows RRE client (installed on this PC)
- Join a test server by direct IP and run a short race with contacts and off-tracks; capture the result JSON (S-4). The server and client are on the same machine, so the WSL IP is the first thing to try.
- SteamID ↔ RaceRoom UserId mapping (D-003); ownership detection (S-5).
