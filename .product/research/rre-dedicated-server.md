# S-1..S-5: RRE dedicated server (research, 2026-11-03)

| Spike | Verdict |
|---|---|
| S-1 Wine/Docker | **Likely, partial.** A community image, [procinger/r3e-oci](https://github.com/procinger/r3e-oci), runs Steam app 354060 under Proton-Wine + .NET 4.8 + Xvfb. No one reports it working. The first run may need an interactive "secret phrase" registration. |
| S-2 5-server limit | **Unverified.** The only source is a secondhand PCGamingWiki snippet, which says "5 per **IP and account**", not per install. If that's right, multiple containers on one IP won't get around it; a separate account and/or IP per 5 servers might. Ports: server 1 uses UDP 60000, TCP 60001, UDP 60002, and each additional instance needs more ports. Whether a Steam account is needed and whether hosting is allowed under the ToS: unknown. |
| S-3 Hooks/API | **No plugin/API found.** There is a web admin UI, and the server **pushes result JSON/XML to an HTTP/FTP URL** at race end (2016 docs). The wiki mentions a "Server API" but doesn't document it. Setting sessions, cars or tracks programmatically: unknown. Raceroom Commander (3rd party) adds kick/penalty chat commands by some unknown mechanism. |
| S-4 Result JSON | **Likely rich.** Each player has `UserId` (RaceRoom ID, **not SteamID**), `Position`, `FinishStatus`, laps with `Incidents[]` of `{Type, Points, OtherUserId}`. Incident types (scheme varies by build): 0 car contact, 1 object contact, 2 wrong way, 3 off track, 4 stationary, 5 loss of control, 6 invalid lap, 7 penalty not served, 8 disconnect. No sample file found. |
| S-5 Client | Shared memory exposes `incident_points`, `cut_track_warnings`, penalties and damage, but **no contact/spin/off-track events and no ownership data**. Ownership candidates: Steam `GetOwnedGames` (DLC coverage unknown; needs a public profile), `game.raceroom.com/users/<id>/?json` (schema unknown). |

Sources: kwstudios-sweden/r3e-api (r3e.h v3.6), r3e-spectator-overlay README, Koenvh1/raceroom-commander README, raceroom.miraheze.org Dedicated_Server, mauserrifle/simresults RaceRoomServer.php, Steam community threads (2016–2022), Steamworks IPlayerService.

Next: run r3e-oci on the x86 Colima profile with a test Steam account. A short race should produce a sample result JSON. Ask KW Studios in writing about the instance limit and hosting ToS.

## Hands-on log (2026-11-03, Colima x86_64 VM)
- `steamcmd +login anonymous +app_info_print 354060`: works. App = "RaceRoom Dedicated Server", Tool, `freetodownload=1`, Windows-only, 64-bit, exe `RRRE_dedicated.exe`, public build 26300285.
- `app_update 354060` anonymously: fails. Without forcing the platform it errors `Invalid platform`; with `+@sSteamCmdForcePlatformType windows` it errors `Missing configuration`. **Anonymous download isn't possible.** It needs a logged-in Steam account; because of `freetodownload`, a free account probably works.
- For D-029 (prebuilt image), that means a build-time Steam account (a CI secret), with Steam Guard in mind.
- Anonymous fallbacks tried, all confirmed as `Connecting anonymously to Steam Public...OK`:
  - `+app_license_request 354060` → **"Not for anonymous users."**
  - `+download_depot 354060 354061` (main depot; the others are shared redists from 228980) → `Missing configuration`
  - `app_update` with/without a forced Windows platform, `-beta public` → `Missing configuration`
  - **Conclusion:** `freetodownload` means a free license is auto-granted to *logged-in* accounts. The anonymous account can't hold it, so a real (free) Steam account is required.

### S-1 result: PASS (2026-11-03)
- Ran `ghcr.io/procinger/r3e-oci:v0.1.4` on Colima x86_64/QEMU with a fresh volume. **The anonymous SteamCMD install succeeded:** the first attempt returned `Missing configuration`, the retry printed `Success! App '354060' fully installed`.
  - **This corrects the earlier "anonymous impossible" conclusion.** The stock `steamcmd/steamcmd` image never succeeded, but the image's own flow (Linux steamcmd under a persisted HOME, `+app_info_update 1`, retry) works. Exact cause not isolated; the image's flow is proven, so use it.
- `RRRE_Dedicated.exe` starts under GE-Proton Wine + .NET 4.8 + Xvfb. Log: "Launching Milkyway workers / scheduler / Starting frontend on http://localhost:8088/". The web UI returned **HTTP 200 after 137 s** (emulated, so real x86 will be faster).
- Not yet tested: an actual race session with a client connected (needs a Windows RRE client).

### S-2 result: registration PASS, live servers untested (2026-11-03)
- The admin UI is a thin client over a **JSON REST API** on loopback :8088 (ServiceStack-style): `/dedi` (GET/POST config), `/dedi/register`, `/dedi/default`, `/session/`, `/result/`, `/result/rule/`, `/user/`, `/positions`, `/chat/`, `/ban`, `/motd/`, `/weight/`, `/power-restriction/`, `/spectator-whitelist`, `/content/all`, `/content/meta`. Endpoints were extracted from `ui.min.js`. **This is our control API for S-3.**
- The instance quota comes from KW's backend: `POST /dedi/register {"Secret": "..."}` → `{"Ip": "<public ip>", "Quota": 5}`. The UI describes the secret as "a unique phrase to differentiate yourself from other potential servers on the same public IP".
- Test: container A with secret A got Quota 5. Container B (own volume, own secret, same public IP) **also got Quota 5**, and A stayed registered. ⇒ **The quota is per (public IP, secret), so one container per secret gets around the limit of 5.**
- Caveats:
  - The test ran behind **shared carrier NAT** (178.25.17.150), and the result may differ on a dedicated IP.
  - Not yet tested: running more than 5 *live* servers at once and having them all appear in the RRE server list or be joinable.
  - Port advertising: each container uses 60000–60002 internally, mapped to different host ports. Does the server advertise its internal port to the master server? If so, it needs host networking with a configured port per container. → **S-2b**

### S-3 result: full REST control (2026-11-03)
Everything below was done with curl and no UI:
1. `GET /dedi/default` returns a full `GameSetting` template.
2. `POST /dedi/ {"GameSettings": {...}}` creates the server and returns `{"ProcessId": N}`.
3. `POST /dedi/start {"ProcessId": N}` returns `{"IsStarted": true}`; it was Running within 10 s.
4. `GET /dedi` returns a `ProcessState` per instance: `Running, Port, TimePort, PlayersOnServer, TimeLeft, SessionTime, SessionTimeLeft, CurrentSession, Players`. **Poll this for session transitions (D-016/D-018).**
5. `POST /session/{id} {"Command": 1}` restarts the session; `{"Command": 2}` goes to the next session. `POST /dedi/stop {ProcessId}` and `DELETE /dedi/{id}` stop and remove it.
6. Results: `GET /result/`, and per-instance upload rules (`/result/rule/`, HttpUpload/FTP `UploadTarget`). Point the upload at the orchestrator for the race-end signal.

`GameSetting` fields relevant to us: `MaxIncidentPoints`, `CutRules`, `FlagRules`, `Damage`, `MinRating/MaxRating`, `MinReputation/MaxReputation`, `RankedServer`, `MaxPing`, `FixedSetup`, `MandatoryPitstop`, `PreventFalseStart`, `AllowPlayersJoinInQualifying`, `KickNonReadyClientsTime`, `StateReportInterval`, `LiveUpdate`, `Schedule*`, plus `TrackLayouts[].Sessions` (Practice/Qualify/Race1-3 duration or laps, TimeOfDay, WarmupDuration) and `Liveries[]` (= eligible cars). Bans are set via `POST/DELETE /ban {UserId}`. The full template is saved in the scratchpad (default.json).

### Architecture finding: two-level process
- `RRRE_Dedicated.exe` is only the **manager** (.NET, REST API on :8088).
- On first start it **extracts the actual game server**, which is bundled inside the 189 MB exe (manager log: "Extracting game files"; `DedicatedServer.exe`, game build 112.0.1733.0, ~109 MB), into `%LOCALAPPDATA%\Sector3_Studios_AB\bin\<guid>\<build>\Game\` and launches it with CLI args (`-port 60000 -timePort 60002 -password … -trackLayoutID … -carLiveryID … -practice1Minutes … -maxPlayers …`).
- **For D-029, run the manager once at image build time so the extraction is baked into the image.** No network download happens at runtime. Game updates arrive via Steam (app 354060), so rebuilding the image picks them up.
- Stability: one launch hiccup (`WARN … pipe still connecting`), then the same PID stayed up for more than 2.5 min with no crash dumps. The game logs (`Log_Dedicated/*/Game.log`) are encrypted/binary, so they're no use for monitoring. Use `GET /dedi` ProcessState and the result upload instead.
- The manager picks the ports and passes them as arguments. For S-2b, check whether it can be told which ports to use, so that host networking works without per-container NAT.

### S-2b result: one live server per container under Wine (2026-11-03)
- Created a-2…a-6 in container A next to the running instance. a-2 and a-3 returned `IsStarted: true`, a-4…a-6 returned `IsStarted: false`.
- **Every spawned `DedicatedServer.exe` got `-port 60000 -timePort 60002`.** The manager logs `Process with such port is in dictionary already!` and the duplicates crash-loop. Its port allocation doesn't work under Wine (likely it uses a Windows port-table API that Wine implements differently; untested on Windows).
- ⇒ **One live server per container.** Each container's server listens on 60000/60002 internally.
- Consequence: the server probably advertises port 60000 to RaceRoom's master server, so remapping host ports (60010→60000) may break discovery and joining. Options for production:
  a) one host IP per container (additional IPs + macvlan/ipvlan), with all containers on 60000;
  b) launch `DedicatedServer.exe` directly with our own `-port`, bypassing the manager's allocation (but the manager relays state to the KW backend via SlaveService, so that needs testing);
  c) find a manager setting for the base port.
- Can't verify locally: public listing and joining. The home firewall doesn't expose the ports and there's no Windows client.
- Still untested: whether one secret shared by more than 5 containers hits the quota. This decides whether D-031 (a secret per container) is required or just recommended.

### Shared-secret quota test: INCONCLUSIVE (2026-11-03)
- 6 containers registered with **the same** secret; each got `{"Ip": "178.25.17.150", "Quota": 5}`. Registration doesn't reject reuse.
- Every server start failed, both in parallel and one at a time (`pipe still connecting` → `OperationCanceledException`), including the first container, so the quota isn't the cause.
- Cause: **CPU starvation**. VM load average 26 on 4 vCPUs, and each *idle* manager uses ~65 % CPU under QEMU x86 emulation. This Mac can't run 6 live servers at once.
- ⇒ Redo on real x86 hardware. Also measure the idle manager's CPU use natively. If it's still high, the per-container overhead (one manager per server, D-033) becomes a real **capacity cost** on the box.

### Prebuilt image (D-029) built and verified (2026-11-03)
- `infra/rre-server/Dockerfile` extends `procinger/r3e-oci` pinned by digest. At build time it runs the base entrypoint's own `install_server` + `prepare_dotnet_runtime` and sets `UPDATE_ON_START=false`.
- Size: 10.7 GB total, of which 8.14 GB is the base and +1.87 GB our layer (server + unpacked prefix). The base layers are shared, so the per-container disk cost is small on one host.
- Emulated timings: cold boot to web UI in **49 s** (was 137 s with runtime SteamCMD). Register → create → start → `Running` in **21 s**.
- Still at runtime: the manager unpacks the game server (~15 s emulated). Pre-baking that is deferred until T-5 spin-up proves too slow on real hardware.
