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
        Self { buf: [0xFF; PANEL_BYTES] }
    }

    pub(crate) fn load_panel_bytes(&mut self, bytes: &[u8; PANEL_BYTES]) {
        self.buf.copy_from_slice(bytes);
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
