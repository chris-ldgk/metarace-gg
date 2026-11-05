# Third-party notices

## procinger/r3e-oci

The container approach (GE-Proton Wine in a prebuilt image) was informed by
[procinger/r3e-oci](https://github.com/procinger/r3e-oci). No code from that
project is distributed in this image; the Wine `httpapi` patch it ships is no
longer needed because the Windows manager has been replaced by the Rust
manager (D-081).

MIT License

Copyright (c) 2026 Adam Hukalowicz

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Downloaded at build time

- **GE-Proton** — [GloriousEggroll/proton-ge-custom](https://github.com/GloriousEggroll/proton-ge-custom).
  The image redistributes the release archive; review its Wine/Proton licenses
  before publishing the image.
- **SteamCMD** — Valve Corporation (build stage only).
- **RaceRoom Dedicated Server** — Sector3 Studios AB / KW Studios, downloaded
  through Steam app 354060. The game payload is embedded in the manager binary;
  redistribution is subject to RaceRoom's terms.
