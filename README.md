# epaper

## Color palette

The panel's 4-gray mode has four levels. Each layer gets its own levels:

| Level        | Used for                      |
|--------------|-------------------------------|
| White        | Background map: land          |
| Light gray   | Background map: parks, woods, water, rivers, airport, roads |
| Dark gray    | Not used by the map. Reserved for overlays |
| Black        | Not used by the map. Reserved for airplanes |

**Decision: the background map uses only white and light gray.** On this panel the two
grays come out darker and closer together than expected. A test with one stripe per
level confirmed the order white, light gray, dark gray, black, but the dark gray reads
as nearly black. An earlier version drew water and roads in dark gray, and the map
looked like it competed with the overlay. With the map kept to the two lightest levels,
black airplanes (and anything later drawn in dark gray) stand out clearly.

The tone for each map feature is set in the `AREAS`, `WATERWAYS` and `ROADS` tables
in `build.rs`.
