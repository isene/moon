# moon

The Moon as it looks tonight, in the terminal.

![moon](img/screenshot.png)

The big disk is the near side of the Moon, lit for the phase of the
moment: the sunlit part in light gray and white, the night side in dark
gray, craters and maria on both. A strip of phase symbols along the bottom shows the
three days before, the day on screen, and the days ahead until the edge
of the window. Today's label is yellow.

Part of the [Fe₂O₃ suite](https://isene.github.io/fe2o3/). Built on
[crust](https://github.com/isene/crust) (panes) and
[orbit](https://github.com/isene/orbit) (phase).

## Keys

| Key | Action |
|---|---|
| `←` `→` / `h` `l` | a day back / forward |
| `t` | back to today |
| `m` | the map (and back) |
| `q` | quit |

## The map

`m` opens a braille map of the near side with the features named: the
seas in blue, craters in yellow, mountains, valleys and rilles in tan.
Zoom in and more names appear, biggest first, wherever there is room.

| Key | Action |
|---|---|
| `+` `-` | zoom in / out, up to 16× |
| `←` `↑` `↓` `→` / `h` `j` `k` `l` | pan |
| `0` | back to the whole disk |
| `m` / `Esc` | back to the Moon |

Every braille cell holds eight dots in two columns of four, so the map
has four times the rows of the phase picture. The detail map is 2048
pixels across, shaded with the Moon's measured heights and lit from the
north-west so craters show their rims. It is compressed into the binary
and unpacked the first time the map opens.

The picture is drawn on start, on a key and on a resize. Nothing runs
in between.

## Install

```bash
git clone https://github.com/isene/moon
cd moon
cargo build --release
```

`crust` and `orbit` are expected as sibling checkouts (`../crust`,
`../orbit`). Release binaries for Linux and macOS are on the
[releases page](https://github.com/isene/moon/releases).

## How it is drawn

Every cell is a `▀` with one gray for its top half and one for its
bottom, so a 50-row window gives a 100-pixel moon. The map is NASA's
LRO camera mosaic of the near side, 512 pixels across, embedded in the
binary. The phase comes from orbit's mean lunar cycle, so it can be a
few hours off the true phase. The Moon's slight wobble (libration) is
not drawn; the map is always the mean near side, north up, with the lit
side on the right while waxing, as seen from the northern hemisphere.

## Credits

Moon map: NASA/GSFC/Arizona State University, Lunar Reconnaissance
Orbiter (LROC WAC), from the
[CGI Moon Kit](https://svs.gsfc.nasa.gov/4720). Public domain.

Heights for the map shading: NASA/GSFC, Lunar Orbiter Laser Altimeter
(LOLA), from the same kit. Public domain.

Feature names: the IAU / USGS
[Gazetteer of Planetary Nomenclature](https://planetarynames.wr.usgs.gov/),
near side only, without the lettered satellite craters. Public domain.

## License

Public domain (Unlicense).
