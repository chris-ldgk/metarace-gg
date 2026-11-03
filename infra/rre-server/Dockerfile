# RRE dedicated server, prebuilt (D-029/D-030). One live server per container (D-033).
# ponytail: extends the community image instead of forking it; fork when we need to patch Wine ourselves.
FROM ghcr.io/procinger/r3e-oci@sha256:25c26d3ad34dc2666ef24490ddd98cd95f6f496982d4222178e3ea3c308a274d

# Bump to force a fresh SteamCMD download (new RRE build) without --no-cache.
ARG RRE_BUILD_STAMP=2026-11-03

# Install the server and unpack the .NET prefix at build time, reusing the base entrypoint's
# own functions. Anonymous SteamCMD often fails with "Missing configuration" on the first try; install_server retries.
RUN set -eu \
    && echo "build ${RRE_BUILD_STAMP}" \
    && source /usr/local/bin/entrypoint \
    && prepare_machine_id \
    && prepare_directories \
    && install_server \
    && check_server_executable \
    && prepare_dotnet_runtime \
    && rm -rf "$HOME/Steam/logs" "$HOME/Steam/appcache/httpcache" \
    && : > /etc/machine-id

# Never touch Steam at container start: every split must boot the exact baked build.
ENV UPDATE_ON_START=false
