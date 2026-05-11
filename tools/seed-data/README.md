# seed-data

TOML bodies for templates and notebook templates that were stripped from the
desktop binary in Phase 6.3 (issue #48). These files are uploaded to the
public catalog by the seed-publish CLI (issue #51) so that they remain
available as downloads from the Gallery, even though the desktop ships only
the basic builtins.

## Layout

```
page_templates/      # serialized via melete_templates::serialize_template_toml
notebook_templates/  # serialized via toml::to_string_pretty(&NotebookTemplate)
```

## Preservation gate

`crates/melete-templates/tests/seed_round_trip.rs` parses every file and
re-serializes it; the bytes must match exactly. Failure means a recent
serializer change is lossy or non-deterministic and the seed bytes need to be
re-extracted. Do not edit these files by hand — regenerate them from the
in-Rust definitions if the schema changes.

## Adding a new seed template

Templates here are author-canonical, meant for upload to the catalog. To add
one, build a `PageTemplate` (or `NotebookTemplate`) value in Rust, run it
through the matching serializer, and check the resulting bytes in alongside
its sibling files. The round-trip test will fail loudly if anything in the
serializer drifts.
