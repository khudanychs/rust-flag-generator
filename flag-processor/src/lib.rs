use wasm_bindgen::prelude::*;

// ============================================================
// Xorshift32 PRNG
// ============================================================
struct Rng { state: u32 }

impl Rng {
    fn from_seed_f32(seed: f32) -> Self {
        let bits = seed.to_bits().wrapping_add(0x9E37_79B9);
        let mut x = bits;
        x ^= x >> 16; x = x.wrapping_mul(0x7feb_352d);
        x ^= x >> 15; x = x.wrapping_mul(0x846c_a68b);
        x ^= x >> 16;
        Self { state: if x == 0 { 0xDEAD_BEEF } else { x } }
    }
    #[inline] fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13; x ^= x >> 17; x ^= x << 5;
        self.state = x; x
    }
    #[inline] fn next_f32(&mut self) -> f32 { (self.next_u32() as f32) / (u32::MAX as f32) }
    #[inline] fn range(&mut self, n: u32) -> u32 { self.next_u32() % n }
}

// ============================================================
// Palette — Rule of Tincture
// ============================================================
type Rgb = [u8; 3];

const METALS: &[Rgb] = &[
    [255, 255, 255],
    [255, 215, 0],
    [255, 153, 51],
];

const COLORS: &[Rgb] = &[
    [0, 40, 104],
    [191, 10, 48],
    [34, 139, 34],
    [12, 12, 12],
    [0, 35, 149],
    [206, 17, 38],
    [0, 153, 0],
    [65, 137, 221],
    [128, 0, 32],
];

#[inline]
fn color_dist_sq(a: Rgb, b: Rgb) -> f32 {
    let dr = a[0] as f32 - b[0] as f32;
    let dg = a[1] as f32 - b[1] as f32;
    let db = a[2] as f32 - b[2] as f32;
    dr * dr + dg * dg + db * db
}

fn pick_tincture(rng: &mut Rng, count: usize) -> Vec<Rgb> {
    let mut out: Vec<Rgb> = Vec::with_capacity(count);
    let start_metal = rng.range(2) == 0;
    let first = if start_metal {
        METALS[rng.range(METALS.len() as u32) as usize]
    } else {
        COLORS[rng.range(COLORS.len() as u32) as usize]
    };
    out.push(first);
    let mut is_metal = start_metal;
    for _ in 1..count {
        is_metal = !is_metal;
        let pool: &[Rgb] = if is_metal { METALS } else { COLORS };
        let prev = *out.last().unwrap();
        let threshold = 160.0 * 160.0;
        let mut best = pool[rng.range(pool.len() as u32) as usize];
        for _ in 0..16 {
            let c = pool[rng.range(pool.len() as u32) as usize];
            if color_dist_sq(c, prev) > threshold { best = c; break; }
        }
        out.push(best);
    }
    out
}

// ============================================================
// Layout taxonomy with randomized internal parameters
// ============================================================
#[derive(Clone)]
enum Layout {
    HorizontalStripes { bands: u32, wide_middle: bool },
    VerticalStripes { wide_middle: bool },
    NordicCross { thickness_frac: f32 },
    Canton,
    RisingSun { rays: u32, cx_frac: f32, cy_frac: f32 },
    NordicStar { thickness_frac: f32, star_scale: f32 },
    Chevron,
}

fn pick_layout(rng: &mut Rng) -> Layout {
    match rng.range(7) {
        0 => Layout::HorizontalStripes {
            bands: if rng.range(2) == 0 { 2 } else { 3 },
            wide_middle: rng.range(3) == 0, // 1-in-3 chance of wide middle
        },
        1 => Layout::VerticalStripes {
            wide_middle: rng.range(3) == 0,
        },
        2 => Layout::NordicCross {
            thickness_frac: 0.10 + rng.next_f32() * 0.08, // 10–18% of min dim
        },
        3 => Layout::Canton,
        4 => {
            // Rising sun: 6–24 rays, origin anywhere in the flag
            let origins: [(f32, f32); 5] = [
                (0.5, 0.5), (0.0, 1.0), (0.0, 0.5), (0.5, 1.0), (0.25, 0.75),
            ];
            let o = origins[rng.range(origins.len() as u32) as usize];
            Layout::RisingSun {
                rays: rng.range(19) + 6,
                cx_frac: o.0,
                cy_frac: o.1,
            }
        }
        5 => Layout::NordicStar {
            thickness_frac: 0.10 + rng.next_f32() * 0.08,
            star_scale: 0.08 + rng.next_f32() * 0.10, // 8–18% of min dim
        },
        _ => Layout::Chevron,
    }
}

fn colors_needed(layout: &Layout) -> usize {
    match layout {
        Layout::HorizontalStripes { bands, .. } => *bands as usize,
        Layout::VerticalStripes { .. } => 3,
        Layout::NordicCross { .. } | Layout::Canton | Layout::Chevron => 2,
        Layout::RisingSun { .. } => 2,
        Layout::NordicStar { .. } => 3,
    }
}

// ============================================================
// SDF: 5-pointed star (negative inside)
// ============================================================
#[inline]
fn sdf_star5(px: f32, py: f32, cx: f32, cy: f32, r: f32) -> f32 {
    let x = (px - cx) / r;
    let y = (py - cy) / r;
    const AN: f32 = 0.628_318; // pi/5
    let mut angle = y.atan2(x);
    if angle < 0.0 { angle += core::f32::consts::TAU; }
    let sector = AN * 2.0;
    angle = angle % sector;
    if angle > AN { angle = sector - angle; }
    let len = (x * x + y * y).sqrt();
    let sx = len * angle.cos();
    let sy = len * angle.sin();
    let inner_r: f32 = 0.38;
    let lx = 1.0 - inner_r * AN.cos();
    let ly = inner_r * AN.sin();
    let ll = (lx * lx + ly * ly).sqrt();
    let nx = ly / ll;
    let ny = -lx / ll;
    ((sx - 1.0) * nx + sy * ny) * r
}

// ============================================================
// Zone color at continuous (floating-point) coordinates
// ============================================================
#[inline]
fn zone_color_f(layout: &Layout, palette: &[Rgb], fx: f32, fy: f32, w: f32, h: f32) -> Rgb {
    match layout {
        Layout::HorizontalStripes { bands, wide_middle } => {
            let n = *bands;
            let band = if *wide_middle && n == 3 {
                // Middle band is 50%, outer bands 25% each
                if fy < 0.25 { 0 } else if fy < 0.75 { 1 } else { 2 }
            } else {
                ((fy * n as f32) as u32).min(n - 1)
            };
            palette[band as usize % palette.len()]
        }
        Layout::VerticalStripes { wide_middle } => {
            let band = if *wide_middle {
                if fx < 0.25 { 0 } else if fx < 0.75 { 1 } else { 2 }
            } else {
                ((fx * 3.0) as u32).min(2)
            };
            palette[band as usize % palette.len()]
        }
        Layout::NordicCross { thickness_frac } => {
            let cross_x = w * (5.0 / 12.0);
            let cross_y = h * 0.5;
            let thickness = w.min(h) * thickness_frac;
            let dx = (fx * w - cross_x).abs();
            let dy = (fy * h - cross_y).abs();
            if dx < thickness || dy < thickness { palette[1] } else { palette[0] }
        }
        Layout::Canton => {
            let cw = 0.4;
            let ch = 0.5;
            if fx < cw && fy < ch { palette[1] } else { palette[0] }
        }
        Layout::RisingSun { rays, cx_frac, cy_frac } => {
            let cx = w * cx_frac;
            let cy = h * cy_frac;
            let dx = fx * w - cx;
            let dy = fy * h - cy;
            let mut angle = dy.atan2(dx);
            if angle < 0.0 { angle += core::f32::consts::TAU; }
            let sector = core::f32::consts::TAU / (*rays as f32);
            let idx = (angle / sector) as u32 % 2;
            palette[idx as usize]
        }
        Layout::NordicStar { thickness_frac, star_scale } => {
            let cross_x = w * (5.0 / 12.0);
            let cross_y = h * 0.5;
            let thickness = w.min(h) * thickness_frac;
            let dx = (fx * w - cross_x).abs();
            let dy = (fy * h - cross_y).abs();
            let on_cross = dx < thickness || dy < thickness;
            let star_r = w.min(h) * star_scale;
            let d = sdf_star5(fx * w, fy * h, cross_x, cross_y, star_r);
            if d < 0.0 { palette[2] } else if on_cross { palette[1] } else { palette[0] }
        }
        Layout::Chevron => {
            let apex_x = 0.4;
            let rel_y = (fy - 0.5).abs() / 0.5;
            let edge_x = apex_x * (1.0 - rel_y);
            if fx < edge_x { palette[1] } else { palette[0] }
        }
    }
}

// ============================================================
// Fabric deformation & shading
// ============================================================
#[inline]
fn fabric_deform(fx: f32, fy: f32, phase: f32) -> (f32, f32) {
    let dx = (fx * 18.0 + (fy * 5.0 + phase).sin() * 1.5 + phase * 0.3).sin() * 0.006;
    let dy = (fy * 12.0 + fx * 4.0 + phase * 0.5).sin() * 0.003;
    (fx + dx, fy + dy)
}

#[inline]
fn fabric_shade(fx: f32, fy: f32, phase: f32) -> f32 {
    let primary = (fx * 26.0 + (fy * 6.0 + phase).sin() * 1.2 + phase * 0.3).cos();
    let cross = (fx * 9.0 + fy * 14.0 + phase * 0.7).cos();
    let wave = primary * 0.8 + cross * 0.2;
    let body = 0.5 + 0.5 * wave;
    let spec = wave.max(0.0).powf(10.0);
    0.90 + body * 0.10 + spec * 0.04
}

#[inline]
fn shade_channel(ch: u8, m: f32) -> u8 { (ch as f32 * m).clamp(0.0, 255.0) as u8 }

// ============================================================
// Shared spec builder
// ============================================================
fn build_flag_spec(seed: f32) -> (Layout, Vec<Rgb>, f32) {
    let mut rng = Rng::from_seed_f32(seed);
    let layout = pick_layout(&mut rng);
    let palette = pick_tincture(&mut rng, colors_needed(&layout));
    let phase = rng.next_f32() * core::f32::consts::TAU;
    (layout, palette, phase)
}

// ============================================================
// 2×2 SSAA sub-pixel offsets
// ============================================================
const SSAA_OFFSETS: [(f32, f32); 4] = [
    (0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75),
];

// ============================================================
// Public WASM API
// ============================================================
#[wasm_bindgen]
pub fn generate_procedural_texture(
    width: u32, height: u32, seed: f32, enable_fabric_shading: bool,
) -> Vec<u8> {
    if width == 0 || height == 0 { return Vec::new(); }

    let (layout, palette, phase) = build_flag_spec(seed);
    let len = (width as usize) * (height as usize) * 4;
    let mut pixels = Vec::with_capacity(len);
    let w = width as f32;
    let h = height as f32;

    for y in 0..height {
        for x in 0..width {
            // 2×2 SSAA: sample geometry at 4 sub-pixel positions, average.
            let mut r_acc: u32 = 0;
            let mut g_acc: u32 = 0;
            let mut b_acc: u32 = 0;

            for &(ox, oy) in &SSAA_OFFSETS {
                let sfx = (x as f32 + ox) / w;
                let sfy = (y as f32 + oy) / h;

                let (lfx, lfy) = if enable_fabric_shading {
                    fabric_deform(sfx, sfy, phase)
                } else {
                    (sfx, sfy)
                };

                let c = zone_color_f(&layout, &palette, lfx, lfy, w, h);
                r_acc += c[0] as u32;
                g_acc += c[1] as u32;
                b_acc += c[2] as u32;
            }

            let r = (r_acc / 4) as u8;
            let g = (g_acc / 4) as u8;
            let b = (b_acc / 4) as u8;

            if enable_fabric_shading {
                // Apply shade AFTER averaging for consistent sheen
                let fx = (x as f32 + 0.5) / w;
                let fy = (y as f32 + 0.5) / h;
                let m = fabric_shade(fx, fy, phase);
                pixels.push(shade_channel(r, m));
                pixels.push(shade_channel(g, m));
                pixels.push(shade_channel(b, m));
            } else {
                pixels.push(r);
                pixels.push(g);
                pixels.push(b);
            }
            pixels.push(255);
        }
    }
    pixels
}

/// JSON layout descriptor for SVG export.
#[wasm_bindgen]
pub fn describe_layout(seed: f32) -> String {
    let (layout, palette, _) = build_flag_spec(seed);

    let colors_json = palette.iter()
        .map(|c| format!("\"#{:02x}{:02x}{:02x}\"", c[0], c[1], c[2]))
        .collect::<Vec<_>>().join(",");

    match layout {
        Layout::HorizontalStripes { bands, wide_middle } => format!(
            "{{\"type\":\"horizontal_stripes\",\"bands\":{},\"wideMiddle\":{},\"colors\":[{}]}}",
            bands, wide_middle, colors_json),
        Layout::VerticalStripes { wide_middle } => format!(
            "{{\"type\":\"vertical_stripes\",\"bands\":3,\"wideMiddle\":{},\"colors\":[{}]}}",
            wide_middle, colors_json),
        Layout::NordicCross { thickness_frac } => format!(
            "{{\"type\":\"nordic_cross\",\"thicknessFrac\":{:.4},\"colors\":[{}]}}",
            thickness_frac, colors_json),
        Layout::Canton => format!(
            "{{\"type\":\"canton\",\"colors\":[{}]}}", colors_json),
        Layout::RisingSun { rays, cx_frac, cy_frac } => format!(
            "{{\"type\":\"rising_sun\",\"rays\":{},\"cxFrac\":{:.4},\"cyFrac\":{:.4},\"colors\":[{}]}}",
            rays, cx_frac, cy_frac, colors_json),
        Layout::NordicStar { thickness_frac, star_scale } => format!(
            "{{\"type\":\"nordic_star\",\"thicknessFrac\":{:.4},\"starScale\":{:.4},\"colors\":[{}]}}",
            thickness_frac, star_scale, colors_json),
        Layout::Chevron => format!(
            "{{\"type\":\"chevron\",\"colors\":[{}]}}", colors_json),
    }
}

#[wasm_bindgen]
pub fn apply_flag_filter(mut pixels: Vec<u8>, filter_type: String) -> Vec<u8> {
    let len = pixels.len();
    let num_pixels = len / 4;

    match filter_type.as_str() {
        "grayscale" => {
            for i in 0..num_pixels {
                let b = i * 4;
                let lum = (pixels[b] as f32 * 0.299
                    + pixels[b+1] as f32 * 0.587
                    + pixels[b+2] as f32 * 0.114) as u8;
                pixels[b] = lum; pixels[b+1] = lum; pixels[b+2] = lum;
            }
        }
        "invert" => {
            for i in 0..num_pixels {
                let b = i * 4;
                pixels[b] = 255 - pixels[b];
                pixels[b+1] = 255 - pixels[b+1];
                pixels[b+2] = 255 - pixels[b+2];
            }
        }
        "vignette" => {
            let side = (num_pixels as f32).sqrt();
            let width = side as u32;
            let height = if width > 0 { num_pixels as u32 / width } else { 1 };
            let cx = width as f32 / 2.0;
            let cy = height as f32 / 2.0;
            let max_dist = (cx * cx + cy * cy).sqrt();
            for i in 0..num_pixels {
                let px = (i as u32 % width) as f32;
                let py = (i as u32 / width) as f32;
                let d = ((px-cx)*(px-cx) + (py-cy)*(py-cy)).sqrt() / max_dist;
                let f = (1.0 - d * d * 1.2).max(0.0);
                let b = i * 4;
                pixels[b] = (pixels[b] as f32 * f) as u8;
                pixels[b+1] = (pixels[b+1] as f32 * f) as u8;
                pixels[b+2] = (pixels[b+2] as f32 * f) as u8;
            }
        }
        _ => {}
    }
    pixels
}
