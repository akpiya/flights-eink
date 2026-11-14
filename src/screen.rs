use core::convert::Infallible;

use embedded_graphics::{pixelcolor::BinaryColor, prelude::*};

pub const WIDTH: usize = 960;
pub const HEIGHT: usize = 680;

pub const PIXELS: usize = WIDTH * HEIGHT;

const BITS_PER_BYTE: usize = 8;
pub const PANEL_BYTES: usize = PIXELS / BITS_PER_BYTE;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Color {
    Black,
    White,
}

pub struct Screen {
    buf: [u8; PANEL_BYTES],
}

impl Screen {
    pub const fn new() -> Self {
        Self { buf: [0x00; PANEL_BYTES] }
    }

    pub fn clear(&mut self) {
        self.buf.fill(0x00);
    }

    pub fn in_bounds(&self, i: usize, j: usize) -> bool {
        i < HEIGHT && j < WIDTH
    }

    pub fn get(&self, i: usize, j: usize) -> Color {
        debug_assert!(self.in_bounds(i, j));

        let screen_pos = WIDTH * i + j;
        let buf_byte_pos = screen_pos / BITS_PER_BYTE;
        let buf_offset_pos = screen_pos % BITS_PER_BYTE;

        let mask = 0b1000_0000 >> buf_offset_pos;
        if self.buf[buf_byte_pos] & mask != 0 {
            Color::White
        } else {
            Color::Black
        }
    }

    pub fn set(&mut self, i: usize, j: usize, color: Color) {
        debug_assert!(self.in_bounds(i, j));

        let screen_pos = WIDTH * i + j;
        let buf_byte_pos = screen_pos / BITS_PER_BYTE;
        let buf_offset_pos = screen_pos % BITS_PER_BYTE;

        let mask = 0b1000_0000 >> buf_offset_pos;
        match color {
            Color::White => self.buf[buf_byte_pos] |= mask,
            Color::Black => self.buf[buf_byte_pos] &= !mask,
        }
    }

   pub(crate) fn panel_bytes(&self) -> &[u8] {
        &self.buf
    }
}

/// Lets embedded-graphics draw text onto the screen; off-screen pixels are dropped.
impl DrawTarget for Screen {
    type Color = BinaryColor;
    type Error = Infallible;

    fn draw_iter<I: IntoIterator<Item = Pixel<BinaryColor>>>(&mut self, pixels: I) -> Result<(), Infallible> {
        for Pixel(point, color) in pixels {
            let (Ok(row), Ok(column)) = (usize::try_from(point.y), usize::try_from(point.x)) else {
                continue;
            };
            if self.in_bounds(row, column) {
                self.set(row, column, if color.is_on() { Color::White } else { Color::Black });
            }
        }
        Ok(())
    }
}

impl OriginDimensions for Screen {
    fn size(&self) -> Size {
        Size::new(WIDTH as u32, HEIGHT as u32)
    }
}
