# syntax=docker/dockerfile:1
# Own Linux image for the RaceRoom dedicated server (Steam app 354060).
# No dependency on the procinger/r3e-oci image: the patched httpapi.dll is
# built from the pinned Wine revision, and SteamCMD, the server and the .NET
# Framework 4.8 prefix are all produced at build time.
#
# Approach and the httpapi patch are adapted from procinger/r3e-oci (MIT);
# see THIRD-PARTY.md.

ARG PROTON_VERSION=GE-Proton11-11
ARG WINE_REVISION=c53678ab22a85f0fad3588dd34f0452a463bd015

###### httpapi-build
FROM ubuntu:26.04@sha256:6a06111c3b0a8600feadb270aec771dc239bf407b25625f93f44ab00e11f4d3c AS httpapi-build
ARG PROTON_VERSION
ARG WINE_REVISION
ARG BUILD_JOBS=4

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# The patch targets wine-11.0. Keep the Wine revision in sync with the
# GE-Proton release, or the DLL will not match the runtime.
RUN set -eu \
    && test "$PROTON_VERSION" = "GE-Proton11-11" \
    && test "$WINE_REVISION" = "c53678ab22a85f0fad3588dd34f0452a463bd015"

# hadolint ignore=DL3008
RUN set -eu \
    && export DEBIAN_FRONTEND=noninteractive \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        autoconf \
        bison \
        flex \
        ca-certificates \
        curl \
        git \
        pkg-config \
        python3 \
        gcc-mingw-w64-x86-64 \
        g++-mingw-w64-x86-64 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src/wine
RUN set -eu \
    && curl -fsSL --retry 3 \
        "https://codeload.github.com/ValveSoftware/wine/tar.gz/${WINE_REVISION}" \
        | tar -xz --strip-components=1

RUN set -eu \
    && autoreconf -f \
    && tools/make_requests \
    && dlls/winevulkan/make_vulkan \
    && tools/make_specfiles

WORKDIR /build/wine64
RUN set -eu \
    && /src/wine/configure --enable-win64 --without-x --without-freetype

COPY patches/wine/0001-httpapi-synchronous-send-iocp.patch /tmp/httpapi.patch
RUN set -eu \
    && git -C /src/wine apply --check /tmp/httpapi.patch \
    && git -C /src/wine apply /tmp/httpapi.patch \
    && make -C /build/wine64 -j"${BUILD_JOBS}" dlls/httpapi/x86_64-windows/httpapi.dll \
    && mkdir -p /out \
    && cp /build/wine64/dlls/httpapi/x86_64-windows/httpapi.dll /out/httpapi.dll

###### runtime
FROM ubuntu:26.04@sha256:6a06111c3b0a8600feadb270aec771dc239bf407b25625f93f44ab00e11f4d3c AS runtime
ARG PROTON_VERSION
ARG WINETRICKS_VERSION=20260125
ARG WINETRICKS_REVISION=b76e1ee79ac57d7aceb384f74518fc423265810c
ARG STEAMCMD_URL=https://steamcdn-a.akamaihd.net/client/installer/steamcmd_linux.tar.gz
ARG RRE_BUILD_STAMP=2026-11-03

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# hadolint ignore=DL3008
RUN set -eu \
    && export DEBIAN_FRONTEND=noninteractive \
    && dpkg --add-architecture i386 \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        cabextract \
        curl \
        fonts-liberation \
        lib32gcc-s1 \
        lib32stdc++6 \
        libasound2t64 libasound2t64:i386 \
        libgl1 libgl1:i386 \
        libvulkan1 libvulkan1:i386 \
        libwine libwine:i386 \
        nginx \
        openssl \
        python3 \
        tini \
        unzip \
        xauth \
        xvfb \
    && rm -rf /var/lib/apt/lists/*

RUN set -eu \
    && mkdir -p /opt/steamcmd \
    && curl -fsSL --retry 3 "$STEAMCMD_URL" | tar -xz -C /opt/steamcmd

# GE-Proton supplies the Wine runtime and the 32-bit support that the .NET
# installer needs. Verify the release archive against its published SHA-512.
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

COPY --from=httpapi-build /out/httpapi.dll /opt/proton/files/lib/wine/x86_64-windows/httpapi.dll

RUN set -eu \
    && curl -fsSL --retry 3 \
        "https://raw.githubusercontent.com/Winetricks/winetricks/${WINETRICKS_REVISION}/src/winetricks" \
        -o /usr/local/bin/winetricks \
    && grep -Fx "WINETRICKS_VERSION=${WINETRICKS_VERSION}" /usr/local/bin/winetricks \
    && chmod 755 /usr/local/bin/winetricks

RUN set -eu \
    && userdel -r ubuntu 2>/dev/null || true \
    && useradd --uid 1000 --create-home --shell /bin/bash steam \
    && install -o steam -g steam -m 0644 /dev/null /etc/machine-id \
    && mkdir -p /data /opt/raceroom \
    && chown -R steam:steam /opt/steamcmd /opt/raceroom /data

ENV HOME=/data/home \
    STEAM_COMPAT_CLIENT_INSTALL_PATH=/opt/steamcmd \
    STEAM_COMPAT_DATA_PATH=/data/compatdata \
    WINEPREFIX=/data/dotnet48 \
    SteamAppId=354060 \
    SteamGameId=354060 \
    PROTON_USE_WINED3D=1 \
    WINEDEBUG=-all \
    WINEDLLOVERRIDES=mscoree=n,b \
    UPDATE_ON_START=false \
    SERVER_EXE=RRRE_Dedicated.exe

COPY --chmod=755 scripts/entrypoint.sh /usr/local/bin/entrypoint
COPY scripts/nginx.conf /etc/raceroom-nginx.conf

USER 1000:1000
WORKDIR /data

# Bake the server outside /data. The entrypoint copies it into an empty volume
# at startup, so bind mounts and emptyDir volumes both work.
RUN set -eu \
    && echo "server build ${RRE_BUILD_STAMP}" \
    && export RRE_SERVER_DIR=/opt/raceroom/server \
    && source /usr/local/bin/entrypoint \
    && prepare_machine_id \
    && prepare_directories \
    && install_server \
    && check_server_executable \
    && rm -rf "$HOME/Steam/logs" "$HOME/Steam/appcache/httpcache"

# Bake the .NET Framework 4.8 prefix once. At runtime the entrypoint extracts
# this tar into WINEPREFIX, so no network access is needed on startup.
RUN set -eu \
    && source /usr/local/bin/entrypoint \
    && mkdir -p "$HOME" "$WINEPREFIX" \
    && env WINE=/opt/proton/files/bin/wine \
             WINESERVER=/opt/proton/files/bin/wineserver \
             WINEDLLOVERRIDES=mscoree=d \
             xvfb-run -a winetricks -q dotnet48 \
    && /opt/proton/files/bin/wineserver -w \
    && tar -cf /opt/raceroom/dotnet48-prefix.tar -C "$WINEPREFIX" . \
    && test -s /opt/raceroom/dotnet48-prefix.tar \
    && rm -rf "$WINEPREFIX" "$HOME/.cache/winetricks" \
    && : > /etc/machine-id

LABEL org.opencontainers.image.title="metarace/rre-server" \
      org.opencontainers.image.description="RaceRoom dedicated server under GE-Proton Wine" \
      org.opencontainers.image.source="https://github.com/chris-ldgk/metarace-gg"

# Each live server uses three consecutive ports: game UDP, game TCP = +1,
# time UDP = +2. One manager is licensed for five live servers, 60000-60014.
EXPOSE 8080/tcp 60000-60014/udp 60000-60014/tcp

ENTRYPOINT ["/usr/bin/tini", "-g", "--", "/usr/local/bin/entrypoint"]
