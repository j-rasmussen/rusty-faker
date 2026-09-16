# Compiled-in data tables

One JSON file per locale and provider. `../build.rs` reads them at compile time and
generates static Rust tables — cumulative weights precomputed, `{{token}}` templates
pre-tokenized — so nothing here is parsed at runtime. The JSON-attribute-to-Rust-field
mapping is the `SCHEMA` constant in `../build.rs`.

**Never hand-edit these files.** Regenerate them instead:

```bash
.venv/bin/python tools/extract_faker_data.py [--locale xx_XX]
```

which requires the pinned Faker (`Faker==40.39.0`).

## Provenance

This data is extracted from Faker (https://github.com/joke2k/faker), MIT,
Copyright (c) 2012 Daniele Faraglia and individual contributors. Faker's license is
kept at `LICENSE-faker` in the repository root and the derivation is recorded in
`NOTICE`. Each file records the `faker_version`, `locale`, `provider` and
`provider_module` it came from.
