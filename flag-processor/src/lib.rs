use wasm_bindgen::prelude::*;

// ============================================================
// Lightweight Xorshift32 PRNG
// (the `rand` crate has issues on wasm32-unknown-unknown,
//  so we ship our own deterministic generator.)
// ============================================================
struct Rng {
    state: u32,
}

impl Rng {
    fn from_seed_f32(seed: f32) -> Self {
        // splitmix32-style avalanche so adjacent seeds (e.g. 1.0 vs 1.1)
        // produce drastically different streams.
        let bits = seed.to_bits().wrapping_add(0x9E37_79B9);
        let mut x = bits;
        x ^= x >> 16;
        x = x.wrapping_mul(0x7feb_352d);
        x ^= x >> 15;
        x = x.wrapping_mul(0x846c_a68b);
        x ^= x >> 16;
        Self {
            state: if x == 0 { 0xDEAD_BEEF } else { x },
        }
    }

    #[inline]
    fn next_u32(&mut self) -> u32 {
        // Xorshift32 — period 2^32 - 1, sufficient for layout decisions.
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    #[inline]
    fn next_f32(&mut self) -> f32 {
        (self.next_u32() as f32) / (u32::MAX as f32)
    }

    #[inline]
    fn range(&mut self, n: u32) -> u32 {
        self.next_u32() % n
    }
}

// ============================================================
// Vexillological color palette (canonical flag colors)
// ============================================================
type Rgb = [u8; 3];

const PALETTE: &[Rgb] = &[
    [0, 40, 104],    // Navy Blue
    [191, 10, 48],   // Crimson
    [255, 215, 0],   // Gold
    [34, 139, 34],   // Forest Green
    [255, 255, 255], // White
    [12, 12, 12],    // Black (slight lift so fabric shading reads)
    [0, 35, 149],    // Royal Blue
    [206, 17, 38],   // Deep Red
    [255, 153, 51],  // Saffron
    [0, 153, 0],     // Pure Green
    [65, 137, 221],  // Sky Blue
    [128, 0, 32],    // Burgundy
];

/// Pick `count` distinct colors from the palette.
fn pick_distinct(rng: &mut Rng, count: usize) -> Vec<Rgb> {
    let mut chosen: Vec<Rgb> = Vec::with_capacity(count);
    let mut attempts = 0;
    while chosen.len() < count && attempts < 64 {
        let c = PALETTE[rng.range(PALETTE.len() as u32) as usize];
        if !chosen.iter().any(|p| *p == c) {
            chosen.push(c);
        }
        attempts += 1;
    }
    // Fallback: if we somehow ran out of distinct picks, fill with palette head.
    while chosen.len() < count {
        chosen.push(PALETTE[chosen.len() % PALETTE.len()]);
    }
    chosen
}

// ============================================================
// Flag layout taxonomy
// ============================================================
enum Layout {
    HorizontalStripes(u32), // 2 or 3 bands
    VerticalStripes,        // 3 bands (tricolor)
    NordicCross,            // off-center cross
    Canton,                 // solid + top-left rectangle
}

fn pick_layout(rng: &mut Rng) -> Layout {
    match rng.range(4) {
        0 => Layout::HorizontalStripes(if rng.range(2) == 0 { 2 } else { 3 }),
        1 => Layout::VerticalStripes,
        2 => Layout::NordicCross,
        _ => Layout::Canton,
    }
}

/// How many distinct base colors a layout needs.
fn colors_needed(layout: &Layout) -> usize {
    match layout {
        Layout::HorizontalStripes(n) => *n as usize,
        Layout::VerticalStripes => 3,
        Layout::NordicCross | Layout::Canton => 2,
    }
}

/// Build the deterministic "spec" for a flag from a seed.
///
/// Both `generate_procedural_texture` and `describe_layout` route through
/// here so the rasterised pixels and the SVG export are guaranteed to be
/// derived from exactly the same RNG draws.
fn build_flag_spec(seed: f32) -> (Layout, Vec<Rgb>, f32) {
    let mut rng = Rng::from_seed_f32(seed);
    let layout = pick_layout(&mut rng);
    let palette = pick_distinct(&mut rng, colors_needed(&layout));
    // Fabric phase, also seed-derived → folds shift with the slider.
    let phase = rng.next_f32() * core::f32::consts::PI * 2.0;
    (layout, palette, phase)
}

/// Resolve which color a pixel gets purely from the geometric layout.
#[inline]
fn zone_color(
    layout: &Layout,
    palette: &[Rgb],
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> Rgb {
    match layout {
        Layout::HorizontalStripes(n) => {
            let band = ((y * *n) / height).min(*n - 1);
            palette[band as usize % palette.len()]
        }
        Layout::VerticalStripes => {
            let band = ((x * 3) / width).min(2);
            palette[band as usize % palette.len()]
        }
        Layout::NordicCross => {
            // Off-center vertical bar at ~5/12 from left,
            // horizontal bar centered. Thickness ~1/7 of min dim.
            let cross_x = (width * 5) / 12;
            let cross_y = height / 2;
            let thickness = width.min(height) / 7;
            let dx = (x as i32 - cross_x as i32).abs() as u32;
            let dy = (y as i32 - cross_y as i32).abs() as u32;
            if dx < thickness || dy < thickness {
                palette[1]
            } else {
                palette[0]
            }
        }
        Layout::Canton => {
            // Canton is ~2/5 width × 1/2 height in the top-left.
            let canton_w = (width * 2) / 5;
            let canton_h = height / 2;
            if x < canton_w && y < canton_h {
                palette[1]
            } else {
                palette[0]
            }
        }
    }
}

// ============================================================
// Fabric folds — softened "flat-nylon" pass.
//
// Previous iteration mapped to roughly [0.55, 1.20] (a 65 % swing)
// and used `|wave|^0.45` for the body, which sharpened zero-crossings
// into hard creases. The result read as glossy clay, not cloth.
//
// This pass keeps the directional drape geometry but:
//   • drops the abs/pow shaping in favour of a *linear* sinusoidal body,
//     which is what a gently lit flat fabric actually looks like;
//   • lowers the primary frequency so folds are broader and calmer;
//   • compresses the multiplier range to ~[0.90, 1.04] (~14 % swing),
//     about a quarter of the original amplitude;
//   • keeps a narrow specular highlight, but at low amplitude so it
//     reads as nylon sheen rather than vinyl reflection.
// ============================================================
#[inline]
fn fabric_modifier(fx: f32, fy: f32, phase: f32) -> f32 {
    // Primary vertical drapes, warped horizontally by a slow Y-sine.
    // Lower frequency than before (26 vs 44) → broader, softer folds.
    let primary =
        (fx * 26.0 + (fy * 6.0 + phase).sin() * 1.2 + phase * 0.3).cos();

    // Subtle diagonal cross-bands (different freq → no resonance).
    let cross = (fx * 9.0 + fy * 14.0 + phase * 0.7).cos();

    // Combined directional wave in roughly [-1, 1].
    let wave = primary * 0.8 + cross * 0.2;

    // Linear body shading. `0.5 + 0.5 * wave` is in [0, 1] and varies
    // smoothly — no abs/pow shaping → no harsh creases.
    let body = 0.5 + 0.5 * wave;

    // Narrow, low-amplitude specular pop on positive crests.
    let spec = wave.max(0.0).powf(10.0);

    // Final per-channel multiplier: gentle ripple, not 3D plastic.
    0.90 + body * 0.10 + spec * 0.04
}

#[inline]
fn shade_channel(channel: u8, modifier: f32) -> u8 {
    (channel as f32 * modifier).clamp(0.0, 255.0) as u8
}

// ============================================================
// Public WASM API
// ============================================================

/// Rasterise a flag into an RGBA byte buffer.
///
/// When `enable_fabric_shading` is `false`, the geometric zone colors
/// are emitted verbatim — pure flat output suitable for vector export
/// and crisp UI previews. When `true`, the softened fabric shader is
/// applied per pixel.
#[wasm_bindgen]
pub fn generate_procedural_texture(
    width: u32,
    height: u32,
    seed: f32,
    enable_fabric_shading: bool,
) -> Vec<u8> {
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let (layout, palette, phase) = build_flag_spec(seed);

    let len = (width as usize) * (height as usize) * 4;
    let mut pixels = Vec::with_capacity(len);

    let w = width as f32;
    let h = height as f32;

    for y in 0..height {
        let fy = y as f32 / h;
        for x in 0..width {
            let fx = x as f32 / w;

            // Step A — base color from geometric layout.
            let base = zone_color(&layout, &palette, x, y, width, height);

            if enable_fabric_shading {
                // Step B — soft fabric modulation per pixel.
                let m = fabric_modifier(fx, fy, phase);
                pixels.push(shade_channel(base[0], m));
                pixels.push(shade_channel(base[1], m));
                pixels.push(shade_channel(base[2], m));
            } else {
                // Flat mode — pure geometric colors, no shading.
                pixels.push(base[0]);
                pixels.push(base[1]);
                pixels.push(base[2]);
            }
            pixels.push(255);
        }
    }

    pixels
}

/// Describe the layout and palette that `generate_procedural_texture`
/// would produce for the given seed.
///
/// Returns a small JSON document the JS side can parse and turn into a
/// pure-vector SVG. Format:
///
/// ```json
/// { "type": "horizontal_stripes", "bands": 3, "colors": ["#aabbcc", ...] }
/// { "type": "vertical_stripes",   "bands": 3, "colors": [ ... ] }
/// { "type": "nordic_cross",                     "colors": [bg, cross] }
/// { "type": "canton",                           "colors": [field, canton] }
/// ```
#[wasm_bindgen]
pub fn describe_layout(seed: f32) -> String {
    let (layout, palette, _phase) = build_flag_spec(seed);

    let colors_json = palette
        .iter()
        .map(|rgb| format!("\"#{:02x}{:02x}{:02x}\"", rgb[0], rgb[1], rgb[2]))
        .collect::<Vec<_>>()
        .join(",");

    match layout {
        Layout::HorizontalStripes(n) => format!(
            "{{\"type\":\"horizontal_stripes\",\"bands\":{},\"colors\":[{}]}}",
            n, colors_json
        ),
        Layout::VerticalStripes => format!(
            "{{\"type\":\"vertical_stripes\",\"bands\":3,\"colors\":[{}]}}",
            colors_json
        ),
        Layout::NordicCross => format!(
            "{{\"type\":\"nordic_cross\",\"colors\":[{}]}}",
            colors_json
        ),
        Layout::Canton => format!(
            "{{\"type\":\"canton\",\"colors\":[{}]}}",
            colors_json
        ),
    }
}

#[wasm_bindgen]
pub fn apply_flag_filter(mut pixels: Vec<u8>, filter_type: String) -> Vec<u8> {
    let len = pixels.len();
    let num_pixels = len / 4;

    match filter_type.as_str() {
        "grayscale" => {
            for i in 0..num_pixels {
                let base = i * 4;
                let lum = (pixels[base] as f32 * 0.299
                    + pixels[base + 1] as f32 * 0.587
                    + pixels[base + 2] as f32 * 0.114) as u8;
                pixels[base] = lum;
                pixels[base + 1] = lum;
                pixels[base + 2] = lum;
            }
        }
        "invert" => {
            for i in 0..num_pixels {
                let base = i * 4;
                pixels[base] = 255 - pixels[base];
                pixels[base + 1] = 255 - pixels[base + 1];
                pixels[base + 2] = 255 - pixels[base + 2];
            }
        }
        "vignette" => {
            // Recover dimensions from pixel count assuming square-ish buffer.
            let side = (num_pixels as f32).sqrt();
            let width = side as u32;
            let height = if width > 0 {
                num_pixels as u32 / width
            } else {
                1
            };

            let cx = width as f32 / 2.0;
            let cy = height as f32 / 2.0;
            let max_dist = (cx * cx + cy * cy).sqrt();

            for i in 0..num_pixels {
                let x = (i as u32 % width) as f32;
                let y = (i as u32 / width) as f32;
                let dx = x - cx;
                let dy = y - cy;
                let dist = (dx * dx + dy * dy).sqrt() / max_dist;
                let factor = (1.0 - (dist * dist * 1.2)).max(0.0);

                let base = i * 4;
                pixels[base] = (pixels[base] as f32 * factor) as u8;
                pixels[base + 1] = (pixels[base + 1] as f32 * factor) as u8;
                pixels[base + 2] = (pixels[base + 2] as f32 * factor) as u8;
            }
        }
        _ => {} // unknown filter — return unchanged
    }

    pixels
}
