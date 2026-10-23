# Third-Party Notices

HarnessHarness is dual-licensed under **MIT OR Apache-2.0** (see
`LICENSE-MIT` and `LICENSE-APACHE`). The workspace is pure-std
modulo a named, pinned admission set (ADR-0359; ADR-0365) plus one
dev-dependency — the complete inventory is below.

This file is **generated — do not edit**. Regenerate with
`bash scripts/gen-third-party-notices.sh`; CI fails on drift
(`--check`). The component set and pinned versions come from the
committed `Cargo.lock`; license expressions come from
`cargo metadata --format-version 1` (each crate's own manifest).

## Direct admitted dependencies

### ring 0.17.14 — Apache-2.0 AND ISC

_admitted by ADR-0359 — `hharness-secrets` DPoP verify + blob-pool AEAD._

ring is licensed Apache-2.0 AND ISC. Portions are derived from OpenSSL and BoringSSL and carry the OpenSSL/SSLeay license terms; see the crate's LICENSE file.

### rustls 0.23.46 — Apache-2.0 OR ISC OR MIT

_admitted by ADR-0365 — `hharness-tls` closed HTTPS/mTLS scope (`ring` backend)._

### rustls-webpki 0.103.16 — ISC

_admitted by ADR-0365 — certificate verification inside `hharness-tls`._

ISC-licensed; see the ISC text in the appendix below.

### tempfile 3.27.0 — MIT OR Apache-2.0

_dev-dependency of `hharness-varhost` tests only — never a shipped artifact._

### webpki-roots 1.0.10 — CDLA-Permissive-2.0

_admitted by ADR-0365 — Mozilla root store inside `hharness-tls`._

The crate is licensed CDLA-Permissive-2.0. The embedded data is Mozilla's root certificate store, licensed MPL-2.0 — a data set, not a code license.

## Transitive dependencies (lockfile-pinned)

| package | version | license |
|---|---|---|
| bitflags | 2.13.2 | MIT OR Apache-2.0 |
| cc | 1.8.0 | MIT OR Apache-2.0 |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 |
| errno | 0.3.14 | MIT OR Apache-2.0 |
| fastrand | 2.5.0 | Apache-2.0 OR MIT |
| find-msvc-tools | 0.1.14 | MIT OR Apache-2.0 |
| getrandom | 0.2.17 | MIT OR Apache-2.0 |
| getrandom | 0.4.3 | MIT OR Apache-2.0 |
| libc | 0.2.189 | MIT OR Apache-2.0 |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |
| rustix | 1.1.5 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 |
| shlex | 2.0.1 | MIT OR Apache-2.0 |
| subtle | 2.6.1 | BSD-3-Clause |
| untrusted | 0.9.0 | ISC |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 |
| zeroize | 1.9.1 | Apache-2.0 OR MIT |

Notes on transitive entries:

- **linux-raw-sys 0.12.1** — License expression includes Apache-2.0 WITH LLVM-exception.
- **r-efi 6.0.0** — MIT OR Apache-2.0 OR LGPL-2.1-or-later — an election among licenses; we rely on MIT OR Apache-2.0.
- **rustix 1.1.5** — License expression includes Apache-2.0 WITH LLVM-exception.
- **subtle 2.6.1** — BSD-3-Clause; see the BSD-3-Clause text in the appendix below.
- **untrusted 0.9.0** — ISC-licensed; see the ISC text in the appendix below.
- **wasi 0.11.1+wasi-snapshot-preview1** — License expression includes Apache-2.0 WITH LLVM-exception.

## Appendix — referenced license texts

### ISC license (`rustls-webpki`, `untrusted`; the ISC component of `ring`)

```
ISC License

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY
SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION
OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN
CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
```

### BSD-3-Clause (`subtle`)

```
BSD 3-Clause License

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:
(1) Redistributions of source code must retain the copyright notice, this
list of conditions and the following disclaimer. (2) Redistributions in
binary form must reproduce the copyright notice, this list of conditions
and the following disclaimer in the documentation and/or other materials
provided with the distribution. (3) Neither the name of the copyright holder
nor the names of its contributors may be used to endorse or promote products
derived from this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED "AS IS" WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING THE WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
PARTICULAR PURPOSE. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS
BE LIABLE FOR DAMAGES ARISING FROM USE OF THIS SOFTWARE.
```

Other referenced licenses: MPL-2.0 (the `webpki-roots` Mozilla root
data), CDLA-Permissive-2.0 (`webpki-roots` packaging),
Apache-2.0 WITH LLVM-exception (`linux-raw-sys`, `rustix`, `wasi`),
LGPL-2.1-or-later (an `r-efi` election option) — see each crate's
license files and https://spdx.org/licenses/.
