#!/bin/bash
# Runs inside the chroot as /data/start.sh. Mirrors the r3e-oci entrypoint's
# environment, but starts the manager directly (no nginx) for local testing.
set -euo pipefail
export HOME=/data/home
export WINEPREFIX=/data/dotnet48
export SteamAppId=354060
export SteamGameId=354060
export STEAM_COMPAT_CLIENT_INSTALL_PATH=/opt/steamcmd
export STEAM_COMPAT_DATA_PATH=/data/compatdata
export PROTON_USE_WINED3D=1
export WINEDEBUG=-all
export WINEDLLOVERRIDES=mscoree=n,b
export PATH=/usr/local/bin:/usr/bin:/bin
mkdir -p "$HOME" /data/compatdata
cd /data/server
exec xvfb-run -a -s "-screen 0 1024x768x24 -nolisten tcp" \
  /opt/proton/files/bin/wine /data/server/RRRE_Dedicated.exe
