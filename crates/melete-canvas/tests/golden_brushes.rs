//! Golden-image regression harness for the brush engine (issue #2).
//!
//! Renders a fixed set of stroke fixtures through `VelloRenderer::render_rgba`
//! and compares the output against checked-in PNG goldens. The whole module
//! is `#![cfg(feature = "vello")]` because we need the renderer; tests are
//! `#[ignore]` because CI runners typically lack a Vulkan adapter.
//!
//!     cargo test -p melete-canvas --features vello -- --ignored
//!
//! Regenerate the baselines after intentional renderer changes:
//!
//!     UPDATE_GOLDENS=1 cargo test -p melete-canvas --features vello -- --ignored
//!
//! Pixel comparison is tolerant (≤2/255 per-channel delta, ≤0.1% pixels) so
//! GPU/driver micro-jitter doesn't fail the suite — we're catching shape /
//! position / brush-recipe regressions, not bit-exact PNGs.

#![cfg(feature = "vello")]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::{ImageBuffer, Rgba};
use melete_canvas::vello_renderer::{OverlayState, ToolStyleParams, VelloRenderer};
use melete_canvas::{BackgroundConfig, ViewportTransform};
use melete_core::asset_resolver::AssetResolver;
use melete_core::pen::{BlendMode, Color, PenSettings, ToolStyle};
use melete_core::stroke::{Stroke, StrokePoint};
use melete_core::{Point, Rect, Viewport};

const W: u32 = 512;
const H: u32 = 384;

struct NullResolver;
impl AssetResolver for NullResolver {
    fn resolve(&self, _: &str) -> Option<Arc<[u8]>> {
        None
    }
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn sine_arc(style: ToolStyle, color: Color, width: f64) -> Stroke {
    let pts: Vec<StrokePoint> = (0..48)
        .map(|i| {
            let t = i as f64 / 47.0;
            StrokePoint {
                x: 32.0 + t * 448.0,
                y: 192.0 + (t * std::f64::consts::TAU).sin() * 80.0,
                pressure: 0.4 + (t * std::f64::consts::PI).sin() as f32 * 0.55,
                tilt_x: 0.0,
                tilt_y: 0.0,
                timestamp_ms: (i as u64) * 16,
            }
        })
        .collect();
    let bbox = bbox_of(&pts);
    Stroke {
        id: uuid::Uuid::nil(),
        points: pts,
        pen: PenSettings {
            color,
            base_width: width,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            brush_style: style,
        },
        zoom_at_creation: 1.0,
        bounding_box: bbox,
        brush_recipe: None,
    }
}

fn bbox_of(pts: &[StrokePoint]) -> Rect {
    let (mut xmin, mut ymin, mut xmax, mut ymax) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for p in pts {
        xmin = xmin.min(p.x);
        ymin = ymin.min(p.y);
        xmax = xmax.max(p.x);
        ymax = ymax.max(p.y);
    }
    Rect {
        x: xmin,
        y: ymin,
        width: xmax - xmin,
        height: ymax - ymin,
    }
}

/// Returns `None` if no Vulkan adapter is available on this host —
/// callers treat that as "skip this test" rather than fail.
fn try_renderer() -> Option<VelloRenderer> {
    match VelloRenderer::new() {
        Ok(r) => Some(r),
        Err(e) => {
            eprintln!("[golden] skipping: vello init failed ({e})");
            None
        }
    }
}

fn render(strokes: &[Stroke]) -> Option<Vec<u8>> {
    let mut r = try_renderer()?;
    let vp = Viewport {
        center: Point {
            x: W as f64 * 0.5,
            y: H as f64 * 0.5,
        },
        zoom: 1.0,
        rotation: 0.0,
    };
    let xform = ViewportTransform::new(vp, W as f64, H as f64);
    let rgba = r
        .render_rgba(
            &xform,
            &BackgroundConfig::Blank,
            Rect {
                x: 0.0,
                y: 0.0,
                width: W as f64,
                height: H as f64,
            },
            strokes,
            &HashSet::new(),
            &OverlayState::default(),
            &ToolStyleParams::default(),
            &NullResolver,
            W,
            H,
            |_scene, _affine, _rect| {},
        )
        .expect("render_rgba");
    Some(rgba)
}

fn assert_golden(name: &str, rgba: Option<Vec<u8>>) {
    let Some(rgba) = rgba else { return };
    let path = fixture_dir().join(format!("{name}.png"));
    if std::env::var("UPDATE_GOLDENS").is_ok() {
        std::fs::create_dir_all(fixture_dir()).unwrap();
        let img: ImageBuffer<Rgba<u8>, _> =
            ImageBuffer::from_raw(W, H, rgba).expect("rgba buffer size");
        img.save(&path).expect("write golden");
        eprintln!("[golden] wrote {}", path.display());
        return;
    }
    let golden = match image::open(&path) {
        Ok(d) => d.to_rgba8(),
        Err(e) => panic!(
            "missing golden {}: {e}\n  re-run with UPDATE_GOLDENS=1 to seed",
            path.display()
        ),
    };
    assert_eq!(golden.width(), W);
    assert_eq!(golden.height(), H);
    let golden_bytes = golden.as_raw();
    const PER_CH_TOL: i32 = 2;
    let total = (W as u64) * (H as u64);
    let mut mismatched = 0u64;
    for i in 0..total as usize {
        let a = &rgba[i * 4..i * 4 + 4];
        let b = &golden_bytes[i * 4..i * 4 + 4];
        let dr = (a[0] as i32 - b[0] as i32).abs();
        let dg = (a[1] as i32 - b[1] as i32).abs();
        let db = (a[2] as i32 - b[2] as i32).abs();
        let da = (a[3] as i32 - b[3] as i32).abs();
        if dr > PER_CH_TOL || dg > PER_CH_TOL || db > PER_CH_TOL || da > PER_CH_TOL {
            mismatched += 1;
        }
    }
    let pct = mismatched as f64 / total as f64;
    assert!(
        pct < 0.001,
        "golden mismatch on {name}: {mismatched}/{total} pixels ({:.3}%) differ beyond tolerance",
        pct * 100.0
    );
}

#[test]
#[ignore = "requires Vulkan adapter; run with `cargo test -- --ignored`"]
fn pen_stroke_golden() {
    let s = sine_arc(
        ToolStyle::Pen,
        Color {
            r: 30,
            g: 30,
            b: 30,
            a: 255,
        },
        2.5,
    );
    assert_golden("pen_basic", render(&[s]));
}

#[test]
#[ignore = "requires Vulkan adapter; run with `cargo test -- --ignored`"]
fn pencil_stroke_golden() {
    let s = sine_arc(
        ToolStyle::Pencil,
        Color {
            r: 80,
            g: 80,
            b: 80,
            a: 255,
        },
        3.0,
    );
    assert_golden("pencil_basic", render(&[s]));
}

#[test]
#[ignore = "requires Vulkan adapter; run with `cargo test -- --ignored`"]
fn highlighter_stroke_golden() {
    let s = sine_arc(
        ToolStyle::Highlighter,
        Color {
            r: 240,
            g: 220,
            b: 60,
            a: 180,
        },
        14.0,
    );
    assert_golden("highlighter_basic", render(&[s]));
}

#[test]
#[ignore = "requires Vulkan adapter; run with `cargo test -- --ignored`"]
fn calligraphy_stroke_golden() {
    let s = sine_arc(
        ToolStyle::Calligraphy,
        Color {
            r: 10,
            g: 10,
            b: 60,
            a: 255,
        },
        5.0,
    );
    assert_golden("calligraphy_basic", render(&[s]));
}

#[test]
#[ignore = "requires Vulkan adapter; run with `cargo test -- --ignored`"]
fn spray_stroke_golden() {
    let s = sine_arc(
        ToolStyle::SprayCan,
        Color {
            r: 200,
            g: 30,
            b: 30,
            a: 255,
        },
        6.0,
    );
    assert_golden("spray_basic", render(&[s]));
}

#[test]
#[ignore = "requires Vulkan adapter; run with `cargo test -- --ignored`"]
fn layered_highlight_then_pen_golden() {
    let strokes = vec![
        sine_arc(
            ToolStyle::Highlighter,
            Color {
                r: 250,
                g: 230,
                b: 60,
                a: 160,
            },
            16.0,
        ),
        sine_arc(
            ToolStyle::Pen,
            Color {
                r: 10,
                g: 10,
                b: 10,
                a: 255,
            },
            2.0,
        ),
    ];
    assert_golden("layered_highlight_then_pen", render(&strokes));
}
