use core::fmt::Write;

use embedded_graphics::{
    mono_font::{MonoTextStyle, iso_8859_1::FONT_6X10},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};
use heapless::String;
use libm::{cosf, sinf};

use crate::{
    flight::Flight,
    screen::{Color, Screen},
};

/// An arrowhead pointing along the plane's track, as (forward, right) pixel offsets
/// from its position: nose, right wing tip, tail notch, left wing tip.
const ARROW: [(f32, f32); 4] = [(9.0, 0.0), (-7.0, 6.0), (-3.0, 0.0), (-7.0, -6.0)];
/// Planes that haven't reported a track get a dot instead.
const DOT_RADIUS: f32 = 4.0;
/// Farther than any part of either shape reaches from the plane's position.
const REACH: f32 = 10.0;

/// Draws a flight in white at its map position: an arrow along its track with its
/// flight number, altitude, track and ground speed beside it.
pub fn draw(screen: &mut Screen, (x, y): (f32, f32), flight: &Flight) {
    draw_icon(screen, (x, y), flight.track);

    let mut label = String::<40>::new();
    let name = flight.flight.as_deref().unwrap_or("").trim_end();
    write!(label, "{name}").expect("label fits");
    label.push('\n').expect("label fits");
    if let Some(feet) = flight.alt_baro {
        write!(label, "{feet}ft ").expect("label fits");
    }
    if let Some(track) = flight.track {
        write!(label, "{track:.0}\u{b0} ").expect("label fits");
    }
    if let Some(knots) = flight.gs {
        write!(label, "{knots:.0}kt").expect("label fits");
    }
    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let corner = Point::new(x as i32 + REACH as i32 + 2, y as i32 - 10);
    Text::with_baseline(&label, corner, style, Baseline::Top)
        .draw(screen)
        .expect("drawing on the screen can't fail");
}

/// `track` is degrees clockwise from north.
fn draw_icon(screen: &mut Screen, (x, y): (f32, f32), track: Option<f32>) {
    let Some(track) = track else {
        fill(screen, (x, y), |px, py| {
            (px - x) * (px - x) + (py - y) * (py - y) <= DOT_RADIUS * DOT_RADIUS
        });
        return;
    };
    let (sin, cos) = (sinf(track.to_radians()), cosf(track.to_radians()));
    // Screen y grows downward, so north is -y.
    let [nose, right_wing, tail, left_wing] =
        ARROW.map(|(forward, right)| (x + forward * sin + right * cos, y - forward * cos + right * sin));
    fill(screen, (x, y), |px, py| {
        in_triangle((px, py), [nose, right_wing, tail]) || in_triangle((px, py), [nose, tail, left_wing])
    });
}

/// Whitens the on-screen pixels near `center` whose centers are `inside` the shape.
fn fill(screen: &mut Screen, (x, y): (f32, f32), inside: impl Fn(f32, f32) -> bool) {
    for row in (y - REACH) as i32..=(y + REACH) as i32 {
        for column in (x - REACH) as i32..=(x + REACH) as i32 {
            let (Ok(row), Ok(column)) = (usize::try_from(row), usize::try_from(column)) else {
                continue;
            };
            if screen.in_bounds(row, column) && inside(column as f32 + 0.5, row as f32 + 0.5) {
                screen.set(row, column, Color::White);
            }
        }
    }
}

fn in_triangle(point: (f32, f32), [a, b, c]: [(f32, f32); 3]) -> bool {
    let side = |p: (f32, f32), q: (f32, f32)| (q.0 - p.0) * (point.1 - p.1) - (q.1 - p.1) * (point.0 - p.0);
    let sides = [side(a, b), side(b, c), side(c, a)];
    sides.iter().all(|&s| s >= 0.0) || sides.iter().all(|&s| s <= 0.0)
}
