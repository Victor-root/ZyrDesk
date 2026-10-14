//! The abstract picture ZyrDesk draws for itself.
//!
//! Every computer whose own wallpaper has not come carries it on its
//! card: no wallpaper is fetched. Ridges one behind the other under a
//! sky the gold of the logo warms, which is what « further, together »
//! looks like without a word: each ridge stands in front of the one
//! before it, and all of them under the same light.
//!
//! Every colour is a role of the design system, so the picture follows
//! the theme like the rest. Its shape comes from a number taken from the
//! computer's fingerprint, so two cards side by side never show the same
//! mountains, and a card shows the same ones from one day to the next.
//!
//! Where the ridges go is plain arithmetic and is tried everywhere; the
//! drawing itself is Windows'.

/// The ridges, from the furthest to the nearest.
pub(crate) const RIDGES: usize = 4;

/// Where each ridge stands and how high it rises, in shares of the
/// picture's height, from the furthest to the nearest.
///
/// The further a ridge, the higher it stands and the more it rises:
/// that is the whole of the depth, with the haze the colours add.
const BASE: [f32; RIDGES] = [0.52, 0.62, 0.74, 0.86];
const RISE: [f32; RIDGES] = [0.30, 0.25, 0.19, 0.13];

/// How many summits each ridge has across the picture.
const SUMMITS: [usize; RIDGES] = [4, 5, 4, 3];

/// How far past each edge a ridge starts and ends, in shares of the
/// width: a ridge cut at the very edge would show where it was cut.
const BEYOND: f32 = 0.06;

/// How round a summit is, against a dip: the share of its handles it
/// keeps. A mountain comes to a point and a valley does not, and ridges
/// drawn round at both read as waves.
const SUMMIT: f32 = 0.45;

/// The points a ridge passes through, in shares of the picture's width
/// and height, from left to right: summits and the dips between them.
pub(crate) fn ridges(seed: u32) -> [Vec<(f32, f32)>; RIDGES] {
    let mut chance = Chance::new(seed);
    std::array::from_fn(|ridge| {
        let turns = SUMMITS[ridge] * 2;
        (0..=turns)
            .map(|turn| {
                let step = (1.0 + BEYOND * 2.0) / turns as f32;
                // Every point but the two ends wanders a little, so no two
                // summits stand the same distance apart.
                let wander = (chance.next() - 0.5) * step * 0.7;
                let inside = turn > 0 && turn < turns;
                let x = -BEYOND + step * turn as f32 + if inside { wander } else { 0.0 };
                let height = if turn % 2 == 0 {
                    0.45 + chance.next() * 0.55
                } else {
                    0.06 + chance.next() * 0.28
                };
                (x, BASE[ridge] - RISE[ridge] * height)
            })
            .collect()
    })
}

/// The same row of numbers for the same seed, every time.
///
/// The picture must not change shape from one frame to the next, nor
/// from one day to the next: a card's mountains are part of how the
/// computer is recognised.
struct Chance(u32);

impl Chance {
    fn new(seed: u32) -> Self {
        // Never nought, the one number this sequence cannot leave.
        Chance(seed.wrapping_mul(0x9E37_79B9) | 1)
    }

    /// The next number, between nought and one.
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// The handles that draw a smooth line through a ridge's points, one
/// pair for each stretch between two of them.
///
/// Each stretch leaves a point in the direction of the line through its
/// two neighbours, which is what keeps the line unbroken: the curve
/// through the points that a hand would draw. A summit, every other
/// point from the first, keeps only part of its handles, which brings it
/// to a point.
pub(crate) fn smooth_line(points: &[(f32, f32)]) -> Vec<((f32, f32), (f32, f32))> {
    let last = points.len() as isize - 1;
    let at = |rank: isize| points[rank.clamp(0, last) as usize];
    let reach = |rank: isize| if rank % 2 == 0 { SUMMIT } else { 1.0 } / 6.0;
    (0..last.max(0))
        .map(|rank| {
            let (before, from, to, after) = (at(rank - 1), at(rank), at(rank + 1), at(rank + 2));
            let (leaving, arriving) = (reach(rank), reach(rank + 1));
            (
                (
                    from.0 + (to.0 - before.0) * leaving,
                    from.1 + (to.1 - before.1) * leaving,
                ),
                (
                    to.0 - (after.0 - from.0) * arriving,
                    to.1 - (after.1 - from.1) * arriving,
                ),
            )
        })
        .collect()
}

#[cfg(windows)]
pub use drawing::Scene;
#[cfg(windows)]
pub(crate) use drawing::draw;

/// The drawing of it, which is Windows'.
#[cfg(windows)]
mod drawing {
    use windows::Win32::Graphics::Direct2D::Common::{
        D2D_RECT_F, D2D1_BEZIER_SEGMENT, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED,
        D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN, D2D1_GRADIENT_STOP,
    };
    use windows::Win32::Graphics::Direct2D::{
        D2D1_EXTEND_MODE_CLAMP, D2D1_GAMMA_2_2, D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES,
        D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES, ID2D1Factory, ID2D1PathGeometry, ID2D1RenderTarget,
        ID2D1StrokeStyle,
    };
    use windows_numerics::Vector2;

    use super::{RIDGES, ridges, smooth_line};
    use crate::design::{Colour, palette};

    /// What the picture is asked to be.
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct Scene {
        /// Whether it is drawn in the light theme's colours or the dark
        /// one's.
        pub light: bool,
        /// The number its shape comes from.
        pub seed: u32,
    }

    /// Draws the scene over `width` by `height` pixels of this target,
    /// from its corner.
    pub(crate) fn draw(
        target: &ID2D1RenderTarget,
        factory: &ID2D1Factory,
        style: &ID2D1StrokeStyle,
        scene: &Scene,
        (width, height): (f32, f32),
        scale: f32,
    ) {
        let colours = palette(scene.light);
        let whole = D2D_RECT_F {
            left: 0.0,
            top: 0.0,
            right: width,
            bottom: height,
        };
        // SAFETY: a target between the start and the end of a drawing,
        // and brushes made from it and used on it for the length of
        // this call.
        unsafe {
            // The sky, from its height to the horizon.
            let sky = linear(
                target,
                (0.0, 0.0),
                (0.0, height),
                &[(0.0, colours.art_sky), (1.0, colours.art_horizon)],
            );
            if let Some(sky) = &sky {
                target.FillRectangle(&whole, sky);
            }
            // The light, low over the far ridges and to the right.
            let glow_at = (width * 0.7, height * 0.42);
            if let Ok(stops) = target.CreateGradientStopCollection(
                &[
                    stop(0.0, colours.art_glow),
                    stop(1.0, colours.art_glow.faded(0.0)),
                ],
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            ) && let Ok(glow) = target.CreateRadialGradientBrush(
                &D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES {
                    center: point(glow_at),
                    gradientOriginOffset: point((0.0, 0.0)),
                    radiusX: width * 0.42,
                    radiusY: height * 0.95,
                },
                None,
                &stops,
            ) {
                target.FillRectangle(&whole, &glow);
            }

            let shades = [
                colours.art_ridge_far,
                colours.art_ridge_mid,
                colours.art_ridge_near,
                colours.art_ridge_front,
            ];
            let lines = ridges(scene.seed);
            for (ridge, points) in lines.iter().enumerate() {
                let points: Vec<(f32, f32)> = points
                    .iter()
                    .map(|(x, y)| (x * width, y * height))
                    .collect();
                let summit = points.iter().map(|(_, y)| *y).fold(height, f32::min);
                // Lighter towards its foot: the haze between one ridge and
                // the next, which is what sets them apart without a line.
                let shade = shades[ridge];
                let foot = shade.mixed_with(colours.art_horizon, 0.62);
                if let (Some(shape), Some(brush)) = (
                    ridge_shape(factory, &points, height, true),
                    linear(
                        target,
                        (0.0, summit),
                        (0.0, height),
                        &[(0.0, shade), (1.0, foot)],
                    ),
                ) {
                    target.FillGeometry(&shape, &brush, None);
                }
                // The light catching the crest, fainter on the nearer
                // ridges, which face away from it.
                let rim = colours
                    .art_rim
                    .faded(1.0 - ridge as f32 / RIDGES as f32 * 0.6);
                if let (Some(crest), Ok(brush)) = (
                    ridge_shape(factory, &points, height, false),
                    target.CreateSolidColorBrush(&tint(rim), None),
                ) {
                    target.DrawGeometry(&crest, &brush, scale, style);
                }
            }
        }
    }

    /// A ridge's outline: closed down to the bottom of the picture to be
    /// filled, or only its crest to be stroked.
    fn ridge_shape(
        factory: &ID2D1Factory,
        points: &[(f32, f32)],
        height: f32,
        filled: bool,
    ) -> Option<ID2D1PathGeometry> {
        let (first, last) = (*points.first()?, *points.last()?);
        // SAFETY: a shape and its sink, both ours, closed again before
        // leaving.
        unsafe {
            let shape = factory.CreatePathGeometry().ok()?;
            let sink = shape.Open().ok()?;
            if filled {
                sink.BeginFigure(point((first.0, height)), D2D1_FIGURE_BEGIN_FILLED);
                sink.AddLine(point(first));
            } else {
                sink.BeginFigure(point(first), D2D1_FIGURE_BEGIN_HOLLOW);
            }
            for (rank, (one, two)) in smooth_line(points).into_iter().enumerate() {
                sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                    point1: point(one),
                    point2: point(two),
                    point3: point(points[rank + 1]),
                });
            }
            if filled {
                sink.AddLine(point((last.0, height)));
                sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            } else {
                sink.EndFigure(D2D1_FIGURE_END_OPEN);
            }
            sink.Close().ok()?;
            Some(shape)
        }
    }

    /// A brush that goes from one colour to the next along a line.
    ///
    /// # Safety
    ///
    /// Between the start and the end of a drawing on `target`.
    unsafe fn linear(
        target: &ID2D1RenderTarget,
        from: (f32, f32),
        to: (f32, f32),
        stops: &[(f32, Colour)],
    ) -> Option<windows::Win32::Graphics::Direct2D::ID2D1LinearGradientBrush> {
        let stops: Vec<D2D1_GRADIENT_STOP> = stops
            .iter()
            .map(|(at, colour)| stop(*at, *colour))
            .collect();
        // SAFETY: as the caller vouches.
        unsafe {
            let collection = target
                .CreateGradientStopCollection(&stops, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP)
                .ok()?;
            target
                .CreateLinearGradientBrush(
                    &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                        startPoint: point(from),
                        endPoint: point(to),
                    },
                    None,
                    &collection,
                )
                .ok()
        }
    }

    fn stop(position: f32, colour: Colour) -> D2D1_GRADIENT_STOP {
        D2D1_GRADIENT_STOP {
            position,
            color: tint(colour),
        }
    }

    fn tint(colour: Colour) -> D2D1_COLOR_F {
        D2D1_COLOR_F {
            r: colour.red,
            g: colour.green,
            b: colour.blue,
            a: colour.alpha,
        }
    }

    fn point(at: (f32, f32)) -> Vector2 {
        Vector2 { X: at.0, Y: at.1 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_draws_the_same_mountains() {
        assert_eq!(ridges(7), ridges(7));
        assert_ne!(ridges(7), ridges(8));
    }

    #[test]
    fn every_ridge_runs_past_both_edges_and_stays_in_the_picture() {
        for seed in [0, 1, 42, u32::MAX] {
            for points in ridges(seed) {
                assert!(points.first().unwrap().0 < 0.0);
                assert!(points.last().unwrap().0 > 1.0);
                assert!(points.windows(2).all(|pair| pair[0].0 < pair[1].0));
                assert!(points.iter().all(|(_, y)| (0.0..1.0).contains(y)));
            }
        }
    }

    #[test]
    fn a_smooth_line_has_one_pair_of_handles_per_stretch() {
        let points = [(1.0, 0.0), (2.0, 1.0), (3.0, 0.0)];
        let handles = smooth_line(&points);
        assert_eq!(handles.len(), 2);
        // In a dip the line is level: the handle arriving and the one
        // leaving stand at the same height, and as far from it.
        assert_eq!(handles[0].1.1, handles[1].0.1);
        assert!(((2.0 - handles[0].1.0) - (handles[1].0.0 - 2.0)).abs() < 1e-6);
        // At a summit the handles reach less far than in a dip.
        assert!(handles[0].0.0 - 1.0 < 2.0 - handles[0].1.0);
    }
}
