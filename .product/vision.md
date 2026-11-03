# Vision

## What
Competitive matchmaking service for sim racing. v1: **RaceRoom Racing Experience (RRE) only**; other sims later.

## Ratings
- **ELO**: skill rating used to rank and match players.
- **Safety Rating (SR)**: how often players crash and how safely they drive.
- ELO + SR together decide which split a player lands in.

## Race format (Gran Turismo Sport-style)
- Admin defines **3 races per week** in a headless CMS (Payload, tentatively): eligible cars, track, rules, interval (how often it runs).
- Registration opens **30 min before** each race start.
- At start, registered players are divided into **splits** by ELO + SR. Each split = its own dedicated server, running in parallel.
- Each race also has a **public practice server**: same rules, 1 hour of free practice only.

## Components
1. **Desktop app**: login, race registration, and a background service that watches the game process + RRE shared memory API and reports incidents (crashes, off-tracks) to the competition service.
2. **Competition service**: takes real-time events from the desktop app and the race-end JSON from the dedicated server, then calculates SR and ELO.
3. **Dedicated server orchestrator**: spawns one server per split (plus practice servers), watches race start/end (ideally via a server plugin), uploads results to the competition service, and tears servers down.
   - Hosting: a single bare-metal box for now, with servers running in Docker/Kubernetes containers.
4. **Public website**: driver profiles, results and leaderboards. It shares its UI with the desktop app (Tauri). API and web run on Cloudflare.
5. **Ops dashboard**: a separate app showing live splits and servers.
