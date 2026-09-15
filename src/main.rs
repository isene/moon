//! moon — the Moon as it looks tonight.
//!
//! The near side, lit for the phase of the moment, drawn with half-block
//! cells so every cell holds two pixels. A strip along the bottom shows
//! the days around the one on screen. `m` opens a braille map to zoom and
//! pan, with the features named. Nothing runs between key presses.

use std::f64::consts::PI;
use std::sync::OnceLock;

use crust::{style, Crust, Input, Pane};

/// The near side, 512×512 8-bit gray, orthographic, north up. Built from
/// NASA's LRO LROC WAC mosaic (CGI Moon Kit, public domain).
const MAP: &[u8] = include_bytes!("../img/moon-512.gray");
const MAP_N: usize = 512;
const SYNODIC: f64 = 29.530_588_853;
/// How bright the night side stays, as a share of daylight (earthshine).
const NIGHT: f32 = 0.22;
/// Map value (0..1) that counts as white; the brightest highlands sit
/// just under it, so the lit side reads light gray and white.
const WHITE: f32 = 0.72;
/// How fast light ramps up past the terminator; higher is a sharper line.
const RAMP: f32 = 5.0;
/// Strip: pixels across each phase symbol, cells per slot, days before
/// the one on screen.
const MINI: usize = 12;
const SLOT: usize = 14;
const PAST: i64 = 3;

/// The near side again at 2048×2048 for the map, from NASA's 8k LROC
/// mosaic shaded with LOLA elevation lit from the north-west, so craters
/// show their rims. zlib, each row stored as its difference from the row
/// above.
const MAP_BIG: &[u8] = include_bytes!("../img/moon-2048.z");
const BIG_N: usize = 2048;
/// Named near-side features from the IAU Gazetteer of Planetary
/// Nomenclature, biggest first: name, kind (p plain, c crater, o other),
/// latitude, longitude (east positive), size in km.
const FEATURES: &str = include_str!("../img/features.tsv");
const MOON_KM: f32 = 3474.8;
/// At 16× a map pixel is about one and a half sub-pixels; past that it
/// only gets blurrier.
const ZOOMS: [f32; 9] = [1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 12.0, 16.0];
/// Map brightness that shows as black; the darkest maria sit just above.
const DARK: f32 = 0.12;
/// How much brighter than its cell a sub-pixel must be to get a dot.
const EDGE: f32 = 0.04;
/// Map value (0..1) that counts as white on the shaded map, where only
/// sunward crater walls go brighter.
const MAP_WHITE: f32 = 0.9;

/// Where the map looks: a step in `ZOOMS`, and the centre as a point on
/// the disk (-1..1, east and north positive).
#[derive(Clone, Copy, Default)]
struct View { zi: usize, cx: f32, cy: f32 }

impl View {
    fn zoom(&self) -> f32 { ZOOMS[self.zi] }
    /// Keep the centre on the disk.
    fn clamp(&mut self) {
        let r = (self.cx * self.cx + self.cy * self.cy).sqrt();
        if r > 1.0 { self.cx /= r; self.cy /= r; }
    }
}

struct Feature { name: String, kind: u8, lat: f32, lon: f32, km: f32 }

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("moon — The Moon as it looks tonight (Fe2O3 suite)");
        println!();
        println!("Usage: moon");
        println!();
        println!("Keys: ← → / h l  day back / forward    t  today    m  map    q  quit");
        println!("Map:  + -  zoom    arrows / h j k l  pan    0  reset    m  back");
        return;
    }
    if args.iter().any(|a| a == "-v" || a == "--version") {
        println!("moon {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    Crust::init();
    Crust::set_app_identity("Moon");
    Crust::clear_screen();
    let mut offset: i64 = 0;
    let mut map = false;
    let mut view = View::default();
    render(offset, map, view);
    loop {
        let Some(key) = Input::getchr(None) else { continue };
        if map {
            let step = 0.3 / view.zoom();
            match key.as_str() {
                "q" | "Q" => break,
                "m" | "ESC" => map = false,
                "+" | "=" => view.zi = (view.zi + 1).min(ZOOMS.len() - 1),
                "-" => {
                    view.zi = view.zi.saturating_sub(1);
                    if view.zi == 0 { view = View::default(); }
                }
                "0" => view = View::default(),
                "h" | "LEFT" => view.cx -= step,
                "l" | "RIGHT" => view.cx += step,
                "k" | "UP" => view.cy += step,
                "j" | "DOWN" => view.cy -= step,
                "RESIZE" => {}
                _ => continue,
            }
            view.clamp();
        } else {
            match key.as_str() {
                "q" | "Q" => break,
                "h" | "LEFT" => offset -= 1,
                "l" | "RIGHT" => offset += 1,
                "t" => offset = 0,
                "m" => map = true,
                "RESIZE" => {}
                _ => continue,
            }
        }
        render(offset, map, view);
    }
    Crust::cleanup();
}

fn render(offset: i64, map: bool, view: View) {
    if map { render_map(view) } else { render_phase(offset) }
}

/// The top bar: facts on the left, keys and version on the right. The
/// keys always show; a narrow window loses facts from the end.
fn header(cols: usize, mut facts: Vec<String>, keys: &str) {
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let right_w = keys.chars().count() + 3 + version.chars().count() + 1;
    let mut left = format!(" {}", facts.join("   "));
    while facts.len() > 1 && left.chars().count() + right_w + 2 > cols {
        facts.pop();
        left = format!(" {}", facts.join("   "));
    }
    let pad = cols.saturating_sub(left.chars().count() + right_w).max(1);
    let mut bar = Pane::new(1, 1, cols as u16, 1, 255, 236);
    bar.wrap = false;
    bar.scroll = false;
    bar.set_text(&format!("{left}{}{keys}   {} ", " ".repeat(pad), style::fg(&version, 245)));
    bar.refresh();
}

/// Paint the whole screen for the day `offset` days from today.
fn render_phase(offset: i64) {
    let (cols, rows) = Crust::terminal_size();
    let (cols, rows) = (cols as usize, rows as usize);
    let (today, hours) = now_local();
    let day = today + offset;
    let strip_h = MINI / 2 + 2;
    let main_h = rows.saturating_sub(1 + strip_h).max(1);

    let f = phase_at(day, hours);
    let (y, m, d) = civil_from_days(day);
    header(cols, vec![
        format!("{} {} {} {}", WEEKDAYS[weekday(day)], d, MONTHS[(m - 1) as usize], y),
        phase_name(f).to_string(),
        format!("{}% lit", (lit_fraction(f) * 100.0).round()),
        format!("{:.1} days old", f * SYNODIC),
        until(f, 0.5, "full"),
        until(f, 0.0, "new"),
    ], "← → day   t today   m map   q quit");

    let diam = cols.saturating_sub(2).min(main_h * 2).max(2);
    let mut main = Pane::new(1, 2, cols as u16, main_h as u16, 255, 16);
    main.wrap = false;
    main.scroll = false;
    main.set_text(&draw_moon(f, diam, cols, main_h).join("\n"));
    main.refresh();

    let slots = (cols / SLOT).max(1);
    let mini_rows = MINI / 2;
    let mut lines: Vec<String> = vec![String::new(); mini_rows];
    let mut labels = String::new();
    let w = SLOT;
    for i in 0..slots {
        let sd = day - PAST + i as i64;
        for (r, l) in draw_symbol(phase_at(sd, hours), MINI, SLOT, mini_rows).into_iter().enumerate() {
            lines[r].push_str(&l);
        }
        let (_, _, dd) = civil_from_days(sd);
        let text = format!("{:^w$}", format!("{} {}", WEEKDAYS[weekday(sd)], dd));
        labels.push_str(&if sd == today {
            style::styled(&text, Some(226), None, "b")
        } else if sd == day {
            style::styled(&text, Some(255), None, "b")
        } else {
            style::fg(&text, 245)
        });
    }
    let mut strip = Pane::new(1, (2 + main_h) as u16, cols as u16, strip_h as u16, 250, 16);
    strip.wrap = false;
    strip.scroll = false;
    strip.set_text(&format!("\n{}\n{}", lines.join("\n"), labels));
    strip.refresh();
}

/// The Moon at cycle fraction `f`, `diam` pixels across, centred in
/// `width` cells by `rows` rows, with craters and maria from the map.
fn draw_moon(f: f64, diam: usize, width: usize, rows: usize) -> Vec<String> {
    let box_px = ((MAP_N as f32 / diam as f32).round() as usize).max(1);
    paint(f, diam, width, rows, |x, y, sunward| {
        let lit = (sunward * RAMP).clamp(0.0, 1.0);
        let (ax, ay) = { let rr = (x * x + y * y).sqrt(); if rr > 0.99 { (x / rr * 0.99, y / rr * 0.99) } else { (x, y) } };
        albedo(ax, ay, box_px) * (NIGHT + (1.0 - NIGHT) * lit)
    })
}

/// The phase as a flat symbol: one light gray for the lit part, one dark
/// gray for the rest, a sharp line between them.
fn draw_symbol(f: f64, diam: usize, width: usize, rows: usize) -> Vec<String> {
    paint(f, diam, width, rows, |_, _, sunward| {
        let t = (sunward * 12.0 + 0.5).clamp(0.0, 1.0);
        0.25 + (0.88 - 0.25) * t
    })
}

/// Draw a disk `diam` pixels across in `width` × `rows` cells. A cell is
/// one pixel wide and two tall: the top pixel is its foreground, the
/// bottom its background. `shade(x, y, sunward)` gives each pixel's
/// brightness from its place on the disk (-1..1) and how far it faces
/// the Sun (-1..1, the terminator at 0).
fn paint(f: f64, diam: usize, width: usize, rows: usize, shade: impl Fn(f32, f32, f32) -> f32) -> Vec<String> {
    let r = diam as f32 / 2.0;
    let cx = width as f32 / 2.0;
    let cy = rows as f32;
    // Where the Sun is, seen from the Moon's centre with the viewer on
    // +z: behind the Moon at new, to the right at first quarter.
    let sun = ((f * 2.0 * PI).sin() as f32, -((f * 2.0 * PI).cos()) as f32);
    let pixel = |px: usize, py: usize| -> Option<(u8, u8, u8)> {
        let x = (px as f32 + 0.5 - cx) / r;
        let y = (cy - py as f32 - 0.5) / r;
        let rr = (x * x + y * y).sqrt();
        let cover = ((1.0 - rr) * r + 0.5).clamp(0.0, 1.0);
        if cover <= 0.0 { return None; }
        let z = (1.0 - rr * rr).max(0.0).sqrt();
        let v = shade(x, y, x * sun.0 + z * sun.1) * cover;
        let g = (v * 255.0).round().clamp(0.0, 255.0) as u8;
        Some((g, g, g))
    };
    (0..rows)
        .map(|row| {
            let mut line = String::with_capacity(width * 24);
            for col in 0..width {
                match (pixel(col, row * 2), pixel(col, row * 2 + 1)) {
                    (None, None) => line.push(' '),
                    (Some(t), Some(b)) => line.push_str(&style::rgb("▀", Some(t), Some(b), "")),
                    (Some(t), None) => line.push_str(&style::rgb("▀", Some(t), None, "")),
                    (None, Some(b)) => line.push_str(&style::rgb("▄", Some(b), None, "")),
                }
            }
            line
        })
        .collect()
}

/// Mean map brightness (0..1) around view point (x, y), over a box
/// `size` map pixels wide, counting only pixels on the disk.
fn albedo(x: f32, y: f32, size: usize) -> f32 {
    let n = MAP_N as f32;
    let h = size as i64 / 2;
    let mx = ((x + 1.0) / 2.0 * n) as i64 - h;
    let my = ((1.0 - y) / 2.0 * n) as i64 - h;
    let (mut sum, mut cnt) = (0u32, 0u32);
    for j in my..my + size as i64 {
        for i in mx..mx + size as i64 {
            if i < 0 || j < 0 || i >= MAP_N as i64 || j >= MAP_N as i64 { continue; }
            let dx = (i as f32 + 0.5) / n * 2.0 - 1.0;
            let dy = 1.0 - (j as f32 + 0.5) / n * 2.0;
            if dx * dx + dy * dy > 1.0 { continue; }
            sum += MAP[j as usize * MAP_N + i as usize] as u32;
            cnt += 1;
        }
    }
    if cnt == 0 { 0.6 } else { (sum as f32 / cnt as f32 / 255.0 / WHITE).min(1.0) }
}

// ── Map ────────────────────────────────────────────────────────────────

/// Paint the braille map for `view`.
fn render_map(view: View) {
    let (cols, rows) = Crust::terminal_size();
    let (cols, rows) = (cols as usize, rows as usize);
    let h = rows.saturating_sub(1).max(1);
    let lat = view.cy.clamp(-1.0, 1.0).asin();
    let lon = (view.cx / lat.cos().max(1e-6)).clamp(-1.0, 1.0).asin();
    let (lat, lon) = (lat.to_degrees().round(), lon.to_degrees().round());
    let zoom = view.zoom();
    header(cols, vec![
        "Map".to_string(),
        if zoom.fract() == 0.0 { format!("zoom {zoom}×") } else { format!("zoom {zoom:.1}×") },
        format!("{}°{} {}°{}", lat.abs(), if lat < 0.0 { "S" } else { "N" }, lon.abs(), if lon < 0.0 { "W" } else { "E" }),
    ], "+ - zoom   ←↑↓→ pan   0 reset   m moon   q quit");
    let mut main = Pane::new(1, 2, cols as u16, h as u16, 255, 16);
    main.wrap = false;
    main.scroll = false;
    main.set_text(&draw_map(cols, h, view, features()).join("\n"));
    main.refresh();
}

/// The map in `width` × `rows` braille cells. Each cell holds 2×4
/// sub-pixels, and a sub-pixel is square, so the disk stays round.
fn draw_map(width: usize, rows: usize, view: View, names: &[Feature]) -> Vec<String> {
    let lv = levels();
    let (w, h) = (width * 2, rows * 4);
    let d = w.min(h) as f32 * 0.96 * view.zoom();
    let k = 2.0 / d;
    let px_per_sub = BIG_N as f32 / d;
    let mut cells: Vec<Vec<String>> = Vec::with_capacity(rows);
    for row in 0..rows {
        let mut line = Vec::with_capacity(width);
        for col in 0..width {
            let mut v = [0f32; 8];
            let mut on = 0;
            for dy in 0..4 {
                for dx in 0..2 {
                    let x = view.cx + ((col * 2 + dx) as f32 + 0.5 - w as f32 / 2.0) * k;
                    let y = view.cy - ((row * 4 + dy) as f32 + 0.5 - h as f32 / 2.0) * k;
                    if x * x + y * y <= 1.0 {
                        v[dy * 2 + dx] = sample(lv, x, y, px_per_sub);
                        on += 1;
                    }
                }
            }
            line.push(if on == 0 { " ".to_string() } else { braille_cell(&v) });
        }
        cells.push(line);
    }
    label(&mut cells, view, d, names);
    cells.into_iter().map(|l| l.concat()).collect()
}

/// One braille cell from its eight sub-pixels, row by row, left then
/// right. The cell's background carries its mean tone. Dots mark the
/// sub-pixels clearly brighter than that mean, drawn brighter still, so a
/// crater rim or a bright edge shows at sub-cell detail and flat ground
/// stays clean.
fn braille_cell(v: &[f32; 8]) -> String {
    const BIT: [u32; 8] = [0x01, 0x08, 0x02, 0x10, 0x04, 0x20, 0x40, 0x80];
    let t = v.map(|x| ((x - DARK) / (1.0 - DARK)).clamp(0.0, 1.0));
    let mean = t.iter().sum::<f32>() / 8.0;
    let (mut bits, mut lit) = (0u32, 0f32);
    for (i, &x) in t.iter().enumerate() {
        if x > mean + EDGE { bits |= BIT[i]; lit += x; }
    }
    let gray = |c: f32| { let c = c.clamp(0.0, 255.0).round() as u8; (c, c, c) };
    let bg = gray(mean * 230.0);
    if bits == 0 { return style::rgb(" ", None, Some(bg), ""); }
    let fg = gray(lit / bits.count_ones() as f32 * 230.0 + 60.0);
    let glyph = char::from_u32(0x2800 + bits).unwrap_or(' ').to_string();
    style::rgb(&glyph, Some(fg), Some(bg), "")
}

/// How much a feature's size counts toward getting its name. Rilles,
/// ridges and valleys are long and thin, so their size overstates them.
fn weight(kind: u8) -> f32 {
    if kind == b'o' { 0.35 } else { 1.0 }
}

/// Write feature names over the map, biggest first, each where it has
/// room. A feature gets its name once it is a few cells across.
fn label(cells: &mut [Vec<String>], view: View, d: f32, names: &[Feature]) {
    let rows = cells.len();
    let width = cells.first().map_or(0, |l| l.len());
    let mut taken = vec![vec![false; width]; rows];
    for f in names {
        let (la, lo) = (f.lat.to_radians(), f.lon.to_radians());
        if la.cos() * lo.cos() < 0.1 { continue; }
        let size = f.km / MOON_KM * d;
        if size * weight(f.kind) < 11.0 { continue; }
        let sx = width as f32 + (la.cos() * lo.sin() - view.cx) * d / 2.0;
        let sy = rows as f32 * 2.0 - (la.sin() - view.cy) * d / 2.0;
        let n = f.name.chars().count();
        // Plains are named in their middle; craters and the rest just
        // below the rim, so the name does not cover what it names.
        let below = if f.kind == b'p' { 0 } else { (size / 8.0).ceil() as i64 };
        let row = (sy / 4.0).floor() as i64 + below;
        let col = (sx / 2.0).floor() as i64 - n as i64 / 2;
        if row < 0 || row >= rows as i64 || col < 0 || col as usize + n > width { continue; }
        // The name itself stays on the disk.
        let ly = view.cy - ((row as f32 + 0.5) * 4.0 - rows as f32 * 2.0) * 2.0 / d;
        let lx = la.cos() * lo.sin();
        if lx * lx + ly * ly > 1.0 { continue; }
        let (row, col) = (row as usize, col as usize);
        let (from, to) = (col.saturating_sub(1), (col + n + 1).min(width));
        if taken[row][from..to].iter().any(|&t| t) { continue; }
        taken[row][from..to].iter_mut().for_each(|t| *t = true);
        let color = match f.kind { b'p' => 153, b'c' => 222, _ => 180 };
        cells[row][col] = style::styled(&f.name, Some(color), Some(16), "");
        for c in col + 1..col + n { cells[row][c] = String::new(); }
    }
}

/// Map brightness (0..1) at disk point (x, y), read from the halving whose
/// pixels come closest to one screen sub-pixel.
fn sample(levels: &[Vec<u8>], x: f32, y: f32, px_per_sub: f32) -> f32 {
    let lvl = (px_per_sub.max(1.0).log2().floor() as usize).min(levels.len() - 1);
    let n = BIG_N >> lvl;
    let map = &levels[lvl];
    let u = ((x + 1.0) / 2.0 * n as f32 - 0.5).clamp(0.0, (n - 1) as f32);
    let v = ((1.0 - y) / 2.0 * n as f32 - 0.5).clamp(0.0, (n - 1) as f32);
    let (x0, y0) = (u as usize, v as usize);
    let (x1, y1) = ((x0 + 1).min(n - 1), (y0 + 1).min(n - 1));
    let (fx, fy) = (u - x0 as f32, v - y0 as f32);
    let at = |i: usize, j: usize| map[j * n + i] as f32;
    let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
    let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
    ((top * (1.0 - fy) + bottom * fy) / 255.0 / MAP_WHITE).min(1.0)
}

/// The big map and its halvings down to 128 pixels, built on first use.
fn levels() -> &'static Vec<Vec<u8>> {
    static LEVELS: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    LEVELS.get_or_init(|| {
        let mut px = miniz_oxide::inflate::decompress_to_vec_zlib(MAP_BIG).unwrap_or_default();
        if px.len() != BIG_N * BIG_N { px = vec![0; BIG_N * BIG_N]; }
        for i in BIG_N..px.len() { px[i] = px[i].wrapping_add(px[i - BIG_N]); }
        let mut out = vec![px];
        let mut n = BIG_N;
        while n > 128 {
            let (prev, h) = (out.last().unwrap(), n / 2);
            let mut next = vec![0u8; h * h];
            for y in 0..h {
                for x in 0..h {
                    let at = |i: usize, j: usize| prev[j * n + i] as u16;
                    let sum = at(2 * x, 2 * y) + at(2 * x + 1, 2 * y) + at(2 * x, 2 * y + 1) + at(2 * x + 1, 2 * y + 1);
                    next[y * h + x] = ((sum + 2) / 4) as u8;
                }
            }
            out.push(next);
            n = h;
        }
        out
    })
}

fn features() -> &'static Vec<Feature> {
    static LIST: OnceLock<Vec<Feature>> = OnceLock::new();
    LIST.get_or_init(|| {
        let mut list: Vec<Feature> = FEATURES.lines().filter_map(|l| {
            let mut p = l.split('\t');
            Some(Feature {
                name: p.next()?.to_string(),
                kind: p.next()?.bytes().next()?,
                lat: p.next()?.parse().ok()?,
                lon: p.next()?.parse().ok()?,
                km: p.next()?.parse().ok()?,
            })
        }).collect();
        list.sort_by(|a, b| (b.km * weight(b.kind)).total_cmp(&(a.km * weight(a.kind))));
        list
    })
}

// ── Phase ──────────────────────────────────────────────────────────────

/// Where in the cycle the Moon is at `hours` on `day`: 0 new, 0.5 full.
fn phase_at(day: i64, hours: f64) -> f64 {
    let (y, m, d) = civil_from_days(day);
    (orbit::moon_phase(y, m, d).phase + hours / 24.0 / SYNODIC).rem_euclid(1.0)
}

fn lit_fraction(f: f64) -> f64 {
    (1.0 - (f * 2.0 * PI).cos()) / 2.0
}

/// The four named phases hold for half a day either side; the rest of
/// the cycle is crescent or gibbous.
fn phase_name(f: f64) -> &'static str {
    let near = |c: f64| { let d = (f - c).abs(); d < 0.017 || d > 0.983 };
    if near(0.0) { "New moon" }
    else if near(0.25) { "First quarter" }
    else if near(0.5) { "Full moon" }
    else if near(0.75) { "Last quarter" }
    else if f < 0.25 { "Waxing crescent" }
    else if f < 0.5 { "Waxing gibbous" }
    else if f < 0.75 { "Waning gibbous" }
    else { "Waning crescent" }
}

/// "full in 11 d", or "full today" within half a day of it.
fn until(f: f64, target: f64, what: &str) -> String {
    let d = (target - f).rem_euclid(1.0) * SYNODIC;
    if d < 0.5 || d > SYNODIC - 0.5 { format!("{what} today") } else { format!("{what} in {d:.0} d") }
}

// ── Dates ──────────────────────────────────────────────────────────────

/// Local date as days since 1970-01-01, and the local hour with minutes.
fn now_local() -> (i64, f64) {
    let t = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    let day = days_from_civil(tm.tm_year + 1900, tm.tm_mon as u32 + 1, tm.tm_mday as u32);
    (day, tm.tm_hour as f64 + tm.tm_min as f64 / 60.0)
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    ((if m <= 2 { y + 1 } else { y }) as i32, m, d)
}

/// 0 = Sunday. 1970-01-01 was a Thursday.
fn weekday(days: i64) -> usize {
    (days + 4).rem_euclid(7) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visible(s: &str) -> usize {
        let mut n = 0;
        let mut in_esc = false;
        for c in s.chars() {
            if in_esc { if c == 'm' { in_esc = false; } }
            else if c == '\x1b' { in_esc = true; }
            else { n += 1; }
        }
        n
    }

    #[test]
    fn a_braille_cell_dots_its_bright_sub_pixels() {
        let mut v = [0.0f32; 8];
        v[0] = 1.0;
        assert!(braille_cell(&v).contains('\u{2801}'));
        let mut v = [0.0f32; 8];
        v[7] = 1.0;
        assert!(braille_cell(&v).contains('\u{2880}'));
        // Flat ground, dark or bright, is its tone alone with no dots.
        assert!(braille_cell(&[0.1; 8]).contains(' '));
        assert!(braille_cell(&[1.0; 8]).contains(' '));
    }

    #[test]
    fn the_map_fills_its_box_and_names_what_it_shows() {
        let names = features();
        assert!(names.len() > 1000);
        let cop = names.iter().find(|f| f.name == "Copernicus").unwrap();
        assert!((cop.lat - 9.6).abs() < 0.2 && (cop.lon + 20.1).abs() < 0.2);
        let lines = draw_map(120, 40, View::default(), names);
        assert_eq!(lines.len(), 40);
        assert!(lines.iter().all(|l| visible(l) == 120));
        assert!(lines.iter().any(|l| l.contains("Mare Imbrium")));
        // Zoomed in on Copernicus, it is named.
        let (la, lo) = (cop.lat.to_radians(), cop.lon.to_radians());
        let view = View { zi: 6, cx: la.cos() * lo.sin(), cy: la.sin() };
        assert!(draw_map(120, 40, view, names).iter().any(|l| l.contains("Copernicus")));
    }

    #[test]
    fn dates_round_trip_and_know_their_weekday() {
        let d = days_from_civil(2026, 9, 15);
        assert_eq!(civil_from_days(d), (2026, 9, 15));
        assert_eq!(WEEKDAYS[weekday(d)], "Tue");
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29) + 1), (2024, 3, 1));
        assert_eq!(civil_from_days(0), (1970, 1, 1));
    }

    #[test]
    fn phases_are_named_and_lit_as_expected() {
        assert_eq!(phase_name(0.5), "Full moon");
        assert_eq!(phase_name(0.99), "New moon");
        assert_eq!(phase_name(0.1), "Waxing crescent");
        assert_eq!(phase_name(0.6), "Waning gibbous");
        assert!((lit_fraction(0.25) - 0.5).abs() < 1e-9);
        assert!(lit_fraction(0.5) > 0.999);
        assert_eq!(until(0.503, 0.5, "full"), "full today");
        assert_eq!(until(0.129, 0.5, "full"), "full in 11 d");
    }

    #[test]
    fn the_drawing_fills_its_box_and_lights_the_right_side() {
        let lines = draw_moon(0.25, 40, 60, 20);
        assert_eq!(lines.len(), 20);
        assert!(lines.iter().all(|l| visible(l) == 60));
        // First quarter: the right half is lit, the left half is not.
        let row = &lines[10];
        let gray = |cell: &str| -> u32 {
            cell.split("38;2;").nth(1).and_then(|s| s.split(';').next()).and_then(|s| s.parse().ok()).unwrap_or(0)
        };
        let cells: Vec<&str> = row.split('▀').collect();
        let left = gray(cells[cells.len() / 2 - 12]);
        let right = gray(cells[cells.len() / 2 + 12]);
        assert!(right > left * 2, "right {right} should be well brighter than left {left}");
    }
}
