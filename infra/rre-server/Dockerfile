# syntax=docker/dockerfile:1
# Metarace RaceRoom dedicated-server image.
#
# The Windows manager (RRRE_Dedicated.exe) is replaced by the Rust manager in
# manager/. The proprietary game payload is extracted from the manager at build
# time and embedded into the Rust binary; only DedicatedServer.exe runs under
# Wine at runtime.
#
# Build context: infra/rre-server
ARG PROTON_VERSION=GE-Proton11-11
ARG STEAMCMD_URL=https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz

###### server
# Fetch the RaceRoom dedicated server for its embedded game payload. Nothing
# from this stage except RRRE_Dedicated.exe reaches the final image.
FROM ubuntu:26.04@sha256:6a06111c3b0a8600feadb270aec771dc239bf407b25625f93f44ab00e11f4d3c AS server
ARG STEAMCMD_URL
SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# hadolint ignore=DL3008
RUN set -eu \
    && export DEBIAN_FRONTEND=noninteractive \
    && dpkg --add-architecture i386 \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        lib32gcc-s1 \
        lib32stdc++6 \
    && rm -rf /var/lib/apt/lists/*

RUN set -eu \
    && mkdir -p /opt/steamcmd \
    && curl -fsSL --retry 3 "$STEAMCMD_URL" | tar -xz -C /opt/steamcmd

RUN set -eu \
    && userdel -r ubuntu 2>/dev/null || true \
    && useradd --uid 1000 --create-home --shell /bin/bash steam \
    && mkdir -p /server \
    && chown -R steam:steam /opt/steamcmd /server

USER steam
ENV HOME=/home/steam

# A fresh SteamCMD cache can answer "Missing configuration" on the first
# login; retry, but never continue after an incomplete installation.
RUN set -eu \
    && for attempt in 1 2 3; do \
         if /opt/steamcmd/steamcmd.sh \
              +@ShutdownOnFailedCommand 1 \
              +@NoPromptForPassword 1 \
              +@sSteamCmdForcePlatformType windows \
              +@sSteamCmdForcePlatformBitness 64 \
              +force_install_dir /server \
              +login anonymous \
              +app_info_update 1 \
              +app_update 354060 validate \
              +quit; then \
           test -f /server/RRRE_Dedicated.exe && exit 0; \
         fi; \
         echo "SteamCMD attempt ${attempt}/3 failed" >&2; \
       done; \
       exit 1

###### manager-build
FROM rust:1.99-bookworm AS manager-build
WORKDIR /src

COPY manager/ /src/manager/
COPY --from=server /server/RRRE_Dedicated.exe /src/RRRE_Dedicated.exe

RUN set -eu \
    && CARGO_TARGET_DIR=/src/target \
       METARACE_DEDICATED_EXE=/src/RRRE_Dedicated.exe \
       cargo build --release --manifest-path /src/manager/Cargo.toml

###### runtime
FROM ubuntu:26.04@sha256:6a06111c3b0a8600feadb270aec771dc239bf407b25625f93f44ab00e11f4d3c AS runtime
ARG PROTON_VERSION

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# Wine runtime libraries for a 64-bit prefix. The .NET 4.8 / httpapi pieces the
# old Windows manager needed are deliberately absent: the manager is native and
# the game server is a native x64 binary.
# hadolint ignore=DL3008
RUN set -eu \
    && export DEBIAN_FRONTEND=noninteractive \
    && dpkg --add-architecture i386 \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        fonts-liberation \
        libasound2t64 \
        libasound2t64:i386 \
        libc6-i386 \
        libc6:i386 \
        libdbus-1-3 \
        libdbus-1-3:i386 \
        libfontconfig1 \
        libfreetype6 \
        libfreetype6:i386 \
        libgcc-s1:i386 \
        libgl1 \
        libgl1:i386 \
        libgl1-mesa-dri \
        libglx-mesa0 \
        libgnutls30t64 \
        libgnutls30t64:i386 \
        libice6 \
        libldap2 \
        libldap2:i386 \
        libpulse0 \
        libpulse0:i386 \
        libsasl2-2 \
        libsasl2-2:i386 \
        libsm6 \
        libsndfile1 \
        libsndfile1:i386 \
        libstdc++6 \
        libstdc++6:i386 \
        libvulkan1 \
        libvulkan1:i386 \
        libx11-6 \
        libx11-6:i386 \
        libx11-xcb1 \
        libxau6 \
        libxcb1 \
        libxcomposite1 \
        libxcomposite1:i386 \
        libxcursor1 \
        libxcursor1:i386 \
        libxdamage1 \
        libxdamage1:i386 \
        libxext6 \
        libxext6:i386 \
        libxfixes3 \
        libxfixes3:i386 \
        libxi6 \
        libxi6:i386 \
        libxinerama1 \
        libxinerama1:i386 \
        libxkbcommon0 \
        libxkbcommon0:i386 \
        libxrandr2 \
        libxrandr2:i386 \
        libxrender1 \
        libxrender1:i386 \
        libxxf86vm1 \
        libxxf86vm1:i386 \
        tar \
        tini \
        xauth \
        xvfb \
        zlib1g \
        zlib1g:i386 \
    && rm -rf /var/lib/apt/lists/*

# GE-Proton supplies the Wine runtime. Verify the release against its
# published SHA-512.
WORKDIR /tmp
RUN set -eu \
    && release_url="https://github.com/GloriousEggroll/proton-ge-custom/releases/download/${PROTON_VERSION}" \
    && archive="${PROTON_VERSION}-x86_64.tar.gz" \
    && mkdir -p /opt/proton \
    && curl -fsSL --retry 3 -O "${release_url}/${archive}" \
    && curl -fsSL --retry 3 -O "${release_url}/${PROTON_VERSION}-x86_64.sha512sum" \
    && sha512sum -c "${PROTON_VERSION}-x86_64.sha512sum" \
    && tar -xzf "$archive" -C /opt/proton --strip-components=1 \
    && rm -f "$archive" "${PROTON_VERSION}-x86_64.sha512sum"

RUN set -eu \
    && userdel -r ubuntu 2>/dev/null || true \
    && useradd --uid 1000 --create-home --shell /bin/bash steam \
    && install -o steam -g steam -m 0644 /dev/null /etc/machine-id \
    && mkdir -p /data/home /data/results /data/logs \
    && chown -R steam:steam /data

ENV HOME=/data/home \
    WINEPREFIX=/data/wineprefix \
    METARACE_WINE_PREFIX=/data/wineprefix \
    METARACE_DATA_DIR=/data \
    METARACE_BIND=0.0.0.0:8088 \
    METARACE_GAME_LAUNCHER=wine \
    METARACE_WINEBOOT_BIN=wine \
    PATH=/opt/proton/files/bin:/usr/local/bin:/usr/bin:/bin \
    WINEDEBUG=-all \
    PROTON_USE_WINED3D=1 \
    SteamAppId=354060 \
    SteamGameId=354060

# Bake a plain Wine prefix (no .NET, no winetricks) so an ephemeral container
# does not pay wineboot at boot.
RUN set -eu \
    && mkdir -p "$WINEPREFIX" "$HOME" \
    && env WINEDEBUG=-all WINEDLLOVERRIDES=mscoree,mshtml=d \
       xvfb-run -a /opt/proton/files/bin/wine wineboot -u \
    && /opt/proton/files/bin/wineserver -w \
    && chown -R steam:steam "$WINEPREFIX" "$HOME" \
    && tar -cf /opt/wineprefix.tar -C "$WINEPREFIX" . \
    && test -s /opt/wineprefix.tar \
    && rm -rf "$WINEPREFIX" \
    && : > /etc/machine-id

COPY --from=manager-build /src/target/release/metarace-rre-manager /usr/local/bin/metarace-rre-manager
COPY --chmod=755 scripts/entrypoint.sh /usr/local/bin/entrypoint

USER 1000:1000
WORKDIR /data

# The portal allocates the game port block (game UDP, game TCP = +1, time UDP
# = +2); the orchestrator publishes the 60000-60014 range or the allocated block.
EXPOSE 8088/tcp 60000-60014/udp 60000-60014/tcp

LABEL org.opencontainers.image.title="metarace/rre-server" \
      org.opencontainers.image.description="Metarace RaceRoom dedicated server (Rust manager, GE-Proton)" \
      org.opencontainers.image.source="https://github.com/chris-ldgk/metarace-gg"

ENTRYPOINT ["/usr/bin/tini", "-g", "--", "/usr/local/bin/entrypoint"]
