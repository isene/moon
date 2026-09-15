//! moon — the Moon as it looks tonight.
//!
//! The near side, lit for the phase of the moment, drawn with half-block
//! cells so every cell holds two pixels. A strip along the bottom shows
//! the days around the one on screen. Nothing runs between key presses.

use std::f64::consts::PI;

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

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("moon — The Moon as it looks tonight (Fe2O3 suite)");
        println!();
        println!("Usage: moon");
        println!();
        println!("Keys: ← → / h l  day back / forward    t  today    q  quit");
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
    render(offset);
    loop {
        let Some(key) = Input::getchr(None) else { continue };
        match key.as_str() {
            "q" | "Q" => break,
            "h" | "LEFT" => offset -= 1,
            "l" | "RIGHT" => offset += 1,
            "t" => offset = 0,
            "RESIZE" => {}
            _ => continue,
        }
        render(offset);
    }
    Crust::cleanup();
}

/// Paint the whole screen for the day `offset` days from today.
fn render(offset: i64) {
    let (cols, rows) = Crust::terminal_size();
    let (cols, rows) = (cols as usize, rows as usize);
    let (today, hours) = now_local();
    let day = today + offset;
    let strip_h = MINI / 2 + 2;
    let main_h = rows.saturating_sub(1 + strip_h).max(1);

    let f = phase_at(day, hours);
    let (y, m, d) = civil_from_days(day);
    let left = format!(
        " {} {} {} {}   {}   {}% lit   {:.1} days old   {}   {}",
        WEEKDAYS[weekday(day)], d, MONTHS[(m - 1) as usize], y, phase_name(f),
        (lit_fraction(f) * 100.0).round(), f * SYNODIC, until(f, 0.5, "full"), until(f, 0.0, "new")
    );
    let right = "← → day   t today   q quit ";
    let pad = cols.saturating_sub(left.chars().count() + right.chars().count());
    let mut header = Pane::new(1, 1, cols as u16, 1, 255, 236);
    header.wrap = false;
    header.scroll = false;
    header.set_text(&format!("{left}{}{right}", " ".repeat(pad)));
    header.refresh();

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
