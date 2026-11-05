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
- One **always-on practice server** per region mirrors the next scheduled race and is always unranked (D-010, D-056); it is listed in the server browser like a split (D-087).

## Components
1. **Desktop app** (Tauri): Steam login, race registration, joining, and results. No background service — the app is the launcher and registration surface, and presence comes from the dedicated server's player list (D-058, D-059).
2. **Competition service**: takes the race-end result JSON and the T-5 roster, then calculates SR and ELO. A roster driver who never appears in the result is a classified last place (D-052).
3. **Dedicated server orchestrator**: one throwaway container per split on the prebuilt image, each running one Rust manager and one game server, created at T-5 and destroyed once results are in (D-081…D-083). The manager registers a slave with the KW portal and announces the server, so players join through the **in-game server browser** with the per-split password (D-087); health is polled from the manager (`/state`), never trusted from a start return code (D-034). The cloud drives the host over Tunnel + Access (D-079).
   - Hosting: a single bare-metal box for now, with one container per split; servers are listed through the KW portal and joined from the server browser (D-041, D-083, D-087).
4. **Public website**: driver profiles, results and leaderboards. It shares its UI with the desktop app (Tauri) and reads only through the API (D-020, D-049). API and web run on Cloudflare; Neon Postgres is the only database (D-046).
5. **Ops dashboard**: a separate, read-only app showing live splits and servers (D-024, D-062).
