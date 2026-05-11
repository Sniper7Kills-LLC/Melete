// Marketing landing for the Melete app.
//
// Aesthetic: editorial stationery / specimen sheet. Cream paper,
// warm ink-black foreground, single accent (manuscript red).
// Typography-led — Instrument Serif (display, italic moments),
// Newsreader (body), JetBrains Mono (eyebrows + CTAs). Hand-drawn
// inline SVG ornaments instead of icon-pack flourishes.
//
// The hero download CTA reads `latest.json` from the releases
// bucket (URL configurable via VITE_RELEASES_MANIFEST_URL at build
// time) to render the current version and link straight at the
// signed S3 / CloudFront tarball. Falls back to a GitHub-releases
// link if the manifest can't load, so the page never ships a
// broken download.

import { useEffect, useState } from "react";
import { Link } from "react-router-dom";

const RELEASES_MANIFEST_URL =
  (import.meta.env.VITE_RELEASES_MANIFEST_URL as string | undefined) ??
  "https://releases.journal.app/latest.json";

const GITHUB_RELEASES_FALLBACK =
  "https://github.com/Sniper7Kills-LLC/Melete/releases";

interface ReleaseManifest {
  version: string;
  publishedAt: string;
  platforms: Record<string, { url: string; sha256?: string; sizeBytes?: number }>;
}

function useLatestRelease(): {
  manifest: ReleaseManifest | null;
  loading: boolean;
} {
  const [manifest, setManifest] = useState<ReleaseManifest | null>(null);
  const [loading, setLoading] = useState(true);
  useEffect(() => {
    let alive = true;
    async function load() {
      try {
        const r = await fetch(RELEASES_MANIFEST_URL, { cache: "no-cache" });
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        const json = (await r.json()) as ReleaseManifest;
        if (alive) setManifest(json);
      } catch {
        // Soft-fail — landing falls back to the GitHub releases link.
        if (alive) setManifest(null);
      } finally {
        if (alive) setLoading(false);
      }
    }
    void load();
    return () => {
      alive = false;
    };
  }, []);
  return { manifest, loading };
}

export function Landing() {
  return (
    <div className="h-full overflow-y-auto bg-[#f6f1e7] text-[#1a1612]">
      <style>{`
        @import url('https://fonts.googleapis.com/css2?family=Instrument+Serif:ital@0;1&family=JetBrains+Mono:wght@400;500&family=Newsreader:ital,opsz,wght@0,6..72,300..600;1,6..72,300..600&display=swap');

        .font-display { font-family: 'Instrument Serif', Georgia, serif; }
        .font-body { font-family: 'Newsreader', Georgia, serif; }
        .font-mono { font-family: 'JetBrains Mono', ui-monospace, monospace; }

        @keyframes ink-rise {
          from { opacity: 0; transform: translateY(10px); }
          to { opacity: 1; transform: translateY(0); }
        }
        .rise { animation: ink-rise 0.8s ease-out both; }
        .d-1 { animation-delay: 0.05s; }
        .d-2 { animation-delay: 0.15s; }
        .d-3 { animation-delay: 0.28s; }
        .d-4 { animation-delay: 0.42s; }
        .d-5 { animation-delay: 0.58s; }

        @keyframes ink-draw { to { stroke-dashoffset: 0; } }
        .ink-stroke {
          stroke-dasharray: 600;
          stroke-dashoffset: 600;
          animation: ink-draw 1.6s ease-out 0.6s forwards;
        }

        @keyframes slow-rot { to { transform: rotate(360deg); } }
        .slow-rot { animation: slow-rot 80s linear infinite; transform-origin: center; }

        .paper-grain {
          background-image:
            radial-gradient(ellipse 80% 50% at 20% 0%, rgba(193,68,46,0.05), transparent 60%),
            radial-gradient(ellipse 60% 40% at 100% 100%, rgba(26,22,18,0.04), transparent 60%);
        }
      `}</style>

      <Hero />
      <Differentiators />
      <FeatureGrid />
      <PlansTeaser />
      <PlatformNote />
      <FooterMark />
    </div>
  );
}

// ── HERO ────────────────────────────────────────────────────────────

function Hero() {
  const { manifest } = useLatestRelease();
  const linuxRelease = manifest?.platforms?.["linux-x86_64"];
  const downloadHref = linuxRelease?.url ?? GITHUB_RELEASES_FALLBACK;
  const versionLabel = manifest?.version ?? "Linux-native";
  return (
    <section className="paper-grain relative overflow-hidden">
      {/* Slow-rotating concentric-circles glyph — reference to the
          infinite-zoom feature. Sits behind the headline, very faint. */}
      <svg
        aria-hidden
        viewBox="0 0 400 400"
        className="slow-rot pointer-events-none absolute -right-32 -top-32 h-[680px] w-[680px] opacity-[0.09]"
      >
        {Array.from({ length: 11 }).map((_, i) => (
          <circle
            key={i}
            cx="200"
            cy="200"
            r={20 + i * 16}
            fill="none"
            stroke="#1a1612"
            strokeWidth="0.6"
          />
        ))}
        <line x1="0" y1="200" x2="400" y2="200" stroke="#1a1612" strokeWidth="0.4" />
        <line x1="200" y1="0" x2="200" y2="400" stroke="#1a1612" strokeWidth="0.4" />
      </svg>

      <div className="relative mx-auto max-w-6xl px-6 pb-28 pt-20 sm:px-10 sm:pt-28">
        <p className="font-mono rise text-[11px] uppercase tracking-[0.3em] text-[#1a1612]/55">
          ⁂ {versionLabel} · Stylus + touch
        </p>

        <h1 className="font-display rise d-1 mt-6 text-[clamp(56px,10vw,140px)] leading-[0.92] tracking-[-0.02em]">
          A journal
          <br />
          that scrolls{" "}
          <span className="relative italic text-[#c1442e]">
            forever.
            <svg
              aria-hidden
              viewBox="0 0 320 30"
              className="absolute -bottom-3 left-0 h-[18px] w-[88%]"
              fill="none"
            >
              <path
                d="M2,18 C 40,4 88,4 140,16 C 200,28 260,8 318,14"
                stroke="#c1442e"
                strokeWidth="3"
                strokeLinecap="round"
                className="ink-stroke"
              />
            </svg>
          </span>
        </h1>

        <div className="mt-14 grid gap-10 md:grid-cols-[1.25fr_1fr] md:items-end md:gap-16">
          <p className="font-body rise d-2 max-w-xl text-[19px] leading-[1.5] text-[#1a1612]/85 sm:text-xl">
            Infinite-canvas notebooks for handwritten notes, planners,
            and sketches. A focused alternative to OneNote and rnote —
            with{" "}
            <span className="italic text-[#c1442e]">
              smart templates
            </span>{" "}
            that auto-generate your planner pages and GPU-accelerated
            ink that stays sharp at any zoom.
          </p>

          <div className="rise d-3 flex flex-col items-start gap-4 md:items-end">
            <a
              href={downloadHref}
              className="group inline-flex items-center gap-3 bg-[#1a1612] px-6 py-3.5 font-mono text-[12px] uppercase tracking-[0.18em] text-[#f6f1e7] transition-colors hover:bg-[#c1442e]"
            >
              Download for Linux
              <span
                aria-hidden
                className="transition-transform group-hover:translate-x-1"
              >
                →
              </span>
            </a>
            {manifest && (
              <p className="font-mono text-[10px] uppercase tracking-[0.18em] text-[#1a1612]/45">
                {manifest.version} · x86_64
              </p>
            )}
            <Link
              to="/gallery"
              className="group inline-flex items-baseline gap-2 border-b border-[#1a1612]/40 pb-0.5 font-mono text-[12px] uppercase tracking-[0.18em] text-[#1a1612] transition-colors hover:border-[#c1442e] hover:text-[#c1442e]"
            >
              Browse template gallery
              <span
                aria-hidden
                className="transition-transform group-hover:translate-x-1"
              >
                →
              </span>
            </Link>
            <p className="font-body max-w-[260px] text-sm italic leading-snug text-[#1a1612]/55 md:text-right">
              Free forever for local use. No account needed to draw.
            </p>
          </div>
        </div>

        {/* Bottom rule + page-marker — visual stationery cue. */}
        <div className="mt-20 flex items-center gap-4">
          <span className="font-mono text-[10px] uppercase tracking-[0.3em] text-[#1a1612]/45">
            § I
          </span>
          <span className="h-px flex-1 bg-[#1a1612]/20" />
          <span className="font-display text-lg italic text-[#1a1612]/45">
            keep scrolling
          </span>
          <span className="h-px w-10 bg-[#1a1612]/20" />
        </div>
      </div>
    </section>
  );
}

// ── DIFFERENTIATORS ────────────────────────────────────────────────

function Differentiators() {
  const items = [
    {
      n: "01",
      kicker: "Canvas",
      title: "Infinite zoom & scroll",
      copy: "Every page is one endless canvas. Zoom in to draw pictures inside pictures, scroll past the bottom edge to keep writing. No fixed page bounds.",
      glyph: <GlyphInfinity />,
    },
    {
      n: "02",
      kicker: "Planner",
      title: "Pages that build themselves",
      copy: "Define a notebook template once — year, month, week, day pages auto-generate for any date. Navigate to a future Tuesday and the page is already there.",
      glyph: <GlyphCalendar />,
    },
    {
      n: "03",
      kicker: "Privacy",
      title: "Local-first by default",
      copy: "Notebooks live as plain SQLite files on your disk. Cloud sync is optional; nothing leaves your laptop unless you ask it to.",
      glyph: <GlyphFolder />,
    },
  ];
  return (
    <section className="border-y border-[#1a1612]/15 bg-[#efe7d6]">
      <div className="mx-auto max-w-6xl px-6 py-20 sm:px-10 sm:py-24">
        <div className="grid gap-14 md:grid-cols-3 md:gap-10">
          {items.map((item, i) => (
            <article
              key={item.n}
              className="rise"
              style={{ animationDelay: `${0.1 + i * 0.12}s` }}
            >
              <div className="mb-5 flex items-baseline gap-3">
                <span className="font-display text-[64px] italic leading-none text-[#c1442e]">
                  {item.n}
                </span>
                <span className="h-10 w-px self-stretch bg-[#1a1612]/25" />
                <span className="font-mono text-[10px] uppercase tracking-[0.22em] text-[#1a1612]/55">
                  {item.kicker}
                </span>
              </div>
              <div className="mb-4 h-12 w-12 text-[#1a1612]/75">
                {item.glyph}
              </div>
              <h3 className="font-display mb-2 text-[28px] leading-tight tracking-tight">
                {item.title}
              </h3>
              <p className="font-body text-[15.5px] leading-[1.55] text-[#1a1612]/75">
                {item.copy}
              </p>
            </article>
          ))}
        </div>
      </div>
    </section>
  );
}

// ── FEATURE GRID ───────────────────────────────────────────────────

function FeatureGrid() {
  const features = [
    {
      title: "Page templates",
      copy: "Grid, dotted, ruled lines, daily planner, or any image / PDF you import as a background.",
    },
    {
      title: "Brush engine",
      copy: "Pen, Pencil, Highlighter, Paintbrush, SprayCan, Calligraphy — composable layers, custom tip shapes, pressure-sensitive widths.",
    },
    {
      title: "Smart calendar pages",
      copy: "Notebook templates with day-of-week selectors. Weekend spreads, weekly reviews, monthly covers — all generated on demand.",
    },
    {
      title: "GPU-rendered ink",
      copy: "Vello on wgpu / Vulkan. A hundred thousand strokes per page stay smooth and sharp at any zoom level.",
    },
    {
      title: "Template sharing",
      copy: "Publish your designs to the gallery. Fork someone else's planner. Free tier publishes 3; Pro publishes 50.",
    },
    {
      title: "Live multi-device sync",
      copy: "Draw on the laptop, watch the tablet update. Pro and Studio plans only — Free saves manually to one cloud notebook.",
    },
  ];
  return (
    <section className="paper-grain border-b border-[#1a1612]/15">
      <div className="mx-auto max-w-6xl px-6 py-20 sm:px-10 sm:py-24">
        <header className="mb-12 flex flex-col items-start justify-between gap-6 border-b border-[#1a1612]/20 pb-4 md:flex-row md:items-end">
          <div>
            <p className="font-mono text-[10px] uppercase tracking-[0.3em] text-[#1a1612]/55">
              § II — In the box
            </p>
            <h2 className="font-display mt-2 text-[44px] italic leading-[0.95] tracking-tight sm:text-[56px]">
              Built for handwriting,
              <br />
              planning, &amp;{" "}
              <span className="not-italic text-[#c1442e]">
                figuring things out.
              </span>
            </h2>
          </div>
        </header>
        <div className="grid grid-cols-1 gap-px bg-[#1a1612]/15 md:grid-cols-2 lg:grid-cols-3">
          {features.map((f, i) => (
            <article
              key={f.title}
              className="group relative bg-[#f6f1e7] p-7 transition-colors hover:bg-[#efe7d6]"
            >
              <span className="font-mono text-[10px] uppercase tracking-[0.2em] text-[#c1442e]">
                F.{String(i + 1).padStart(2, "0")}
              </span>
              <h3 className="font-display mb-3 mt-2 text-[26px] leading-tight tracking-tight">
                {f.title}
              </h3>
              <p className="font-body text-[14.5px] leading-[1.55] text-[#1a1612]/75">
                {f.copy}
              </p>
              <span
                aria-hidden
                className="absolute bottom-5 right-5 font-display text-2xl text-[#1a1612]/15 transition-colors group-hover:text-[#c1442e]/60"
              >
                ✦
              </span>
            </article>
          ))}
        </div>
      </div>
    </section>
  );
}

// ── PLANS TEASER ───────────────────────────────────────────────────

function PlansTeaser() {
  const tiers = [
    {
      name: "Free",
      price: "$0",
      cadence: "forever",
      bullets: [
        "Unlimited local notebooks",
        "1 notebook synced to the cloud",
        "Browse + fork public templates",
      ],
    },
    {
      name: "Pro",
      price: "$8",
      cadence: "/ mo · $80 / yr",
      featured: true,
      bullets: [
        "10 cloud notebooks",
        "Live multi-device sync",
        "Publish up to 50 templates",
        "10 GB cloud storage",
      ],
    },
    {
      name: "Studio",
      price: "$18",
      cadence: "/ mo · $180 / yr",
      bullets: [
        "20 cloud notebooks",
        "Unlimited template publishing",
        "30 GB cloud storage",
        "Priority sync",
      ],
    },
  ];
  return (
    <section className="border-b border-[#1a1612]/15 bg-[#efe7d6]">
      <div className="mx-auto max-w-6xl px-6 py-20 sm:px-10 sm:py-24">
        <header className="mb-10 flex flex-col items-start justify-between gap-6 border-b border-[#1a1612]/20 pb-4 md:flex-row md:items-end">
          <div>
            <p className="font-mono text-[10px] uppercase tracking-[0.3em] text-[#1a1612]/55">
              § III — Plans
            </p>
            <h2 className="font-display mt-2 text-[44px] italic leading-[0.95] tracking-tight sm:text-[56px]">
              Free is forever.
              <br />
              <span className="not-italic">
                Paid unlocks the{" "}
                <span className="italic text-[#c1442e]">cloud.</span>
              </span>
            </h2>
          </div>
          <Link
            to="/billing"
            className="group inline-flex items-baseline gap-2 border-b border-[#1a1612]/40 pb-0.5 font-mono text-[11px] uppercase tracking-[0.18em] transition-colors hover:border-[#c1442e] hover:text-[#c1442e]"
          >
            Compare in detail
            <span
              aria-hidden
              className="transition-transform group-hover:translate-x-1"
            >
              →
            </span>
          </Link>
        </header>

        <div className="grid gap-px bg-[#1a1612]/15 md:grid-cols-3">
          {tiers.map((t) => {
            const dark = t.featured;
            return (
              <div
                key={t.name}
                className={`relative flex flex-col p-8 ${
                  dark
                    ? "bg-[#1a1612] text-[#f6f1e7]"
                    : "bg-[#f6f1e7] text-[#1a1612] hover:bg-[#efe7d6]"
                } transition-colors`}
              >
                {dark && (
                  <span className="absolute right-5 top-5 rotate-3 bg-[#c1442e] px-2.5 py-1 font-mono text-[9px] uppercase tracking-[0.22em] text-[#f6f1e7]">
                    Most popular
                  </span>
                )}
                <div className="flex items-baseline justify-between">
                  <h3 className="font-display text-[34px] leading-none tracking-tight">
                    {t.name}
                  </h3>
                </div>
                <div className="mt-6 flex items-baseline gap-2">
                  <span
                    className={`font-display text-[64px] leading-none tracking-tight ${
                      dark ? "text-[#f6f1e7]" : "text-[#1a1612]"
                    }`}
                  >
                    {t.price}
                  </span>
                  <span
                    className={`font-mono text-[10px] uppercase tracking-[0.18em] ${
                      dark ? "text-[#f6f1e7]/60" : "text-[#1a1612]/55"
                    }`}
                  >
                    {t.cadence}
                  </span>
                </div>
                <hr
                  className={`my-6 ${
                    dark ? "border-[#f6f1e7]/20" : "border-[#1a1612]/15"
                  }`}
                />
                <ul className="space-y-2.5">
                  {t.bullets.map((b) => (
                    <li
                      key={b}
                      className={`flex items-baseline gap-3 font-body text-[14.5px] leading-snug ${
                        dark ? "text-[#f6f1e7]/85" : "text-[#1a1612]/85"
                      }`}
                    >
                      <span
                        aria-hidden
                        className="font-mono text-[10px] text-[#c1442e]"
                      >
                        ─
                      </span>
                      <span>{b}</span>
                    </li>
                  ))}
                </ul>
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}

// ── PLATFORM NOTE ──────────────────────────────────────────────────

function PlatformNote() {
  return (
    <section className="paper-grain border-b border-[#1a1612]/15">
      <div className="mx-auto max-w-3xl px-6 py-20 text-center sm:px-10 sm:py-24">
        <svg
          aria-hidden
          viewBox="0 0 80 50"
          className="mx-auto mb-6 h-16 w-24 text-[#c1442e]"
          fill="none"
        >
          {/* Tiny laptop / framework-sketch glyph */}
          <path
            d="M14,8 L66,8 L66,38 L14,38 Z"
            stroke="currentColor"
            strokeWidth="1.5"
            className="ink-stroke"
          />
          <path
            d="M6,44 L74,44"
            stroke="currentColor"
            strokeWidth="1.5"
            className="ink-stroke"
          />
          <path
            d="M34,38 L46,38 L44,44 L36,44 Z"
            stroke="currentColor"
            strokeWidth="1.2"
            className="ink-stroke"
          />
        </svg>
        <p className="font-mono text-[10px] uppercase tracking-[0.3em] text-[#1a1612]/55">
          § IV — Platforms
        </p>
        <h2 className="font-display mt-3 text-[40px] italic leading-tight tracking-tight sm:text-[48px]">
          Linux today.
          <br />
          <span className="not-italic">macOS &amp; Windows on the roadmap.</span>
        </h2>
        <p className="font-body mx-auto mt-5 max-w-xl text-[15.5px] leading-relaxed text-[#1a1612]/75">
          The desktop app is built on GTK4 and ships as a single binary
          for x86_64 / aarch64 Linux. Touchscreen + stylus tested on
          Framework 12.
        </p>
      </div>
    </section>
  );
}

// ── FOOTER ─────────────────────────────────────────────────────────

function FooterMark() {
  return (
    <footer className="bg-[#1a1612] text-[#f6f1e7]">
      <div className="mx-auto flex max-w-6xl flex-col items-start justify-between gap-6 px-6 py-12 sm:flex-row sm:items-end sm:px-10">
        <div>
          <p className="font-display text-[44px] italic leading-none tracking-tight">
            Melete.
          </p>
          <p className="font-mono mt-3 text-[10px] uppercase tracking-[0.22em] text-[#f6f1e7]/55">
            © {new Date().getFullYear()} Sniper7Kills LLC · Made with ink &amp; pixels
          </p>
        </div>
        <nav className="flex flex-wrap gap-6 font-mono text-[11px] uppercase tracking-[0.2em] text-[#f6f1e7]/75">
          <a
            href="https://github.com/Sniper7Kills-LLC/Melete"
            className="border-b border-transparent transition-colors hover:border-[#c1442e] hover:text-[#c1442e]"
          >
            GitHub →
          </a>
          <Link
            to="/gallery"
            className="border-b border-transparent transition-colors hover:border-[#c1442e] hover:text-[#c1442e]"
          >
            Gallery →
          </Link>
          <Link
            to="/billing"
            className="border-b border-transparent transition-colors hover:border-[#c1442e] hover:text-[#c1442e]"
          >
            Pricing →
          </Link>
        </nav>
      </div>
    </footer>
  );
}

// ── HAND-DRAWN SVG GLYPHS ──────────────────────────────────────────
// Single-stroke, slightly imperfect — keep the "ink-on-paper" tone.

function GlyphInfinity() {
  return (
    <svg viewBox="0 0 48 48" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
      <path d="M8,24 C 8,16 18,16 24,24 C 30,32 40,32 40,24 C 40,16 30,16 24,24 C 18,32 8,32 8,24 Z" />
    </svg>
  );
}

function GlyphCalendar() {
  return (
    <svg viewBox="0 0 48 48" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
      <rect x="8" y="12" width="32" height="28" rx="1" />
      <path d="M8,20 L40,20" />
      <path d="M16,10 L16,16" />
      <path d="M32,10 L32,16" />
      <circle cx="18" cy="28" r="1.4" fill="currentColor" />
      <circle cx="26" cy="28" r="1.4" fill="currentColor" />
      <circle cx="34" cy="28" r="1.4" fill="currentColor" />
      <circle cx="18" cy="34" r="1.4" fill="currentColor" />
    </svg>
  );
}

function GlyphFolder() {
  return (
    <svg viewBox="0 0 48 48" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
      <path d="M6,14 L18,14 L22,18 L42,18 L42,38 L6,38 Z" />
      <path d="M14,24 L34,24" strokeOpacity="0.5" />
      <path d="M14,30 L28,30" strokeOpacity="0.5" />
    </svg>
  );
}
