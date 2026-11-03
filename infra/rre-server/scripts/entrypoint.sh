#!/usr/bin/env bash
# Entrypoint for the RaceRoom dedicated server image.
# Adapted from procinger/r3e-oci (MIT); see THIRD-PARTY.md.
set -Eeuo pipefail

readonly STEAMCMD=/opt/steamcmd/steamcmd.sh
readonly WINE=/opt/proton/files/bin/wine
readonly DOTNET_TEMPLATE=/opt/raceroom/dotnet48-prefix.tar
readonly BAKED_SERVER_DIR=/opt/raceroom/server
readonly MAX_INSTALL_ATTEMPTS=3
readonly SERVER_DIR=${RRE_SERVER_DIR:-/data/server}
readonly SERVER_EXE=${SERVER_EXE:-RRRE_Dedicated.exe}

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

    python3 -c 'import uuid; print(uuid.uuid4().hex)' > "$machine_id_file"
}

prepare_directories() {
    umask 0027
    mkdir -p "$HOME" "$WINEPREFIX" "$SERVER_DIR"
}

install_server() {
    local attempt

    echo 'Installing/updating RaceRoom Dedicated Server (354060)...'

    # A fresh SteamCMD cache can return "Missing configuration" on the first
    # login. Retry, but never continue after an incomplete installation.
    for ((attempt = 1; attempt <= MAX_INSTALL_ATTEMPTS; attempt++)); do
        if "$STEAMCMD" \
            +@ShutdownOnFailedCommand 1 \
            +@NoPromptForPassword 1 \
            +@sSteamCmdForcePlatformType windows \
            +@sSteamCmdForcePlatformBitness 64 \
            +force_install_dir "$SERVER_DIR" \
            +login anonymous \
            +app_info_update 1 \
            +app_update 354060 validate \
            +quit; then
            return 0
        fi

        echo "SteamCMD attempt ${attempt}/${MAX_INSTALL_ATTEMPTS} failed." >&2
    done

    return 1
}

provision_server() {
    # The image bakes the server outside /data so that bind mounts and
    # emptyDir volumes work. A non-empty volume always wins.
    if [[ ! -f "$SERVER_DIR/$SERVER_EXE" && -d "$BAKED_SERVER_DIR" ]]; then
        echo "Provisioning server from $BAKED_SERVER_DIR into $SERVER_DIR"
        mkdir -p "$SERVER_DIR"
        cp -a "$BAKED_SERVER_DIR/." "$SERVER_DIR"/
    fi
}

update_server_if_enabled() {
    case "${UPDATE_ON_START:-false}" in
        true)  install_server ;;
        false) return 0 ;;
        *)
            echo 'UPDATE_ON_START must be true or false.' >&2
            return 1
            ;;
    esac
}

check_server_executable() {
    if [[ ! -f "$SERVER_DIR/$SERVER_EXE" ]]; then
        echo "Missing $SERVER_DIR/$SERVER_EXE. Check the baked server or the SteamCMD logs." >&2
        return 1
    fi
}

link_steam_libraries() {
    local arch library

    # Some Steamworks applications look for client libraries in the home directory.
    mkdir -p "$HOME/.steam/sdk32" "$HOME/.steam/sdk64"

    for arch in 32 64; do
        library="/opt/steamcmd/linux${arch}/steamclient.so"
        if [[ -f "$library" ]]; then
            ln -sf "$library" "$HOME/.steam/sdk${arch}/steamclient.so"
        fi
    done
}

prepare_dotnet_runtime() {
    local marker="$WINEPREFIX/.raceroom-dotnet48-ready"

    if [[ -f "$marker" ]]; then
        return 0
    fi

    # Preserve existing installations and fail rather than overwrite an unknown prefix.
    if [[ -n $(find "$WINEPREFIX" -mindepth 1 -maxdepth 1 -print -quit) ]]; then
        echo "Wine prefix $WINEPREFIX is not empty and has no .NET readiness marker. Use an empty prefix or restore a known working one." >&2
        return 1
    fi

    echo 'Extracting the .NET Framework 4.8 prefix baked into the image...'
    tar --no-same-owner -xf "$DOTNET_TEMPLATE" -C "$WINEPREFIX"
    touch "$marker"
}

check_terminal() {
    # RRE with native .NET requires a console window.
    if [[ ! -t 1 ]]; then
        echo 'RaceRoom with native .NET requires a terminal. Use docker run -t or Kubernetes tty: true.' >&2
        return 1
    fi
}

configure_admin_auth() {
    local password_file=/tmp/raceroom-admin.htpasswd
    local config_file=/tmp/raceroom-admin-auth.conf
    local password_hash temporary_file

    if [[ ! -v ADMIN_USERNAME && ! -v ADMIN_PASSWORD ]]; then
        echo 'auth_basic off;' > "$config_file"
        rm -f "$password_file"
        return 0
    fi

    if [[ -z "${ADMIN_USERNAME:-}" || -z "${ADMIN_PASSWORD:-}" ]]; then
        echo 'Set both ADMIN_USERNAME and ADMIN_PASSWORD to nonempty values.' >&2
        return 1
    fi

    if [[ ! "$ADMIN_USERNAME" =~ ^[a-zA-Z0-9_.@-]+$ ]]; then
        echo 'ADMIN_USERNAME may contain only letters, digits, _, ., @ and -.' >&2
        return 1
    fi

    if [[ "$ADMIN_PASSWORD" == *$'\n'* || "$ADMIN_PASSWORD" == *$'\r'* ]]; then
        echo 'ADMIN_PASSWORD must not contain line breaks.' >&2
        return 1
    fi

    # Keep the plaintext password out of command-line arguments and files.
    password_hash=$(printf '%s' "$ADMIN_PASSWORD" | openssl passwd -6 -stdin)
    temporary_file=$(mktemp /tmp/raceroom-admin.htpasswd.XXXXXX)
    printf '%s:%s\n' "$ADMIN_USERNAME" "$password_hash" > "$temporary_file"
    mv -f "$temporary_file" "$password_file"

    cat > "$config_file" <<'AUTH_CONFIG'
auth_basic "RaceRoom administration";
auth_basic_user_file /tmp/raceroom-admin.htpasswd;
AUTH_CONFIG
}

start_server() {
    cd "$SERVER_DIR"
    echo "Starting $SERVER_EXE with GE-Proton Wine and native .NET; prefix: $WINEPREFIX"

    # The manager's HttpListener only answers Host: localhost:8088. nginx sets it.
    nginx -c /etc/raceroom-nginx.conf

    # Replace this shell so Tini can forward signals to the server process group.
    exec xvfb-run -a -s '-screen 0 1024x768x24 -nolisten tcp' \
        "$WINE" "$PWD/$SERVER_EXE" "$@"
}

main() {
    check_terminal
    prepare_machine_id
    prepare_directories
    configure_admin_auth
    provision_server
    update_server_if_enabled
    check_server_executable
    link_steam_libraries
    prepare_dotnet_runtime
    start_server "$@"
}

# Allow the image build to reuse the helpers without starting the server.
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
