use crate::screen::{Screen, PANEL_BYTES};

const MAP_RECORD_BYTES: usize = PANEL_BYTES + 1;
static MAP_BUNDLE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/maps.bin"));

pub const MAP_LATITUDE: &str = env!("MAP_LATITUDE");
pub const MAP_LONGITUDE: &str = env!("MAP_LONGITUDE");

pub struct Map {
    map_selection: usize,
}

impl Map {
    pub const fn new() -> Self {
        Self { map_selection: 0 }
    }

    pub fn select_zoom(&mut self, zoom: u8) -> bool {
        if let Some(index) = map_records().position(|record| record[0] == zoom) {
            self.map_selection = index;
            true
        } else {
            false
        }
    }

    pub fn draw_background(&self, screen: &mut Screen) {
        let record = map_records()
            .nth(self.map_selection)
            .expect("map selection is valid");
        let image: &[u8; PANEL_BYTES] = record[1..]
            .try_into()
            .expect("map record contains one panel image");
        screen.load_panel_bytes(image);
    }
}

fn map_records() -> impl Iterator<Item = &'static [u8]> {
    MAP_BUNDLE.chunks_exact(MAP_RECORD_BYTES)
}
