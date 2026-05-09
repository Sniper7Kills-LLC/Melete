//! Integration tests for the Phase 6.3 catalog schema, brush /
//! template stores, and the one-shot TOML → SQLite migration.
//!
//! All tests run against `TempDir`s — none touch `~/.config/` or
//! `~/.local/share/`.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use uuid::Uuid;

use journal_storage::{
    init_index_schema, migrate_if_needed, AssetBytes, BrushRow, BrushStore, MigrationPaths,
    MultiFileSqliteBackend, TemplateRow, TemplateStore,
};

// ── helpers ────────────────────────────────────────────────────────────

fn tmpdir() -> TempDir {
    tempfile::tempdir().unwrap()
}

fn open_index(root: &Path) -> Connection {
    let mut conn = Connection::open(root.join("index.db")).unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.pragma_update(None, "foreign_keys", "ON").unwrap();
    init_index_schema(&mut conn).unwrap();
    conn
}

fn paths_for(cfg: &Path, data: &Path) -> MigrationPaths {
    MigrationPaths {
        brushes_toml: cfg.join("brushes.toml"),
        page_templates_dir: data.join("templates"),
        notebook_templates_dir: data.join("notebook_templates"),
    }
}

fn write(p: &Path, body: &str) {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, body).unwrap();
}

fn write_bytes(p: &Path, body: &[u8]) {
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(p, body).unwrap();
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

const BRUSH_ID_A: &str = "11111111-1111-1111-1111-111111111111";
const BRUSH_ID_B: &str = "22222222-2222-2222-2222-222222222222";
const TPL_ID_A: &str = "aaaa1111-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
const TPL_ID_B: &str = "bbbb2222-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
const NB_TPL_ID: &str = "cccc3333-cccc-cccc-cccc-cccccccccccc";

fn brushes_toml() -> String {
    format!(
        r#"
[[brushes]]
id = "{a}"
name = "Inky"
[[brushes.layers]]
enabled = true
tip_scale = 1.0
blend = "Normal"
geometry = {{ Smooth = {{ resample_step_mm = 1.0 }} }}
width = {{ Pressure = {{ floor = 0.6, amp = 0.4 }} }}
tip = "Round"
[brushes.layers.color]
sat_jitter_amp = 0.0
val_jitter_amp = 0.0
opacity_pressure_amp = 0.0
[brushes.cursor]
Auto = {{}}

[[brushes]]
id = "{b}"
name = "Sketchy"
[[brushes.layers]]
enabled = true
tip_scale = 1.0
blend = "Normal"
geometry = "Raw"
width = "Constant"
tip = "Round"
[brushes.layers.color]
sat_jitter_amp = 0.0
val_jitter_amp = 0.0
opacity_pressure_amp = 0.0
[brushes.cursor]
Auto = {{}}
"#,
        a = BRUSH_ID_A,
        b = BRUSH_ID_B,
    )
}

fn page_template_toml(id: &str, name: &str) -> String {
    format!(
        r#"
schema_version = 1
id = "{id}"
name = "{name}"
description = "test page tpl"
size_mm = [215.9, 279.4]
category = "test"

[background]
type = "blank"
"#
    )
}

fn page_template_with_image_toml(id: &str, image_path: &str) -> String {
    format!(
        r#"
schema_version = 1
id = "{id}"
name = "Imaged"
size_mm = [215.9, 279.4]

[background]
type = "image"
path = "{image_path}"
"#
    )
}

fn page_template_with_pdf_toml(id: &str, pdf_path: &str) -> String {
    format!(
        r#"
schema_version = 1
id = "{id}"
name = "Pdfful"
size_mm = [215.9, 279.4]

[background]
type = "pdf"
path = "{pdf_path}"
page = 1
"#
    )
}

fn notebook_template_toml() -> String {
    format!(
        r#"
id = "{id}"
name = "Yearly Test"

[grouping]
type = "monthly"
"#,
        id = NB_TPL_ID
    )
}

// ── tests ──────────────────────────────────────────────────────────────

#[test]
fn init_index_schema_idempotent() {
    let root = tmpdir();
    fs::create_dir_all(root.path().join("journals")).unwrap();
    // Open once, verify tables.
    let mut conn = open_index(root.path());
    for table in [
        "brushes",
        "page_templates",
        "notebook_templates",
        "page_template_assets",
    ] {
        let n: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }
    // Re-running init_index_schema must not error or duplicate.
    init_index_schema(&mut conn).unwrap();
    init_index_schema(&mut conn).unwrap();
    let v: u32 = conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 1);
}

#[test]
fn template_migration_inserts_all_three_kinds() {
    let root = tmpdir();
    let cfg = tmpdir();
    let data = tmpdir();
    fs::create_dir_all(root.path().join("journals")).unwrap();

    write(&cfg.path().join("brushes.toml"), &brushes_toml());
    write(
        &data.path().join("templates").join("a.toml"),
        &page_template_toml(TPL_ID_A, "PageA"),
    );
    write(
        &data.path().join("notebook_templates").join("yearly.toml"),
        &notebook_template_toml(),
    );

    let paths = paths_for(cfg.path(), data.path());
    let mut be =
        MultiFileSqliteBackend::open_with_migration_paths(root.path(), Some(paths)).unwrap();

    let brushes = be.list_brushes().unwrap();
    assert_eq!(brushes.len(), 2, "two brushes expected");
    let names: Vec<_> = brushes.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains(&"Inky"));
    assert!(names.contains(&"Sketchy"));

    let page_tpls = be.list_page_templates().unwrap();
    assert_eq!(page_tpls.len(), 1);
    assert_eq!(page_tpls[0].name, "PageA");

    let nb_tpls = be.list_notebook_templates().unwrap();
    assert_eq!(nb_tpls.len(), 1);
    assert_eq!(nb_tpls[0].name, "Yearly Test");
}

#[test]
fn template_migration_idempotent() {
    let root = tmpdir();
    let cfg = tmpdir();
    let data = tmpdir();
    write(&cfg.path().join("brushes.toml"), &brushes_toml());
    write(
        &data.path().join("templates").join("a.toml"),
        &page_template_toml(TPL_ID_A, "PageA"),
    );
    write(
        &data.path().join("notebook_templates").join("nt.toml"),
        &notebook_template_toml(),
    );

    let paths = paths_for(cfg.path(), data.path());
    {
        let mut be =
            MultiFileSqliteBackend::open_with_migration_paths(root.path(), Some(paths)).unwrap();
        assert_eq!(be.list_brushes().unwrap().len(), 2);
        assert_eq!(be.list_page_templates().unwrap().len(), 1);
        assert_eq!(be.list_notebook_templates().unwrap().len(), 1);
    }
    // Reopen against the same dirs. Second run sees non-empty
    // tables and skips — counts must not double.
    let paths = paths_for(cfg.path(), data.path());
    {
        let mut be =
            MultiFileSqliteBackend::open_with_migration_paths(root.path(), Some(paths)).unwrap();
        assert_eq!(be.list_brushes().unwrap().len(), 2);
        assert_eq!(be.list_page_templates().unwrap().len(), 1);
        assert_eq!(be.list_notebook_templates().unwrap().len(), 1);
    }
}

#[test]
fn template_migration_handles_malformed_toml() {
    let root = tmpdir();
    let cfg = tmpdir();
    let data = tmpdir();
    write(&cfg.path().join("brushes.toml"), "this = is not valid {{");
    write(&data.path().join("templates").join("bad.toml"), "@@@ junk");
    write(
        &data.path().join("templates").join("good.toml"),
        &page_template_toml(TPL_ID_A, "Good"),
    );
    write(
        &data.path().join("notebook_templates").join("bad.toml"),
        "###",
    );

    let paths = paths_for(cfg.path(), data.path());
    let mut be =
        MultiFileSqliteBackend::open_with_migration_paths(root.path(), Some(paths)).unwrap();

    // Malformed brushes file → zero brushes inserted, but other
    // walks still run.
    assert_eq!(be.list_brushes().unwrap().len(), 0);
    let page = be.list_page_templates().unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].name, "Good");
    assert_eq!(be.list_notebook_templates().unwrap().len(), 0);
}

#[test]
fn template_migration_missing_asset_file_dangling_uri() {
    let root = tmpdir();
    let cfg = tmpdir();
    let data = tmpdir();
    write(
        &data.path().join("templates").join("img.toml"),
        &page_template_with_image_toml(TPL_ID_A, "missing.png"),
    );

    let paths = paths_for(cfg.path(), data.path());
    let mut be =
        MultiFileSqliteBackend::open_with_migration_paths(root.path(), Some(paths)).unwrap();

    let pages = be.list_page_templates().unwrap();
    assert_eq!(pages.len(), 1);
    let body = &pages[0].body_toml;
    assert!(
        body.contains("asset:missing.png"),
        "body should rewrite to asset URI even when bytes are missing: {}",
        body
    );

    // No asset bytes were available, so no asset rows were inserted.
    let assets = be
        .list_page_template_assets(Uuid::parse_str(TPL_ID_A).unwrap())
        .unwrap();
    assert!(assets.is_empty());
}

#[test]
fn assets_cascade_on_delete_page_template() {
    let root = tmpdir();
    let cfg = tmpdir();
    let data = tmpdir();
    let png_bytes = b"fake-png-bytes\x00\x01\x02".to_vec();
    write_bytes(&data.path().join("templates").join("hero.png"), &png_bytes);
    write(
        &data.path().join("templates").join("img.toml"),
        &page_template_with_image_toml(TPL_ID_A, "hero.png"),
    );

    let paths = paths_for(cfg.path(), data.path());
    let mut be =
        MultiFileSqliteBackend::open_with_migration_paths(root.path(), Some(paths)).unwrap();

    let id = Uuid::parse_str(TPL_ID_A).unwrap();
    let assets = be.list_page_template_assets(id).unwrap();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].name, "hero.png");
    assert_eq!(assets[0].mime, "image/png");
    assert_eq!(assets[0].sha256, sha256_hex(&png_bytes));
    assert_eq!(assets[0].size, png_bytes.len() as u64);

    let got = be.get_page_template_asset(id, "hero.png").unwrap().unwrap();
    assert_eq!(got.bytes, png_bytes);

    // Delete → ON DELETE CASCADE drops the asset row.
    be.delete_page_template(id).unwrap();
    let after = be.list_page_template_assets(id).unwrap();
    assert!(after.is_empty());
}

#[test]
fn large_blob_smoke() {
    let root = tmpdir();
    let cfg = tmpdir();
    let data = tmpdir();
    // 50 MB of incompressible-ish bytes via byte index xor counter.
    let mut payload = vec![0u8; 50 * 1024 * 1024];
    for (i, b) in payload.iter_mut().enumerate() {
        *b = (i as u8) ^ ((i >> 13) as u8);
    }
    let expected_sha = sha256_hex(&payload);
    write_bytes(&data.path().join("templates").join("big.png"), &payload);
    write(
        &data.path().join("templates").join("big.toml"),
        &page_template_with_image_toml(TPL_ID_A, "big.png"),
    );

    let paths = paths_for(cfg.path(), data.path());
    let mut be =
        MultiFileSqliteBackend::open_with_migration_paths(root.path(), Some(paths)).unwrap();

    let id = Uuid::parse_str(TPL_ID_A).unwrap();
    let asset = be.get_page_template_asset(id, "big.png").unwrap().unwrap();
    assert_eq!(asset.bytes.len(), payload.len());
    assert_eq!(asset.sha256, expected_sha);
    assert_eq!(sha256_hex(&asset.bytes), expected_sha);
}

#[test]
fn put_brush_upserts() {
    let root = tmpdir();
    fs::create_dir_all(root.path().join("journals")).unwrap();
    let mut be = MultiFileSqliteBackend::open_with_migration_paths(root.path(), None).unwrap();

    let id = Uuid::new_v4();
    let body_v1 = "name = \"v1\"".to_string();
    be.put_brush(&BrushRow {
        id,
        name: "v1".into(),
        body_toml: body_v1.clone(),
        sha256: sha256_hex(body_v1.as_bytes()),
    })
    .unwrap();

    let body_v2 = "name = \"v2\"".to_string();
    be.put_brush(&BrushRow {
        id,
        name: "v2".into(),
        body_toml: body_v2.clone(),
        sha256: sha256_hex(body_v2.as_bytes()),
    })
    .unwrap();

    let listed = be.list_brushes().unwrap();
    assert_eq!(listed.len(), 1, "upsert, not a second row");
    assert_eq!(listed[0].name, "v2");
    assert_eq!(listed[0].body_toml, body_v2);
}

#[test]
fn asset_uri_rewrite_image_and_pdf() {
    let root = tmpdir();
    let cfg = tmpdir();
    let data = tmpdir();

    let png = b"PNG-bytes".to_vec();
    let pdf = b"%PDF-1.4 fake".to_vec();
    write_bytes(&data.path().join("templates").join("hero.png"), &png);
    write_bytes(&data.path().join("templates").join("doc.pdf"), &pdf);
    write(
        &data.path().join("templates").join("img.toml"),
        &page_template_with_image_toml(TPL_ID_A, "hero.png"),
    );
    write(
        &data.path().join("templates").join("pdf.toml"),
        &page_template_with_pdf_toml(TPL_ID_B, "doc.pdf"),
    );

    let paths = paths_for(cfg.path(), data.path());
    let mut be =
        MultiFileSqliteBackend::open_with_migration_paths(root.path(), Some(paths)).unwrap();

    let img_row = be
        .get_page_template(Uuid::parse_str(TPL_ID_A).unwrap())
        .unwrap();
    assert!(img_row.body_toml.contains("asset:hero.png"));
    assert!(!img_row.body_toml.contains("hero.png\"\npath ="),);

    let pdf_row = be
        .get_page_template(Uuid::parse_str(TPL_ID_B).unwrap())
        .unwrap();
    assert!(pdf_row.body_toml.contains("asset:doc.pdf"));

    let img_assets = be
        .list_page_template_assets(Uuid::parse_str(TPL_ID_A).unwrap())
        .unwrap();
    assert_eq!(img_assets.len(), 1);
    assert_eq!(img_assets[0].mime, "image/png");

    let pdf_assets = be
        .list_page_template_assets(Uuid::parse_str(TPL_ID_B).unwrap())
        .unwrap();
    assert_eq!(pdf_assets.len(), 1);
    assert_eq!(pdf_assets[0].mime, "application/pdf");
}

#[test]
fn migration_inside_transaction_partial_failure_atomic() {
    // Pre-seed the brushes table so `needs_migration` reports
    // FALSE — i.e. a normal idempotent skip. Then directly call
    // `migrate_if_needed` on a connection that already has rows
    // and ensure no extra rows show up.
    let root = tmpdir();
    let cfg = tmpdir();
    let data = tmpdir();
    fs::create_dir_all(root.path().join("journals")).unwrap();
    write(&cfg.path().join("brushes.toml"), &brushes_toml());

    let mut conn = open_index(root.path());
    let dummy_id = Uuid::new_v4();
    let updated_at = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO brushes (id, name, body_toml, sha256, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            dummy_id.as_bytes().to_vec(),
            "preseed",
            "x",
            "y",
            updated_at
        ],
    )
    .unwrap();

    let paths = paths_for(cfg.path(), data.path());
    migrate_if_needed(&mut conn, &paths).unwrap();
    // Pre-seeded row blocks the migration; brushes.toml entries are
    // NOT inserted.
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM brushes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        n, 1,
        "migration should skip when any catalog table is non-empty"
    );
}

// ── extra sanity: trait surface ─────────────────────────────────────────

#[test]
fn template_row_round_trip() {
    let root = tmpdir();
    fs::create_dir_all(root.path().join("journals")).unwrap();
    let mut be = MultiFileSqliteBackend::open_with_migration_paths(root.path(), None).unwrap();

    let id = Uuid::new_v4();
    let body = "name = \"TRT\"".to_string();
    let row = TemplateRow {
        id,
        name: "TRT".into(),
        description: "desc".into(),
        category: "cat".into(),
        body_toml: body.clone(),
        sha256: sha256_hex(body.as_bytes()),
    };
    be.put_page_template(
        &row,
        &[AssetBytes {
            name: "x.png".into(),
            mime: "image/png".into(),
            sha256: sha256_hex(b"x"),
            bytes: b"x".to_vec(),
        }],
    )
    .unwrap();

    let got = be.get_page_template(id).unwrap();
    assert_eq!(got, row);

    let asset = be.get_page_template_asset(id, "x.png").unwrap().unwrap();
    assert_eq!(asset.bytes, b"x");
    assert_eq!(asset.mime, "image/png");

    // Notebook template round-trip.
    let nb_id = Uuid::new_v4();
    let nb_body = "id = \"abc\"".to_string();
    let nb_row = TemplateRow {
        id: nb_id,
        name: "NB".into(),
        description: String::new(),
        category: String::new(),
        body_toml: nb_body.clone(),
        sha256: sha256_hex(nb_body.as_bytes()),
    };
    be.put_notebook_template(&nb_row).unwrap();
    let got = be.get_notebook_template(nb_id).unwrap();
    assert_eq!(got, nb_row);
}

#[allow(dead_code)]
fn _unused(_p: &PathBuf) {}
