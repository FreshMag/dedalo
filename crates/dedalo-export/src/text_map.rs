//! The text map: the view drawn with characters (spec 9.6).
//!
//! It is cheap for agents to read and diffable in tests. One character is
//! one cell. A wall takes a character of its own between the cells it
//! separates; lattice lines that carry no wall anywhere are left out, so a
//! lone 8 by 6 room is 10 characters wide and 8 tall.

use dedalo_geom::{Loc, Pt, locate};
use dedalo_render::View;

/// Largest map a text map draws, in cells per side.
const MAX_CELLS: usize = 1000;
const WALL: char = '#';
const FLOOR: char = '.';

fn opening_char(ty: &str, secret: bool, implicit: bool) -> char {
    match ty {
        _ if implicit => FLOOR,
        "door" if secret => 'S',
        "door" => '+',
        "archway" => ':',
        "stairs" => '>',
        "ladder" => '=',
        "trapdoor" => 'o',
        _ => FLOOR,
    }
}

fn opening_label(c: char) -> &'static str {
    match c {
        '+' => "door",
        'S' => "secret door",
        ':' => "archway",
        '>' => "stairs",
        '=' => "ladder",
        'o' => "trapdoor",
        _ => "",
    }
}

/// Draws a view as text. Returns an empty string for an empty view.
pub fn text_map(view: &View) -> String {
    let (x0, y0) = (
        view.bounds.x0.div_euclid(1000),
        view.bounds.y0.div_euclid(1000),
    );
    let (x1, y1) = (
        -(-view.bounds.x1).div_euclid(1000),
        -(-view.bounds.y1).div_euclid(1000),
    );
    let (w, h) = ((x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
    if w == 0 || h == 0 || view.spaces.is_empty() {
        return String::new();
    }
    if w > MAX_CELLS || h > MAX_CELLS {
        return format!(
            "(the map is {w} by {h} cells; a text map is limited to {MAX_CELLS} by {MAX_CELLS})\n"
        );
    }

    // Which space or stub passage owns each cell, judged at the cell's centre.
    let stubs: Vec<&[Pt]> = view
        .openings
        .iter()
        .filter_map(|o| o.stub.as_deref())
        .collect();
    let owner_of = |col: usize, row: usize| -> Option<usize> {
        let p = Pt::new(
            (x0 + col as i64) * 1000 + 500,
            (y0 + row as i64) * 1000 + 500,
        );
        let in_space = view.spaces.iter().position(|s| match s.circle {
            Some(c) => {
                let (dx, dy) = ((p.x - c.center.x) as i128, (p.y - c.center.y) as i128);
                dx * dx + dy * dy <= c.radius as i128 * c.radius as i128
            }
            None => locate(p, &s.polygon) != Loc::Outside,
        });
        in_space.or_else(|| {
            stubs
                .iter()
                .position(|s| locate(p, s) != Loc::Outside)
                .map(|i| view.spaces.len() + i)
        })
    };
    let owner: Vec<Vec<Option<usize>>> = (0..h)
        .map(|row| (0..w).map(|col| owner_of(col, row)).collect())
        .collect();
    let at = |col: i64, row: i64| -> Option<usize> {
        if col < 0 || row < 0 || col >= w as i64 || row >= h as i64 {
            None
        } else {
            owner[row as usize][col as usize]
        }
    };

    // A grid at double resolution: odd positions are cells, even positions
    // are the lattice lines between them.
    let (gw, gh) = (2 * w + 1, 2 * h + 1);
    let mut grid = vec![vec![' '; gw]; gh];
    let between = |a: Option<usize>, b: Option<usize>| {
        if a != b {
            WALL
        } else if a.is_some() {
            FLOOR
        } else {
            ' '
        }
    };
    for row in 0..h as i64 {
        for col in 0..w as i64 {
            if at(col, row).is_some() {
                grid[2 * row as usize + 1][2 * col as usize + 1] = FLOOR;
            }
        }
    }
    for row in 0..h as i64 {
        for col in 0..=w as i64 {
            grid[2 * row as usize + 1][2 * col as usize] = between(at(col - 1, row), at(col, row));
        }
    }
    for row in 0..=h as i64 {
        for col in 0..w as i64 {
            grid[2 * row as usize][2 * col as usize + 1] = between(at(col, row - 1), at(col, row));
        }
    }
    for row in 0..=h {
        for col in 0..=w {
            let (gx, gy) = (2 * col, 2 * row);
            let mut around = Vec::new();
            if gx > 0 {
                around.push(grid[gy][gx - 1]);
            }
            if gx + 1 < gw {
                around.push(grid[gy][gx + 1]);
            }
            if gy > 0 {
                around.push(grid[gy - 1][gx]);
            }
            if gy + 1 < gh {
                around.push(grid[gy + 1][gx]);
            }
            grid[gy][gx] = if around.contains(&WALL) {
                WALL
            } else if around.iter().all(|c| *c == FLOOR) {
                FLOOR
            } else {
                ' '
            };
        }
    }

    // Openings replace the wall characters they cut.
    let mut used: Vec<char> = Vec::new();
    let mut set = |grid: &mut Vec<Vec<char>>, gx: i64, gy: i64, c: char| {
        if gx >= 0 && gy >= 0 && (gx as usize) < gw && (gy as usize) < gh {
            grid[gy as usize][gx as usize] = c;
            if c != FLOOR && !used.contains(&c) {
                used.push(c);
            }
        }
    };
    for o in &view.openings {
        let c = opening_char(&o.ty, o.secret, o.implicit);
        for (segment, c) in [(o.segment, c), (o.far, FLOOR)] {
            let Some([a, b]) = segment else { continue };
            if (a.x - b.x).abs() >= (a.y - b.y).abs() {
                let line = ((a.y + b.y) as f64 / 2000.0).round() as i64 - y0;
                for col in a.x.min(b.x).div_euclid(1000)..-(-a.x.max(b.x)).div_euclid(1000) {
                    set(&mut grid, 2 * (col - x0) + 1, 2 * line, c);
                }
            } else {
                let line = ((a.x + b.x) as f64 / 2000.0).round() as i64 - x0;
                for row in a.y.min(b.y).div_euclid(1000)..-(-a.y.max(b.y)).div_euclid(1000) {
                    set(&mut grid, 2 * line, 2 * (row - y0) + 1, c);
                }
            }
        }
        if let Some(cell) = o.cell {
            set(
                &mut grid,
                2 * (cell.x0.div_euclid(1000) - x0) + 1,
                2 * (cell.y0.div_euclid(1000) - y0) + 1,
                c,
            );
        }
    }

    // Elements, in drawing order: a later element covers an earlier one.
    let mut legend: Vec<(char, String)> = used
        .iter()
        .map(|c| (*c, opening_label(*c).to_string()))
        .collect();
    for e in &view.elements {
        let glyph = e.glyph.chars().next().unwrap_or('?');
        let centre = ((e.bbox.x0 + e.bbox.x1) / 2, (e.bbox.y0 + e.bbox.y1) / 2);
        let (first_col, first_row) = (
            centre.0.min(e.bbox.x0 + 500).div_euclid(1000),
            centre.1.min(e.bbox.y0 + 500).div_euclid(1000),
        );
        let (last_col, last_row) = (
            (e.bbox.x1 - 500).max(centre.0).div_euclid(1000),
            (e.bbox.y1 - 500).max(centre.1).div_euclid(1000),
        );
        for row in first_row..=last_row {
            for col in first_col..=last_col {
                let (gx, gy) = (2 * (col - x0) + 1, 2 * (row - y0) + 1);
                if gx >= 0 && gy >= 0 && (gx as usize) < gw && (gy as usize) < gh {
                    grid[gy as usize][gx as usize] = glyph;
                }
            }
        }
        if !legend.iter().any(|(c, _)| *c == glyph) {
            legend.push((glyph, e.type_label.clone()));
        }
    }

    // Leave out the lattice lines that carry no wall and no opening.
    let plain = |c: &char| *c == FLOOR || *c == ' ';
    // A line carries a wall when one of its edge positions does; the corner
    // positions where it merely crosses another wall do not count.
    let keep_row: Vec<bool> = (0..gh)
        .map(|gy| gy % 2 == 1 || (1..gw).step_by(2).any(|gx| !plain(&grid[gy][gx])))
        .collect();
    let keep_col: Vec<bool> = (0..gw)
        .map(|gx| gx % 2 == 1 || (1..gh).step_by(2).any(|gy| !plain(&grid[gy][gx])))
        .collect();
    let mut lines: Vec<String> = (0..gh)
        .filter(|gy| keep_row[*gy])
        .map(|gy| {
            (0..gw)
                .filter(|gx| keep_col[*gx])
                .map(|gx| grid[gy][gx])
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    if !legend.is_empty()
        && let Some(first) = lines.first_mut()
    {
        let width = keep_col.iter().filter(|k| **k).count();
        let entries: Vec<String> = legend
            .iter()
            .map(|(c, label)| format!("{c} {label}"))
            .collect();
        *first = format!("{first:<width$}      {}", entries.join("    "));
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}
