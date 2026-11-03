# Vision

## What
Competitive matchmaking service for sim racing. v1: **RaceRoom Racing Experience (RRE) only**; other sims later.

## Ratings
- **ELO**: skill rating from head-to-head results inside a split (D-035). New drivers start at 1500 with a 3× provisional K for their first five rated races (D-014, D-044).
- **Safety Rating (SR)**: 0–100, driven by incident points per corner (D-037, D-038). It measures driving, not conduct: server penalties, no-shows and DNFs do not touch it (D-012, D-040).
- ELO + SR together decide which split a player lands in, weighted toward SR and applied as percentiles within the registered pool (D-004…D-006, D-036).

## Race format (Gran Turismo Sport-style)
- Admin defines **3 races per week** in a headless CMS (Payload): eligible cars, track, rules, interval (how often it runs). A definition locks when its registration opens (D-064).
- Registration runs **T-30 to T-5**. At T-5 the splits are built and each gets its own dedicated server with a 5-minute join window; the race session starts at T-0 (D-016, D-018).
- Registration is capped at host capacity × grid max, so the split build can never exceed the box (D-053).
- One **always-on public practice server** per region mirrors the next scheduled race and is always unranked (D-010, D-056).

## Components
1. **Desktop app** (Tauri): Steam login, race registration, joining, and results. No background service — the app is the launcher and registration surface, and presence comes from the dedicated server's player list (D-058, D-059).
2. **Competition service**: takes the race-end result JSON and the T-5 roster, then calculates SR and ELO. A roster driver who never appears in the result is a classified last place (D-052).
3. **Dedicated server orchestrator**: one prebuilt-image server per split (plus the practice server), created through the server's own REST API, health-checked by polling rather than trusting return codes (D-029, D-032, D-034), and torn down once results are in. One manager runs up to five live servers (D-041); the cloud drives it over Workers VPC (D-026, D-028).
   - Hosting: a single bare-metal box for now, with servers running in Docker containers and one manager per public IP (D-041).
4. **Public website**: driver profiles, results and leaderboards. It shares its UI with the desktop app (Tauri) and reads only through the API (D-020, D-049). API and web run on Cloudflare; Neon Postgres is the only database (D-046).
5. **Ops dashboard**: a separate, read-only app showing live splits and servers (D-024, D-062).
