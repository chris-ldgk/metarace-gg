#!/usr/bin/env bash
# Entrypoint for the Metarace RaceRoom image.
#
# The Rust manager runs natively. Only DedicatedServer.exe runs under Wine, so
# this script prepares the machine id and the baked Wine prefix, then starts
# the manager under Xvfb (Wine still initialises user32/gdi32).
set -Eeuo pipefail

readonly WINE_PREFIX_TEMPLATE=/opt/wineprefix.tar
readonly PREFIX=${WINEPREFIX:-/data/wineprefix}
readonly MANAGER=${METARACE_MANAGER:-/usr/local/bin/metarace-rre-manager}

prepare_machine_id() {
    local machine_id_file=/etc/machine-id
    local current_id=""

    if [[ -r "$machine_id_file" ]]; then
        current_id=$(< "$machine_id_file")
    fi

    # Keep valid IDs, including IDs supplied through a read-only mount.
    if [[ "$current_id" =~ ^[0-9a-f]{32}$ && "$current_id" != 00000000000000000000000000000000 ]]; then
        return 0
    fi

    tr -d '-' < /proc/sys/kernel/random/uuid > "$machine_id_file"
}

prepare_directories() {
    umask 0027
    mkdir -p "$HOME" "$PREFIX" /data/results /data/logs
}

prepare_wine_prefix() {
    local marker="$PREFIX/.metarace-wine-prefix-ready"

    if [[ -f "$marker" ]]; then
        return 0
    fi

    if [[ -n $(find "$PREFIX" -mindepth 1 -maxdepth 1 -print -quit) ]]; then
        echo "Wine prefix $PREFIX is not empty and has no readiness marker; using it as-is." >&2
        touch "$marker"
        return 0
    fi

    echo 'Extracting the baked Wine prefix...'
    tar --no-same-owner -xf "$WINE_PREFIX_TEMPLATE" -C "$PREFIX"
    touch "$marker"
}

main() {
    prepare_machine_id
    prepare_directories
    prepare_wine_prefix

    # Xvfb is kept until the game server is proven to run without a display;
    # it is only used by Wine, the manager itself is headless.
    exec xvfb-run -a -s '-screen 0 1024x768x24 -nolisten tcp' "$MANAGER" "$@"
}

main "$@"
