# Third-party notices

inim is MIT-licensed (see `LICENSE`). MIT covers inim's original code and
documentation. The following committed material originates outside inim
and retains its own license terms.

## bgpkit-parser test fixture — `tests/fixtures/mrt/update-example.gz`

- **Source:** BGPKIT (`bgpkit/bgpkit-parser`) upstream test suite.
- **Retrieved from:** `https://spaces.bgpkit.org/parser/update-example.gz`
  (2026-07-31), exact copy, SHA-256
  `9298763bbecbaef2a4378aa8bf58f0c8e911d9afd8e5d4cd1c15f0beb6922d66`.
- **License:** MIT (BGPKIT).
- **Use:** exercise the ingestion boundary in `ingest` tests only.
- **Provenance:** see `tests/fixtures/README.md` and
  `docs/DATA_PROVENANCE.md`.

MIT License — Copyright (c) BGPKIT contributors:

> Permission is hereby granted, free of charge, to any person obtaining a
> copy of this software and associated documentation files (the
> "Software"), to deal in the Software without restriction, including
> without limitation the rights to use, copy, modify, merge, publish,
> distribute, sublicense, and/or sell copies of the Software, and to
> permit persons to whom the Software is furnished to do so, subject to
> the following conditions:
>
> The above copyright notice and this permission notice shall be included
> in all copies or substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
> OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
> MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
> IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
> CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
> TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
> SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

## Public operational ticket fixtures — `tests/fixtures/internet2/`, `tests/fixtures/grnoc/`

Public operational announcements (Internet2 GRNOC task records and an
Indiana University GRNOC Public Task Viewer record) reformatted into
minimal JSON fixtures. These are public factual operational notices, not
copyrighted software; no license text is required for their use. See
`tests/fixtures/README.md` for per-file provenance.

## Dependencies

Cargo dependencies are linked/compiled from crates.io, not vendored into
this repository. Their licenses are audited reproducibly with
`cargo deny check licenses` (see `deny.toml` and `RELEASING.md`). When
producing release bundles that redistribute dependency binaries or source,
generate per-dependency notices with `cargo deny` at that time.
