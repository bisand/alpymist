# Alpymist logo kit

Four designs came out of the logo exploration and all four were kept. They
share one idea: a tall Alpine peak, a lower companion to its right, and a broad
sweep of mist passing through both. The mist is deliberately wide enough to
survive tray-icon sizes.

`alpymist-logos.png` is the sheet those four were approved from.
`variants/alpymist-variants.png` is the same four rendered from the vector
masters, which is what everything else is built from.

| Variant | Shape | Where it is used |
| --- | --- | --- |
| `variants/alpymist-halo-mist` | The peaks cut out of a disc, closed at the bottom by the mist's own arc. | The product mark: the boot menus, the boot splash, the bar, and the site's favicon. |
| `variants/alpymist-layered-ridge` | Two ranges with an ink-dark mist band between them. | — |
| `variants/alpymist-open-ridge` | An open outline with an inner slope rising through it. | — |
| `variants/alpymist-blue-mist` | The brighter alpine-blue peak with a dark mist band. | — |

Each variant is an SVG master with a PNG rendered from it. The SVGs are the
masters: change one and re-render its PNG, never the other way round.

## The halo mark in the product

The halo variant is drawn from geometry rather than shipped as a bitmap, so the
same mark is exact at 14 px in the bar and at a quarter of the screen on the
splash. The geometry lives in `crates/alpymist-ui/src/badge.rs`, traced from
`variants/alpymist-halo-mist.svg`:

- **The bar** — `alpymist-wallpaper --mark` writes the coverage mask out as a
  transparent PNG in the accent colour, so the bar's background shows through
  the peaks. The `alpymist-desktop` aport does this at build time.
- **The boot splash** — `alpymist_ui::render::paint_badge` fills it opaquely in
  four flat palette colours, above the wordmark.
- **The boot menus** — `alpymist-wallpaper --boot` puts the badge over the
  system picture (`wallpapers/milky-way.jpg`, the one the splash and the login
  screen show),
  above the space the menu uses. The `alpymist-splash-boot` subpackage
  carries the result, and `profiles/mkimg.alpymist.sh` puts it behind both the
  syslinux and the GRUB menu. Two files: a 640x480 PNG for syslinux's fixed
  VESA mode, and a 1280x800 JPEG that GRUB stretches to the panel — GRUB on
  arm64 has no PNG decoder.
- **The site's favicon** — `site/static/favicon.svg`, the only place the SVG is
  used as it is. Its viewBox crops to the disc so the mark fills the square:
  the disc is the favicon's own container, and setting it on a panel only
  loses its lower edge into the panel.

Nothing else in the product reads an SVG at runtime.

## Colour

- Sky high / ink: `#0b121e`
- Surface: `#0f1827`
- Line: `#1d2a3b`
- Sky low: `#3a4c63`
- Accent deep: `#3b6e8d`
- Mist: `#afc2d6`
- Alpine accent: `#7fb8d9`

Keep clear space of at least the height of the mist band around every edge. The
masters use only subtle gradients between existing theme colours; do not add
outlines, shadows, or extra effects. For raster use, export at 1024 px and
downsample.

## The kit

Every file here is built on the halo variant. The marks are square; the lockups
are the mark with the wordmark beside it, baseline aligned to the disc's centre.
`png/` holds a raster of each, exported from the SVG beside it.

| Asset | Intended use |
| --- | --- |
| `alpymist-mark.svg` | Single-ink master. It inherits `currentColor`: black on light surfaces, white on dark. |
| `alpymist-mark-reverse.svg` | The same, ready-inked in mist grey for dark surfaces. |
| `alpymist-mark-color.svg` | Colour mark for dark surfaces. `-dark` is an alias of it. |
| `alpymist-mark-color-light.svg` | Colour mark for light surfaces. |
| `alpymist-lockup.svg` | Colour lockup for light surfaces. |
| `alpymist-lockup-dark.svg` | Colour lockup for dark surfaces. |
| `alpymist-lockup-mono.svg` | Single-ink lockup; inherits `currentColor`. |
| `alpymist-lockup-reverse.svg` | The same, ready-inked in mist grey. |
| `alpymist-app-icon.svg` | Application icon: the mark in the accent colour on an ink panel. |

Two things about the halo are worth knowing before editing any of these.

**The peaks are a hole, not a shape.** On a dark surface the mark's mountains
are the surface showing through, which is why the dark-surface files knock them
out and the light-surface ones ink them in with `#0b121e` instead. That also
means the two have different outlines: knocked out, the disc appears to have a
bite taken out of its lower right, because the mountains there are the same
colour as everything outside the mark. Inked, it is a whole disc. Both are
correct; they are the same drawing on different surfaces.

**The app icon is the one place the mark is not in its own colours.** At the
size a launcher draws an icon the full-colour disc is a dark blob against a
dark panel, so the icon uses the accent knockout the bar uses. If you would
rather have the literal artwork, swap the panel and the `color` attribute for
the contents of `alpymist-mark-color.svg`.

The wordmark is set in Fira Sans SemiBold, falling back through Avenir Next,
Avenir, Helvetica and Arial. It is live text rather than outlines, so a
renderer without Fira Sans will set it slightly differently — which is why the
lockup viewBox leaves room to the right of the word.

## Wallpapers

`wallpapers/` holds the photographs and paintings Alpymist ships as desktop
backgrounds, and the one the boot shows. Its README says where they came
from, how they were cropped, and what a new one has to be.
