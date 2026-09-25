//! Sizes and places: the picture a screen gives, and where a pointer
//! lands.
//!
//! The screen being filmed rarely has the shape a viewer asks for. The
//! picture keeps the screen's proportions, as large as fits in what was
//! asked but never larger than the screen itself: blowing a screen up
//! before encoding costs bits and time and adds no detail the viewer's
//! own scaling would not give. The player shapes its window on the size
//! it is told.
//!
//! The viewer points at the picture in coordinates from 0 to 65535 each
//! way; the host plays that point on the screen being filmed, placed on
//! the desktop that spans all of this computer's screens.

/// A size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// A rectangle in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}

/// The picture sent for a screen of `screen` pixels when `asked` was
/// asked: the screen's proportions, as large as fits in what was asked
/// and no larger than the screen, both sides even and at least 2.
///
/// A side of nothing asked means the screen's own.
pub fn picture_size(screen: Size, asked: Size) -> Size {
    let bound = |asked: u32, screen: u32| match (asked, screen) {
        (0, screen) => screen,
        (asked, 0) => asked,
        (asked, screen) => asked.min(screen),
    };
    let room = Size::new(
        bound(asked.width, screen.width),
        bound(asked.height, screen.height),
    );
    let fitted = if screen.is_empty() {
        room
    } else {
        fit(screen, room)
    };
    Size::new(even_floor(fitted.width), even_floor(fitted.height))
}

/// Where an image of `image` pixels is drawn in a picture of `picture`
/// pixels: with its own proportions, as large as fits, centred, the rest
/// black. Every edge falls on an even pixel, so that the half-size
/// chroma plane lines up with it.
///
/// Bars no wider than what rounding the picture's sides down to even
/// leaves are not drawn: the image is stretched over them, a difference
/// of proportions nobody can see where a black line at the edge would
/// be. Rounding one side takes up to two pixels off it, which shows on
/// the other side as that much times the picture's proportions.
pub fn placement(image: Size, picture: Size) -> Rect {
    let whole = Rect::new(0, 0, picture.width, picture.height);
    if image.is_empty() || picture.is_empty() {
        return whole;
    }
    let fitted = fit(image, picture);
    let width = even_floor(fitted.width).min(picture.width);
    let height = even_floor(fitted.height).min(picture.height);
    let slack = |along: u32, across: u32| ROUNDED_OFF * along.div_ceil(across) + ROUNDED_OFF;
    if picture.width - width <= slack(picture.width, picture.height)
        && picture.height - height <= slack(picture.height, picture.width)
    {
        return whole;
    }
    let x = even((picture.width - width) / 2);
    let y = even((picture.height - height) / 2);
    Rect::new(x as i32, y as i32, width, height)
}

/// Pixels a side of the picture loses at most when rounded down to even.
const ROUNDED_OFF: u32 = 2;

/// The largest size with the proportions of `shape` inside `room`.
fn fit(shape: Size, room: Size) -> Size {
    let (sw, sh) = (u64::from(shape.width), u64::from(shape.height));
    let (rw, rh) = (u64::from(room.width), u64::from(room.height));
    if rw * sh <= rh * sw {
        Size::new(room.width, (rw * sh / sw) as u32)
    } else {
        Size::new((rh * sw / sh) as u32, room.height)
    }
}

/// Down to even.
fn even(value: u32) -> u32 {
    value & !1
}

/// Down to even, and never below 2: a picture of nothing encodes nothing.
fn even_floor(value: u32) -> u32 {
    even(value).max(2)
}

/// The largest coordinate the viewer sends: its right or bottom edge.
const FAR_EDGE: u64 = 65_536;

/// How the picture maps onto this computer's desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mapping {
    /// The picture sent.
    pub picture: Size,
    /// Where the screen's image sits in it.
    pub placement: Rect,
    /// The filmed screen, on the desktop spanning every screen.
    pub screen: Rect,
    /// The desktop spanning every screen.
    pub desktop: Rect,
}

impl Mapping {
    /// The pixel of the filmed screen under `(x, y)`, in the viewer's
    /// coordinates from 0 to 65535 across the picture. A point on the
    /// black bars goes to the nearest edge of the screen.
    pub fn screen_pixel(&self, x: u16, y: u16) -> (u32, u32) {
        (
            onto_screen(
                x,
                self.picture.width,
                self.placement.x,
                self.placement.width,
                self.screen.width,
            ),
            onto_screen(
                y,
                self.picture.height,
                self.placement.y,
                self.placement.height,
                self.screen.height,
            ),
        )
    }

    /// The same point on the desktop spanning every screen, in its
    /// pixels (which may be negative, left of or above the main screen).
    pub fn desktop_pixel(&self, x: u16, y: u16) -> (i32, i32) {
        let (sx, sy) = self.screen_pixel(x, y);
        (
            self.screen.x.saturating_add_unsigned(sx),
            self.screen.y.saturating_add_unsigned(sy),
        )
    }

    /// The same point as Windows takes an absolute move over the desktop
    /// spanning every screen: from 0 to 65535 across it, aimed at the
    /// middle of the pixel so that rounding either way lands on it.
    pub fn absolute(&self, x: u16, y: u16) -> (i32, i32) {
        let (dx, dy) = self.desktop_pixel(x, y);
        (
            normalised(dx, self.desktop.x, self.desktop.width),
            normalised(dy, self.desktop.y, self.desktop.height),
        )
    }
}

/// One axis of [`Mapping::screen_pixel`]: the point is carried across
/// without being rounded to a pixel of the picture on the way, so that
/// the edges of a picture smaller than its screen still reach the
/// screen's own edges.
fn onto_screen(at: u16, picture: u32, offset: i32, placed: u32, screen: u32) -> u32 {
    if placed == 0 || screen == 0 {
        return 0;
    }
    let far = i128::from(FAR_EDGE);
    // Where the point falls in the image, in pixels times FAR_EDGE.
    let in_image = i128::from(at) * i128::from(picture) - i128::from(offset) * far;
    let on_screen = in_image.max(0) * i128::from(screen) / (i128::from(placed) * far);
    on_screen.min(i128::from(screen) - 1) as u32
}

/// One axis of [`Mapping::absolute`].
fn normalised(pixel: i32, start: i32, span: u32) -> i32 {
    if span == 0 {
        return 0;
    }
    let inside = (i64::from(pixel) - i64::from(start)).clamp(0, i64::from(span) - 1);
    let middle = (inside * 2 + 1) * FAR_EDGE as i64 / (2 * i64::from(span));
    middle.min(FAR_EDGE as i64 - 1) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screen_of_the_shape_asked_is_sent_at_the_size_asked() {
        assert_eq!(
            picture_size(Size::new(1920, 1080), Size::new(1920, 1080)),
            Size::new(1920, 1080)
        );
        assert_eq!(
            picture_size(Size::new(3840, 2160), Size::new(1280, 720)),
            Size::new(1280, 720)
        );
    }

    #[test]
    fn a_screen_of_another_shape_keeps_its_own_inside_what_was_asked() {
        // 16:10 into 16:9: the height decides.
        assert_eq!(
            picture_size(Size::new(2560, 1600), Size::new(1920, 1080)),
            Size::new(1728, 1080)
        );
        // 4:3 into 21:9.
        assert_eq!(
            picture_size(Size::new(1600, 1200), Size::new(3440, 1440)),
            Size::new(1600, 1200)
        );
        // 21:9 into 16:9: the width decides.
        assert_eq!(
            picture_size(Size::new(3440, 1440), Size::new(1920, 1080)),
            Size::new(1920, 802)
        );
    }

    #[test]
    fn a_picture_is_never_larger_than_its_screen() {
        assert_eq!(
            picture_size(Size::new(1920, 1080), Size::new(3840, 2160)),
            Size::new(1920, 1080)
        );
        assert_eq!(
            picture_size(Size::new(1366, 768), Size::new(2560, 1440)),
            Size::new(1366, 768)
        );
    }

    #[test]
    fn both_sides_are_even_and_never_nothing() {
        let odd = picture_size(Size::new(1365, 767), Size::new(1365, 767));
        assert_eq!(odd, Size::new(1364, 766));
        let tiny = picture_size(Size::new(1920, 1080), Size::new(1, 1));
        assert_eq!(tiny, Size::new(2, 2));
        for (screen, asked) in [
            ((1920, 1080), (1000, 999)),
            ((1080, 1920), (1920, 1080)),
            ((1024, 768), (333, 777)),
        ] {
            let size = picture_size(Size::new(screen.0, screen.1), Size::new(asked.0, asked.1));
            assert!(
                size.width.is_multiple_of(2) && size.height.is_multiple_of(2),
                "{size:?}"
            );
            assert!(size.width <= asked.0.max(2) && size.height <= asked.1.max(2));
        }
    }

    #[test]
    fn nothing_asked_means_the_screen_itself() {
        assert_eq!(
            picture_size(Size::new(2560, 1440), Size::new(0, 0)),
            Size::new(2560, 1440)
        );
        assert_eq!(
            picture_size(Size::new(0, 0), Size::new(1280, 720)),
            Size::new(1280, 720)
        );
    }

    #[test]
    fn an_image_of_the_picture_s_shape_fills_it() {
        assert_eq!(
            placement(Size::new(3840, 2160), Size::new(1920, 1080)),
            Rect::new(0, 0, 1920, 1080)
        );
        // The picture was rounded down to even: the pixel or two of bars
        // that leaves are not worth drawing.
        assert_eq!(
            placement(Size::new(1365, 767), Size::new(1364, 766)),
            Rect::new(0, 0, 1364, 766)
        );
        assert_eq!(
            placement(Size::new(3440, 1440), Size::new(1920, 802)),
            Rect::new(0, 0, 1920, 802)
        );
    }

    #[test]
    fn an_image_of_another_shape_is_centred_between_black_bars_on_even_pixels() {
        let placed = placement(Size::new(1600, 1200), Size::new(1920, 1080));
        assert_eq!(placed, Rect::new(240, 0, 1440, 1080));
        let placed = placement(Size::new(1920, 1080), Size::new(1000, 1000));
        assert_eq!(placed, Rect::new(0, 218, 1000, 562));
        for rect in [placed, placement(Size::new(333, 777), Size::new(640, 480))] {
            for edge in [rect.x as u32, rect.y as u32, rect.width, rect.height] {
                assert_eq!(edge % 2, 0, "{rect:?}");
            }
        }
    }

    fn mapping() -> Mapping {
        // A 2560x1440 screen right of a 1920x1080 main screen that sits
        // lower, sent as a 1280x720 picture.
        Mapping {
            picture: Size::new(1280, 720),
            placement: Rect::new(0, 0, 1280, 720),
            screen: Rect::new(1920, -200, 2560, 1440),
            desktop: Rect::new(0, -200, 4480, 1440),
        }
    }

    #[test]
    fn the_corners_of_the_picture_are_the_corners_of_the_screen() {
        let map = mapping();
        assert_eq!(map.screen_pixel(0, 0), (0, 0));
        assert_eq!(map.screen_pixel(65535, 65535), (2559, 1439));
        assert_eq!(map.desktop_pixel(0, 0), (1920, -200));
        assert_eq!(map.desktop_pixel(65535, 65535), (4479, 1239));
    }

    #[test]
    fn the_middle_of_the_picture_is_the_middle_of_the_screen() {
        let (x, y) = mapping().screen_pixel(32768, 32768);
        assert!((1279..=1281).contains(&x), "{x}");
        assert!((719..=721).contains(&y), "{y}");
    }

    #[test]
    fn points_on_the_bars_go_to_the_nearest_edge() {
        let map = Mapping {
            picture: Size::new(1920, 1080),
            placement: Rect::new(240, 0, 1440, 1080),
            screen: Rect::new(0, 0, 1600, 1200),
            desktop: Rect::new(0, 0, 1600, 1200),
        };
        assert_eq!(map.screen_pixel(0, 0), (0, 0));
        assert_eq!(map.screen_pixel(65535, 65535), (1599, 1199));
        // Just inside the left edge of the image.
        let left = (240 * 65536 / 1920 + 1) as u16;
        assert_eq!(map.screen_pixel(left, 0).0, 0);
    }

    /// What Windows does with an absolute coordinate: the pixel it falls
    /// in, across the desktop.
    fn windows_pixel(normalised: i32, start: i32, span: u32) -> i32 {
        start + (i64::from(normalised) * i64::from(span) / FAR_EDGE as i64) as i32
    }

    #[test]
    fn an_absolute_move_lands_on_the_pixel_pointed_at() {
        let map = mapping();
        for (x, y) in [(0, 0), (65535, 65535), (12345, 54321), (32768, 1)] {
            let (px, py) = map.desktop_pixel(x, y);
            let (nx, ny) = map.absolute(x, y);
            assert!((0..65536).contains(&nx) && (0..65536).contains(&ny));
            assert_eq!(windows_pixel(nx, map.desktop.x, map.desktop.width), px);
            assert_eq!(windows_pixel(ny, map.desktop.y, map.desktop.height), py);
        }
    }

    #[test]
    fn a_screen_left_of_and_above_the_main_one_is_reached_too() {
        // A 1920x1080 screen whose corner lies left of and above the main
        // screen's: the desktop spanning both starts in the negatives.
        let map = Mapping {
            picture: Size::new(1920, 1080),
            placement: Rect::new(0, 0, 1920, 1080),
            screen: Rect::new(-1920, -300, 1920, 1080),
            desktop: Rect::new(-1920, -300, 4480, 1740),
        };
        assert_eq!(map.desktop_pixel(0, 0), (-1920, -300));
        assert_eq!(map.desktop_pixel(65535, 65535), (-1, 779));
        for (x, y) in [(0, 0), (65535, 65535), (40000, 20000)] {
            let (px, py) = map.desktop_pixel(x, y);
            let (nx, ny) = map.absolute(x, y);
            assert_eq!(windows_pixel(nx, map.desktop.x, map.desktop.width), px);
            assert_eq!(windows_pixel(ny, map.desktop.y, map.desktop.height), py);
        }
    }

    #[test]
    fn every_viewer_pixel_reaches_a_distinct_screen_pixel_at_equal_size() {
        let map = Mapping {
            picture: Size::new(640, 480),
            placement: Rect::new(0, 0, 640, 480),
            screen: Rect::new(0, 0, 640, 480),
            desktop: Rect::new(0, 0, 640, 480),
        };
        for pixel in 0..640u32 {
            // The viewer aims at the middle of its pixel.
            let at = ((u64::from(pixel) * 2 + 1) * FAR_EDGE / 1280) as u16;
            assert_eq!(map.screen_pixel(at, 0).0, pixel);
        }
    }
}
