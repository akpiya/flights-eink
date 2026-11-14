use embedded_graphics::{
    mono_font::{MonoTextStyle, iso_8859_1::FONT_6X10},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Baseline, Text},
};

use crate::screen::{Color, Screen};

/// Airports on the maps as (latitude, longitude, code), found by `build.rs`.
pub const AIRPORTS: &[(f32, f32, &str)] = include!(concat!(env!("OUT_DIR"), "/airports.rs"));

/// The usual map sign for an airport: an airplane pointing north.
const ICON: [&[u8; 15]; 14] = [
    b".......#.......",
    b"......###......",
    b"......###......",
    b"......###......",
    b".....#####.....",
    b"...#########...",
    b".#############.",
    b"###############",
    b"......###......",
    b"......###......",
    b"......###......",
    b".....#####.....",
    b"....#######....",
    b"....###.###....",
];

/// Draws an airport's sign in white at its map position, with its code beside it.
pub fn draw(screen: &mut Screen, (x, y): (f32, f32), code: &str) {
    let left = x as i32 - ICON[0].len() as i32 / 2;
    let top = y as i32 - ICON.len() as i32 / 2;
    for (dy, row) in ICON.iter().enumerate() {
        for (dx, &pixel) in row.iter().enumerate() {
            let (Ok(row), Ok(column)) = (
                usize::try_from(top + dy as i32),
                usize::try_from(left + dx as i32),
            ) else {
                continue;
            };
            if pixel == b'#' && screen.in_bounds(row, column) {
                screen.set(row, column, Color::White);
            }
        }
    }

    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    let corner = Point::new(left + ICON[0].len() as i32 + 3, y as i32);
    Text::with_baseline(code, corner, style, Baseline::Middle)
        .draw(screen)
        .expect("drawing on the screen can't fail");
}
