# Open Questions

## Spikes (blocking)
- **S-1** Does the RRE dedicated server run reliably under Wine in a Linux container? (D-019)
- **S-2** Can we get around the limit of 5 servers per instance by running one container per instance? Check port allocation, Steam auth and licensing implications. (D-019)
- **S-3** Does the RRE dedicated server expose plugins or event hooks? Otherwise, how do we detect session transitions (practice → qualifying → race → end)?
- **S-4** What incident and contact data does the RRE result JSON actually contain? This decides how much of D-012 the server can supply.

## Product
- **S-5** Can the desktop app reliably detect owned RRE content (cars and tracks)? (D-021)
- SR/ELO weights and the incident point values (D-005, D-012) need tuning.
- **S-6** Workers VPC: confirm it can reach a bare-metal host (via cloudflared tunnel) and what latency and limits apply. (D-026)
- Split DB: which data counts as "auth-related" for D1 versus Postgres? Draw the line before the schema work. (D-025)

## Spike setup (2026-11-03)
- S-1/S-2 are tested locally with Colima + QEMU (x86_64 emulation on macOS). Performance numbers there are **not** representative; this only tests viability. Re-check performance on real x86 hardware before committing.
- S-5 and the client side of S-4 are research-only until a Windows machine with RRE is available.

## Research impact (2026-11-03, see research/)
- **D-003 at risk:** the result JSON identifies drivers by RaceRoom `UserId`, not SteamID. We need a way to map SteamID → RaceRoom UserId (have the desktop app read `user_id` from shared memory? link through the RaceRoom portal?).
- **D-012 may simplify:** the server reports off-track (3) and loss of control (5) itself. Shared memory has no contact/spin events, so the client's SR role may shrink to heartbeat + `cut_track_warnings`.
- **D-021 at risk:** there's no ownership data in shared memory. Ownership needs Steam GetOwnedGames (requires a public profile) or the RaceRoom portal JSON.
- **S-2 at risk:** the limit may apply per IP + account. If so, getting more than 5 servers on one box needs several accounts and/or IPs. Ask KW Studios.
- **S-3:** the result push to an HTTP URL can be the race-end signal (point it at the orchestrator). Session-start detection is still unknown.

## Spike status (2026-11-03)
- **S-1 PASS:** runs under Wine in Docker, anonymous SteamCMD works (see research/rre-dedicated-server.md)
- **S-2 PASS at registration level (secret reuse: inconclusive locally, CPU-starved):** the quota of 5 applies per (IP, secret), and a second container with its own secret got its own 5. → S-2b open: are more than 5 live servers listed and joinable, can ports be configured for host networking, and does it behave the same on a dedicated IP rather than carrier NAT?
- **S-3 PASS:** full REST control of create/start/stop/session/bans/results.
- **S-4 open:** needs a real race with a client to get a sample result JSON (needs a Windows RRE client).
- **S-5 open:** research only. There's no ownership data in shared memory.
- **S-6 decided:** D-028.
- **D-029 image: built + verified** (`infra/rre-server/`).

## Needs the production box (real x86 + open ports)
- S-2b: idle manager CPU cost natively; >5 live servers (shared vs per-container secret); host-port remap vs macvlan for listing/joining (D-033)
## Needs a Windows RRE client
- Join a server; get a sample result JSON (S-4); SteamID ↔ RaceRoom UserId mapping (D-003); ownership detection (S-5)
