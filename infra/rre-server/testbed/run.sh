#!/bin/bash
# Run the prebuilt RRE dedicated manager on Linux without Docker: chroot into the
# extracted r3e-oci rootfs (GE-Proton 11 + dotnet48 prefix) as an unprivileged user.
#
# Prerequisites (see README.md):
#   $RRE_ROOT  extracted r3e-oci image layers (has opt/proton and opt/raceroom)
#   $RRE_DATA  dotnet48 prefix + server exe, mounted at /data inside the chroot
set -euo pipefail
ROOT=${RRE_ROOT:-$HOME/r3e-root}
DATA=${RRE_DATA:-$HOME/r3e-data}

[ -x "$ROOT/opt/proton/files/bin/wine" ] || { echo "missing GE-Proton in $ROOT" >&2; exit 1; }
[ -f "$DATA/dotnet48/system.reg" ] || { echo "missing dotnet48 prefix in $DATA/dotnet48" >&2; exit 1; }
[ -f "$DATA/server/RRRE_Dedicated.exe" ] || { echo "missing server exe in $DATA/server" >&2; exit 1; }

if ! grep -qE '^[0-9a-f]{32}$' "$ROOT/etc/machine-id" 2>/dev/null; then
  python3 -c 'import uuid; print(uuid.uuid4().hex)' > "$ROOT/etc/machine-id"
fi

mkdir -p "$ROOT/proc" "$ROOT/dev" "$ROOT/sys" "$ROOT/data"
unshare -r --mount --pid --fork --uts --ipc bash -c '
  R="$1"; D="$2"
  mount -t proc proc "$R/proc" 2>/dev/null || true
  mount --rbind /dev "$R/dev" 2>/dev/null || true
  mount --rbind /sys "$R/sys" 2>/dev/null || true
  mount --bind /etc/resolv.conf "$R/etc/resolv.conf" 2>/dev/null || true
  mount --bind /etc/hosts "$R/etc/hosts" 2>/dev/null || true
  mount --bind "$D" "$R/data"
  exec chroot "$R" /usr/bin/script -qefc "/data/start.sh" /dev/null
' _ "$ROOT" "$DATA"
