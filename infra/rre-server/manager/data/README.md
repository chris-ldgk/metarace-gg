# Proprietary build input

Place the original `RRRE_Dedicated.exe` here, or point `METARACE_DEDICATED_EXE`
at it. The build extracts `CommunityMultiplayer.Game.game.zip` and embeds it in
the manager binary.

The file is proprietary (Sector3 Studios / KW Studios) and is **not** committed.
The container image obtains it through SteamCMD (app 354060) at build time.
