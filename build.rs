use std::{env, f64::consts::PI, fs, path::PathBuf};

use mvt_reader::{Reader, feature::Value};
use tiny_skia::{LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

const WIDTH: usize = 960;
const HEIGHT: usize = 680;
const PANEL_BYTES: usize = (WIDTH * HEIGHT) / 8;

const TILEJSON_URL: &str = "https://tiles.openfreemap.org/planet";
const USER_AGENT: &str = "epaper-map-builder/0.1";
const TILE_SIZE: i64 = 256;

const DEFAULT_ZOOMS: &str = "10";
const DEFAULT_LATITUDE: &str = "30.2672";
const DEFAULT_LONGITUDE: &str = "-97.7431";

/// Line width in pixels by feature class; unlisted classes aren't drawn. The device
/// shows lines light gray on black.
const WATERWAYS: &[(&str, f32)] = &[("river", 2.0), ("canal", 1.0)];
const ROADS: &[(&str, f32)] = &[
    ("motorway", 3.0),
    ("trunk", 3.0),
    ("primary", 3.0),
    ("secondary", 2.0),
    ("tertiary", 2.0),
    ("minor", 1.0),
    ("rail", 1.0),
];

fn main() {
    println!("cargo:rustc-link-arg=-Tdefmt.x");
    println!("cargo:rustc-link-arg=-Tlinkall.x");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=MAP_ZOOMS");
    println!("cargo:rerun-if-env-changed=MAP_LATITUDE");
    println!("cargo:rerun-if-env-changed=MAP_LONGITUDE");

    let zooms = parse_zooms(&env::var("MAP_ZOOMS").unwrap_or_else(|_| DEFAULT_ZOOMS.into()));
    let latitude = parse_coordinate(
        &env::var("MAP_LATITUDE").unwrap_or_else(|_| DEFAULT_LATITUDE.into()),
        -85.0,
        85.0,
    );
    let longitude = parse_coordinate(
        &env::var("MAP_LONGITUDE").unwrap_or_else(|_| DEFAULT_LONGITUDE.into()),
        -180.0,
        180.0,
    );
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));

    let tile_url_pattern = fetch_tile_url_pattern();
    // Each record: zoom, the map's top-left world pixel as two little-endian i32s
    // (the device needs it to place planes), then the packed image.
    let mut maps = Vec::with_capacity(zooms.len() * (9 + PANEL_BYTES));
    let mut airports: Vec<Airport> = Vec::new();
    for zoom in zooms {
        let (left, top) = top_left(zoom, latitude.into(), longitude.into());
        let tiles = fetch_tiles(&tile_url_pattern, zoom, left, top);
        let map = render_map(&tiles);
        for airport in find_airports(&tiles, zoom, left, top) {
            if !airports.iter().any(|known| known.code == airport.code) {
                airports.push(airport);
            }
        }
        maps.push(zoom);
        for world_pixel in [left, top] {
            let world_pixel = i32::try_from(world_pixel).expect("world pixels fit in i32 up to zoom 14");
            maps.extend(world_pixel.to_le_bytes());
        }
        maps.extend(pack(&map));
    }

    fs::write(out_dir.join("maps.bin"), maps).expect("write map data bundle");

    // Included by src/airport.rs as a Rust slice expression.
    let airports: String = airports
        .iter()
        .map(|airport| format!("({:?}, {:?}, {:?}),\n", airport.latitude, airport.longitude, airport.code))
        .collect();
    fs::write(out_dir.join("airports.rs"), format!("&[\n{airports}]\n")).expect("write airport list");
}

struct Airport {
    latitude: f32,
    longitude: f32,
    code: String,
}

fn parse_coordinate(value: &str, min: f32, max: f32) -> f32 {
    let coordinate: f32 = value
        .parse()
        .unwrap_or_else(|_| panic!("coordinate {value:?} must be a number"));
    assert!(
        coordinate.is_finite() && (min..=max).contains(&coordinate),
        "coordinate {value} must be between {min} and {max}"
    );
    coordinate
}

fn parse_zooms(value: &str) -> Vec<u8> {
    let mut zooms: Vec<u8> = value
        .split(',')
        .map(|part| {
            part.trim()
                .parse()
                .expect("MAP_ZOOMS must be comma-separated integers")
        })
        .collect();
    assert!(
        !zooms.is_empty(),
        "MAP_ZOOMS must include at least one zoom level"
    );
    assert!(
        zooms.iter().all(|zoom| *zoom <= 14),
        "OpenFreeMap zoom levels must be from 0 to 14"
    );
    zooms.sort();
    assert!(
        zooms.windows(2).all(|pair| pair[0] != pair[1]),
        "MAP_ZOOMS cannot contain duplicates"
    );
    zooms
}

fn get(url: &str) -> Vec<u8> {
    ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .and_then(|mut response| response.body_mut().read_to_vec())
        .unwrap_or_else(|error| panic!("failed to download {url}: {error}"))
}

/// Fetches OpenFreeMap's current tile URL pattern (`.../{z}/{x}/{y}.pbf`) from its
/// TileJSON. The URL contains a data version that changes with each OpenFreeMap update.
fn fetch_tile_url_pattern() -> String {
    let tilejson = String::from_utf8(get(TILEJSON_URL)).expect("TileJSON is UTF-8");
    let start = tilejson
        .find("https://")
        .expect("TileJSON contains a tile URL");
    let end = start + tilejson[start..].find('"').expect("tile URL is a JSON string");
    let pattern = &tilejson[start..end];
    assert!(
        pattern.contains("{z}/{x}/{y}"),
        "unexpected tile URL in TileJSON: {pattern}"
    );
    pattern.to_owned()
}

/// Converts a coordinate to its "world pixel": its position in an imaginary image of
/// the whole Earth at `zoom`, which is `TILE_SIZE * 2^zoom` pixels on each side
/// (2,097,152 at zoom 13) and is cut into `TILE_SIZE` tiles. x runs east from
/// longitude -180; y runs south from the top, using Web Mercator, which stretches
/// latitudes away from the equator. World pixels relate everything: dividing by
/// `TILE_SIZE` gives a tile's column and row, and subtracting the map's top-left
/// world pixel gives a position on the map.
fn world_pixel(latitude: f64, longitude: f64, zoom: u8) -> (f64, f64) {
    let world_size = TILE_SIZE as f64 * f64::from(1u32 << zoom);
    let x = (longitude + 180.0) / 360.0 * world_size;
    let y = (1.0 - latitude.to_radians().tan().asinh() / PI) / 2.0 * world_size;
    (x, y)
}

/// The world pixel of the top-left corner of a WIDTH x HEIGHT map centered on the
/// coordinate.
fn top_left(zoom: u8, latitude: f64, longitude: f64) -> (i64, i64) {
    let (center_x, center_y) = world_pixel(latitude, longitude, zoom);
    let left = (center_x - WIDTH as f64 / 2.0).round() as i64;
    let top = (center_y - HEIGHT as f64 / 2.0).round() as i64;
    (left, top)
}

/// Downloads every vector tile overlapping the WIDTH x HEIGHT map whose top-left
/// corner is world pixel (`left`, `top`), each with where its corner falls on the map.
fn fetch_tiles(tile_url_pattern: &str, zoom: u8, left: i64, top: i64) -> Vec<(Reader, (f32, f32))> {
    let tiles_per_side = 1i64 << zoom;

    let mut tiles = Vec::new();
    for tile_y in top.div_euclid(TILE_SIZE)..=(top + HEIGHT as i64 - 1).div_euclid(TILE_SIZE) {
        assert!(
            (0..tiles_per_side).contains(&tile_y),
            "map at zoom {zoom} extends past the top or bottom of the world"
        );
        for tile_x in left.div_euclid(TILE_SIZE)..=(left + WIDTH as i64 - 1).div_euclid(TILE_SIZE) {
            let wrapped_x = tile_x.rem_euclid(tiles_per_side);
            let url = tile_url_pattern
                .replace("{z}", &zoom.to_string())
                .replace("{x}", &wrapped_x.to_string())
                .replace("{y}", &tile_y.to_string());
            let reader = Reader::new(get(&url))
                .unwrap_or_else(|error| panic!("failed to parse tile {url}: {error:?}"));
            let origin = (
                (tile_x * TILE_SIZE - left) as f32,
                (tile_y * TILE_SIZE - top) as f32,
            );
            tiles.push((reader, origin));
        }
    }
    tiles
}

/// Draws the tiles' lines, returning whether each map pixel is part of one.
fn render_map(tiles: &[(Reader, (f32, f32))]) -> Vec<bool> {
    let mut pixmap = Pixmap::new(WIDTH as u32, HEIGHT as u32).expect("map size is valid");
    pixmap.fill(tiny_skia::Color::BLACK);

    for (layer, widths) in [("waterway", WATERWAYS), ("transportation", ROADS)] {
        for (reader, origin) in tiles {
            for (feature, scale) in features(reader, layer) {
                let Some(&(_, width)) = widths.iter().find(|(c, _)| *c == class(&feature))
                else {
                    continue;
                };
                if property(&feature, "brunnel") == Some("tunnel") {
                    continue;
                }
                stroke_lines(&mut pixmap, &feature.geometry, *origin, scale, width);
            }
        }
    }

    // Anti-aliasing is off, so every pixel is pure black or pure white.
    pixmap.pixels().iter().map(|pixel| pixel.red() == 255).collect()
}

/// The airports on the map with an IATA code, which in practice means passenger
/// airports. (OpenFreeMap classes everything but international airports as
/// "other", down to private strips and heliports.)
fn find_airports(tiles: &[(Reader, (f32, f32))], zoom: u8, left: i64, top: i64) -> Vec<Airport> {
    let world_size = TILE_SIZE as f64 * f64::from(1u32 << zoom);
    let mut airports = Vec::new();
    for (reader, (origin_x, origin_y)) in tiles {
        for (feature, scale) in features(reader, "aerodrome_label") {
            let Some(code) = property(&feature, "iata") else {
                continue;
            };
            let geo_types::Geometry::MultiPoint(points) = &feature.geometry else {
                continue;
            };
            let Some(point) = points.0.first() else {
                continue;
            };
            let (x, y) = (origin_x + point.x() * scale, origin_y + point.y() * scale);
            if !(0.0..WIDTH as f32).contains(&x) || !(0.0..HEIGHT as f32).contains(&y) {
                continue;
            }
            // Back from world pixel to coordinate: the inverse of `world_pixel`.
            let world_x = left as f64 + f64::from(x);
            let world_y = top as f64 + f64::from(y);
            let longitude = world_x / world_size * 360.0 - 180.0;
            let latitude = (PI * (1.0 - 2.0 * world_y / world_size)).sinh().atan().to_degrees();
            airports.push(Airport {
                latitude: latitude as f32,
                longitude: longitude as f32,
                code: code.to_owned(),
            });
        }
    }
    airports
}

/// The features of one layer, each with the scale from tile units to pixels.
fn features(reader: &Reader, layer: &str) -> Vec<(mvt_reader::feature::Feature, f32)> {
    let layers = reader.get_layer_metadata().expect("tile has layer metadata");
    let Some(meta) = layers.iter().find(|meta| meta.name == layer) else {
        return Vec::new();
    };
    let scale = TILE_SIZE as f32 / meta.extent as f32;
    reader
        .get_features(meta.layer_index)
        .expect("layer features decode")
        .into_iter()
        .map(|feature| (feature, scale))
        .collect()
}

fn property<'a>(feature: &'a mvt_reader::feature::Feature, key: &str) -> Option<&'a str> {
    match feature.properties.as_ref()?.get(key)? {
        Value::String(value) => Some(value),
        _ => None,
    }
}

fn class(feature: &mvt_reader::feature::Feature) -> &str {
    property(feature, "class").unwrap_or("")
}

fn white() -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(tiny_skia::Color::WHITE);
    // Pure black and white only: anti-aliasing would create grays.
    paint.anti_alias = false;
    paint
}

fn stroke_lines(
    pixmap: &mut Pixmap,
    geometry: &geo_types::Geometry<f32>,
    origin: (f32, f32),
    scale: f32,
    width: f32,
) {
    let lines = match geometry {
        geo_types::Geometry::LineString(line) => vec![line.clone()],
        geo_types::Geometry::MultiLineString(multi) => multi.0.clone(),
        _ => return,
    };
    let stroke = Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    for line in lines {
        let mut path = PathBuilder::new();
        add_points(&mut path, line.0.iter(), origin, scale);
        if let Some(path) = path.finish() {
            pixmap.stroke_path(&path, &white(), &stroke, Transform::identity(), None);
        }
    }
}

/// Adds tile-unit points to the path in map pixels. (Converting the points rather
/// than using a `Transform` keeps stroke widths in pixels.)
fn add_points<'a>(
    path: &mut PathBuilder,
    points: impl Iterator<Item = &'a geo_types::Coord<f32>>,
    (origin_x, origin_y): (f32, f32),
    scale: f32,
) {
    for (index, point) in points.enumerate() {
        let (x, y) = (origin_x + point.x * scale, origin_y + point.y * scale);
        if index == 0 {
            path.move_to(x, y);
        } else {
            path.line_to(x, y);
        }
    }
}

/// Packs the map 8 pixels per byte, MSB first, 1 for a line: the panel's RAM layout.
fn pack(map: &[bool]) -> Vec<u8> {
    map.chunks_exact(8)
        .map(|pixels| pixels.iter().fold(0, |byte, &white| (byte << 1) | u8::from(white)))
        .collect()
}
