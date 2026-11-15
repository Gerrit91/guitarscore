//! Icons drawn from outlines: filled shapes with holes and curves, which egui's painter
//! cannot fill itself (it fills convex polygons only). Each icon is rasterized once per
//! pixel size into a white coverage mask, kept as a texture, and painted tinted in the
//! ink's color.
//!
//! An icon is a list of [`Part`]s in a 100 by 100 box. Its outlines are SVG-style paths
//! made of `M`, `L`, `C` (cubic curve) and `Z` with absolute coordinates.

use egui::{Color32, ColorImage, Painter, Rect, TextureHandle, TextureOptions, pos2, vec2};

/// One shape of an icon.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Part {
    /// A filled shape: the first path is its outline, any further paths are holes (filled
    /// by the even-odd rule, so a hole's direction does not matter).
    Fill(&'static [&'static str]),
    /// A stroke along a path, this wide (in box units), with round ends.
    Line(&'static str, f32),
}

/// How many samples a pixel takes across and down (16 per pixel, smooth edges).
const SUPERSAMPLE: usize = 4;
/// How many straight pieces a curve is drawn with.
const CURVE_STEPS: usize = 12;
/// The largest mask made; larger icons are drawn from it, scaled.
const MAX_PIXELS: usize = 256;

/// Paints the icon `parts` (known as `name`) inside `rect`, square and centered, in `ink`.
pub(crate) fn paint(
    painter: &Painter,
    rect: Rect,
    name: &'static str,
    parts: &'static [Part],
    ink: Color32,
) {
    let side = rect.width().min(rect.height());
    if side <= 0.0 || !side.is_finite() {
        return;
    }
    let square = Rect::from_center_size(rect.center(), vec2(side, side));
    let ctx = painter.ctx();
    let pixels = ((side * ctx.pixels_per_point()).ceil() as usize).clamp(1, MAX_PIXELS);
    let id = egui::Id::unique(("gscore_vector_icon", name, pixels));
    let texture = ctx.data_mut(|data| data.get_temp::<TextureHandle>(id));
    let texture = texture.unwrap_or_else(|| {
        let image = rasterize(parts, pixels);
        let handle = ctx.load_texture(
            format!("icon-{name}-{pixels}"),
            image,
            TextureOptions::LINEAR,
        );
        ctx.data_mut(|data| data.insert_temp(id, handle.clone()));
        handle
    });
    painter.image(
        texture.id(),
        square,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        ink,
    );
}

/// The icon as a white mask `pixels` wide and high: each pixel's alpha is how much of it
/// the shapes cover.
pub(crate) fn rasterize(parts: &[Part], pixels: usize) -> ColorImage {
    let samples = pixels * SUPERSAMPLE;
    let scale = samples as f32 / 100.0;
    // Every shape as closed polygons in sample coordinates, filled by the even-odd rule:
    // a fill with its holes is one shape; each piece of a line is a shape of its own, so
    // pieces that overlap where they meet do not cancel out.
    let mut shapes: Vec<Vec<Vec<(f32, f32)>>> = Vec::new();
    for part in parts {
        match part {
            Part::Fill(paths) => shapes.push(
                paths
                    .iter()
                    .flat_map(|path| polygons(path))
                    .map(|polygon| scaled(polygon, scale))
                    .collect(),
            ),
            Part::Line(path, width) => {
                for points in polygons(path) {
                    for pair in points.windows(2) {
                        if let [a, b] = pair {
                            shapes.push(vec![scaled(capsule(*a, *b, width / 2.0), scale)]);
                        }
                    }
                }
            }
        }
    }
    let mut coverage = vec![0_u16; pixels * pixels];
    let mut inside = vec![false; samples];
    let mut crossings = Vec::new();
    for row in 0..samples {
        let y = row as f32 + 0.5;
        inside.fill(false);
        for shape in &shapes {
            crossings.clear();
            for polygon in shape {
                for (index, &(x0, y0)) in polygon.iter().enumerate() {
                    let Some(&(x1, y1)) = polygon.get((index + 1) % polygon.len()) else {
                        continue;
                    };
                    if (y0 <= y) != (y1 <= y) {
                        crossings.push(x0 + (y - y0) / (y1 - y0) * (x1 - x0));
                    }
                }
            }
            crossings.sort_by(f32::total_cmp);
            for &[from, to] in crossings.as_chunks::<2>().0 {
                // The samples whose centers lie between the two crossings.
                let first = (from - 0.5).ceil().max(0.0) as usize;
                let last = ((to - 0.5).floor().min(samples as f32 - 1.0)).max(-1.0);
                if last < 0.0 {
                    continue;
                }
                for sample in inside.iter_mut().take(last as usize + 1).skip(first) {
                    *sample = true;
                }
            }
        }
        let pixel_row = row / SUPERSAMPLE;
        for (column, on) in inside.iter().enumerate() {
            if *on && let Some(cell) = coverage.get_mut(pixel_row * pixels + column / SUPERSAMPLE) {
                *cell += 1;
            }
        }
    }
    let full = (SUPERSAMPLE * SUPERSAMPLE) as f32;
    let pixels_rgba: Vec<Color32> = coverage
        .iter()
        .map(|&count| {
            let alpha = (f32::from(count) / full * 255.0).round().clamp(0.0, 255.0) as u8;
            Color32::from_white_alpha(alpha)
        })
        .collect();
    ColorImage::new([pixels, pixels], pixels_rgba)
}

fn scaled(polygon: Vec<(f32, f32)>, scale: f32) -> Vec<(f32, f32)> {
    polygon
        .into_iter()
        .map(|(x, y)| (x * scale, y * scale))
        .collect()
}

/// A round-ended stroke from `a` to `b`, `radius` thick on each side, as a polygon.
fn capsule(a: (f32, f32), b: (f32, f32), radius: f32) -> Vec<(f32, f32)> {
    let angle = (b.1 - a.1).atan2(b.0 - a.0);
    let half_turn = std::f32::consts::PI;
    let mut points = Vec::with_capacity(2 * (CURVE_STEPS + 1));
    // Round the far end, then the near one.
    for (center, start) in [(b, angle - half_turn / 2.0), (a, angle + half_turn / 2.0)] {
        for step in 0..=CURVE_STEPS {
            let t = start + half_turn * step as f32 / CURVE_STEPS as f32;
            points.push((center.0 + radius * t.cos(), center.1 + radius * t.sin()));
        }
    }
    points
}

/// The closed polygons (or, for a path without `Z`, open point lists) a path is made of,
/// with its curves flattened. Anything unreadable ends the path there.
fn polygons(path: &str) -> Vec<Vec<(f32, f32)>> {
    let mut out: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut current: Vec<(f32, f32)> = Vec::new();
    let mut tokens = path.split_whitespace().peekable();
    let number = |tokens: &mut std::iter::Peekable<std::str::SplitWhitespace<'_>>| {
        tokens.next().and_then(|token| token.parse::<f32>().ok())
    };
    while let Some(command) = tokens.next() {
        match command {
            "M" => {
                let (Some(x), Some(y)) = (number(&mut tokens), number(&mut tokens)) else {
                    break;
                };
                if current.len() > 1 {
                    out.push(std::mem::take(&mut current));
                }
                current = vec![(x, y)];
            }
            "L" => {
                let (Some(x), Some(y)) = (number(&mut tokens), number(&mut tokens)) else {
                    break;
                };
                current.push((x, y));
            }
            "C" => {
                let mut values = [0.0_f32; 6];
                let mut complete = true;
                for value in &mut values {
                    match number(&mut tokens) {
                        Some(read) => *value = read,
                        None => complete = false,
                    }
                }
                let Some(&start) = current.last() else {
                    break;
                };
                if !complete {
                    break;
                }
                let [x1, y1, x2, y2, x3, y3] = values;
                for step in 1..=CURVE_STEPS {
                    let t = step as f32 / CURVE_STEPS as f32;
                    let u = 1.0 - t;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    current.push((
                        a * start.0 + b * x1 + c * x2 + d * x3,
                        a * start.1 + b * y1 + c * y2 + d * y3,
                    ));
                }
            }
            "Z" => {
                if current.len() > 1 {
                    out.push(std::mem::take(&mut current));
                }
            }
            _ => break,
        }
    }
    if current.len() > 1 {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(image: &ColorImage, x: usize, y: usize) -> u8 {
        image
            .pixels
            .get(y * image.size[0] + x)
            .map_or(0, |pixel| pixel.a())
    }

    #[test]
    fn a_filled_square_with_a_hole_covers_its_ring_only() {
        const RING: &[Part] = &[Part::Fill(&[
            "M 10 10 L 90 10 L 90 90 L 10 90 Z",
            "M 30 30 L 70 30 L 70 70 L 30 70 Z",
        ])];
        let image = rasterize(RING, 20);
        assert_eq!(alpha(&image, 0, 0), 0, "outside");
        assert_eq!(alpha(&image, 3, 10), 255, "in the ring");
        assert_eq!(alpha(&image, 10, 10), 0, "in the hole");
    }

    #[test]
    fn a_curve_and_a_line_are_drawn() {
        const ROUND: &[Part] = &[
            Part::Fill(&["M 50 10 C 72 10 90 28 90 50 C 90 72 72 90 50 90 Z"]),
            Part::Line("M 10 10 L 10 90", 8.0),
        ];
        let image = rasterize(ROUND, 50);
        assert_eq!(alpha(&image, 40, 25), 255, "inside the half disc");
        assert_eq!(alpha(&image, 20, 25), 0, "left of it");
        assert!(alpha(&image, 5, 25) > 200, "on the line");
        assert!(
            alpha(&image, 5, 1) < 255,
            "the line's round end is not square"
        );
    }

    #[test]
    fn broken_paths_draw_nothing_rather_than_fail() {
        const BROKEN: &[Part] = &[Part::Fill(&["M 10 10 L 90", "C 1 2", "X"])];
        let image = rasterize(BROKEN, 10);
        assert!(image.pixels.iter().all(|pixel| pixel.a() == 0));
    }
}
