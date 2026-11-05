# metarace-rre-manager

Native Linux manager for the RaceRoom dedicated server. It replaces the
Windows `RRRE_Dedicated.exe` manager (D-081), registers a slave with the
RaceRoom portal and announces the server to the browser (D-087), and runs one
`DedicatedServer.exe` under Wine per container (D-083). It is derived from the
`rrre-dedi-rs` Rust port and pruned to the metarace feature set.

The build embeds the proprietary `CommunityMultiplayer.Game.game.zip` extracted
from `RRRE_Dedicated.exe`; the game server itself is not replaced and its files
are never committed. At runtime the payload is extracted into the data
directory and launched with the argument vector the original manager generated.

## Build

Supply the original manager either at `data/RRRE_Dedicated.exe` or through
`METARACE_DEDICATED_EXE`:

```bash
METARACE_DEDICATED_EXE=/path/to/RRRE_Dedicated.exe cargo build --release
```

On a mounted Windows filesystem (WSL `/mnt/c`), use a Linux-local target
directory:

```bash
CARGO_TARGET_DIR="$HOME/.cache/metarace-rre-target" \
METARACE_DEDICATED_EXE=/path/to/RRRE_Dedicated.exe \
cargo build --release
```

## Test

```bash
METARACE_DEDICATED_EXE=/path/to/RRRE_Dedicated.exe \
CARGO_TARGET_DIR="$HOME/.cache/metarace-rre-target" \
cargo test
```

The suite covers the crypto envelope against C# fixtures, the argument builder,
settings and payload extraction, state and result handling, the bearer-token
gate, and the result push (including headers and failure behaviour).

## Run

Portal mode (the production path):

```bash
METARACE_SETTINGS_FILE=./settings.json \
METARACE_PORTAL_SECRET='unique-per-container' \
METARACE_CONTROL_TOKEN='...' \
METARACE_GAME_LAUNCHER=wine \
METARACE_WINE_PREFIX=/tmp/metarace-prefix \
METARACE_BIND=0.0.0.0:8088 \
./target/release/metarace-rre-manager
```

Local mode (no portal, not joinable from the browser):

```bash
METARACE_SETTINGS_FILE=./settings.json \
METARACE_PORTAL_DISABLED=true \
METARACE_GAME_DIR=/path/to/extracted/game \
METARACE_GAME_LAUNCHER=wine \
METARACE_WINE_PREFIX=/tmp/metarace-prefix \
METARACE_BIND=127.0.0.1:8088 \
METARACE_ALLOW_INSECURE=true \
./target/release/metarace-rre-manager
```

See [API.md](API.md) for the routes, the environment table and the result push
contract.

## Notes

- One server per process. A second start is refused, the game port comes from
  the portal (`GetPort`), and a crash is never restarted (D-083).
- No SQLite, no scheduler, no UI, no track rotation and no named-pipe relay
  (D-085).
- The manager keeps running after the game server exits so the orchestrator can
  read `/state` and pull `/results`.
- The manager passes the auto-detected container IPv4 as `-ip` (D-088).
  Override with `METARACE_DEDI_IP` if the container has several addresses; a
  game server without an IPv4 bind is unreachable over the Docker/gateway path.
- The settings payload must be the complete `MultiplayerServerSettings`
  (D-090); a trimmed object is rejected by the portal.
- `StayInPracticeSessionIfEmpty = 1` (`-sessionFreeze 1`) keeps an empty server
  joinable for tests/practice; a ranked split needs the configured start timing.
