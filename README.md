# Journal

Personal stylus-friendly journal and planner for Linux.  
Infinite-scroll canvas, page/notebook templates, calendar-based planners.  
Optimized for Framework 12 with touchscreen + stylus.

## Tech stack

- Rust, GTK4 (gtk4-rs), Vello (GPU compute via wgpu Vulkan, drawn into a `GLArea`); Cairo retained for PDF export
- SQLite per notebook (`.journal` files)
- See [PLAN.md](PLAN.md) for full architecture and phase status.

## Building & installing

### Build

```bash
cargo build --release -p journal-app
```

The binary lands at `target/release/journal-app`.

### User-level install (no sudo)

```bash
make install
```

This copies:

| File | Destination |
|------|-------------|
| `target/release/journal-app` | `~/.local/bin/journal-app` |
| `resources/dev.s7k.journal.desktop` | `~/.local/share/applications/` |
| `resources/icons/dev.s7k.journal.svg` | `~/.local/share/icons/hicolor/scalable/apps/` |

Then runs `update-desktop-database` and `gtk-update-icon-cache` if present.

### Uninstall

```bash
make uninstall
```

## Data locations

| Purpose | Path |
|---------|------|
| Notebook index | `~/.local/share/journal/index.db` |
| Per-notebook data | `~/.local/share/journal/journals/{notebook_id}.journal` (one self-contained SQLite file each) |
| Legacy pre-Phase-6.2 db | `~/.local/share/journal/journal.db.legacy` (auto-renamed after migration) |
| User page templates | `~/.local/share/journal/templates/` |
| User notebook templates | `~/.local/share/journal/notebook_templates/` |
| App config | `~/.config/journal/config.toml` |

## Development

```bash
cargo run -p journal-app
cargo build --workspace
```

## Flatpak (stub)

A manifest skeleton lives at `packaging/dev.s7k.journal.yaml`.  
It is not yet buildable end-to-end (Rust/Cargo SDK extension and vendored deps not wired up).
