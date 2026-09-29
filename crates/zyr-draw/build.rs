use std::fmt::Write as _;

/// The design system, read from the one file that holds it.
///
/// Colours, spacings, radii, shadows and text sizes are written once, in
/// `design.css`, and everything ZyrDesk draws reads them from there
/// rather than keeping a second copy. Two copies of a palette is
/// two palettes: the first colour changed in one of them is the day the
/// product stops looking like itself.
///
/// It keeps the notation it was written in, and no browser reads it any
/// more: a stylesheet writes two themes side by side, and reading it
/// here is what checks that the two declare the same roles. Transcribing
/// forty values into Rust by hand would be the one thing this whole file
/// exists to prevent.
const DESIGN: &str = "design.css";

fn main() {
    println!("cargo:rerun-if-changed={DESIGN}");
    let css = std::fs::read_to_string(DESIGN)
        .unwrap_or_else(|e| panic!("the design system {DESIGN} could not be read: {e}"));
    let written = design(&plain(&css));
    let out = std::path::Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("design.rs");
    std::fs::write(&out, written).expect("the design system is written");
}

/// The stylesheet with its comments taken out, so a colour named inside
/// one is never read as a value.
fn plain(css: &str) -> String {
    let mut plain = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(at) = rest.find("/*") {
        plain.push_str(&rest[..at]);
        match rest[at..].find("*/") {
            Some(end) => rest = &rest[at + end + 2..],
            None => return plain,
        }
    }
    plain.push_str(rest);
    plain
}

/// What one block of the stylesheet declares, in the order it declares
/// it.
fn block<'a>(css: &'a str, selector: &str) -> Vec<(String, &'a str)> {
    let from = css
        .find(selector)
        .unwrap_or_else(|| panic!("{selector} is not found in {DESIGN}"));
    let open = css[from..]
        .find('{')
        .unwrap_or_else(|| panic!("{selector} opens no brace"))
        + from;
    let close = css[open..]
        .find('}')
        .unwrap_or_else(|| panic!("{selector} does not close"))
        + open;
    css[open + 1..close]
        .split(';')
        .filter_map(|line| line.split_once(':'))
        .filter_map(|(name, value)| {
            let name = name.trim().strip_prefix("--")?;
            Some((name.replace('-', "_"), value.trim()))
        })
        .collect()
}

/// What a declared value turns out to be.
///
/// Told from the value itself rather than from a list of names kept here:
/// a list would have to be edited every time the design system gains a
/// token, and a list nobody edits is a build that fails for the wrong
/// reason.
enum Sort {
    Colour(String),
    Shadow(String),
    Length(f32),
    Time(u64),
}

impl Sort {
    fn kind(&self) -> &'static str {
        match self {
            Sort::Colour(_) => "Colour",
            Sort::Shadow(_) => "Shadow",
            Sort::Length(_) => "f32",
            Sort::Time(_) => "u64",
        }
    }

    fn written(&self) -> String {
        match self {
            Sort::Colour(said) | Sort::Shadow(said) => said.clone(),
            Sort::Length(number) => format!("{number:?}"),
            Sort::Time(number) => number.to_string(),
        }
    }
}

/// Names the drawing has no use for, and which are therefore not asked to
/// be readable. Said out loud here rather than skipped in silence: a
/// value quietly dropped is a value that stops being carried the day
/// somebody needs it.
const NOT_DRAWN: [&str; 1] = ["curve"];

/// The whole of the generated file.
fn design(css: &str) -> String {
    let dark = block(css, ":root");
    let light = block(css, r#":root[data-theme="light"]"#);

    // The palette is exactly what the light theme says again, and the
    // rest is the same whatever the theme. Read that way, the two follow
    // the stylesheet on their own: a colour added to both blocks joins
    // the palette, a spacing added to one joins the constants.
    let mut palette = String::new();
    let mut dark_fields = String::new();
    let mut light_fields = String::new();
    let mut apart = String::new();

    for (name, value) in &dark {
        if NOT_DRAWN.contains(&name.trim_start_matches("r#")) {
            continue;
        }
        let mine = read(name, value);
        match light.iter().find(|(other, _)| other == name) {
            Some((_, other)) => {
                let theirs = read(name, other);
                assert_eq!(
                    mine.kind(),
                    theirs.kind(),
                    "« {name} » is not of the same sort in the two themes"
                );
                let _ = writeln!(palette, "    pub {name}: {},", mine.kind());
                let _ = writeln!(dark_fields, "    {name}: {},", mine.written());
                let _ = writeln!(light_fields, "    {name}: {},", theirs.written());
            }
            None => {
                let _ = writeln!(
                    apart,
                    "pub const {}: {} = {};",
                    name.trim_start_matches("r#").to_uppercase(),
                    mine.kind(),
                    mine.written()
                );
            }
        }
    }

    format!(
        "// Written by build.rs from {DESIGN}. Not to be edited by hand:\n\
         // the stylesheet decides, and it alone.\n\
         \n\
         /// What a theme says of each role.\n\
         #[derive(Clone, Copy)]\n\
         pub struct Palette {{\n{palette}}}\n\
         \n\
         /// The dark theme, the one the stylesheet lays down first.\n\
         pub const DARK: Palette = Palette {{\n{dark_fields}}};\n\
         \n\
         /// The light theme, the one it says again afterwards.\n\
         pub const LIGHT: Palette = Palette {{\n{light_fields}}};\n\
         \n\
         {apart}"
    )
}

/// One declared value, read into what it is.
fn read(name: &str, value: &str) -> Sort {
    if let Some(colour) = colour(value) {
        return Sort::Colour(colour);
    }
    if let Some(shadow) = shadow(value) {
        return Sort::Shadow(shadow);
    }
    if let Some(number) = value.strip_suffix("px") {
        return Sort::Length(
            number.trim().parse().unwrap_or_else(|e| {
                panic!("« {name} » is « {value} », which is not a length: {e}")
            }),
        );
    }
    if let Some(number) = value.strip_suffix("ms") {
        return Sort::Time(number.trim().parse().unwrap_or_else(|e| {
            panic!("« {name} » is « {value} », which is not a duration: {e}")
        }));
    }
    panic!(
        "« {name} » is « {value} », which the drawing cannot read. \
         Add it to NOT_DRAWN in build.rs if it is not to be drawn."
    )
}

/// A colour, written as six digits or as four numbers.
fn colour(value: &str) -> Option<String> {
    if let Some(digits) = value.strip_prefix('#') {
        if digits.len() != 6 {
            return None;
        }
        let band = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
        let (red, green, blue) = (band(0)?, band(2)?, band(4)?);
        return Some(written(
            f32::from(red) / 255.0,
            f32::from(green) / 255.0,
            f32::from(blue) / 255.0,
            1.0,
        ));
    }
    let inside = value.strip_prefix("rgba(")?.strip_suffix(')')?;
    let numbers: Vec<f32> = inside
        .split(',')
        .filter_map(|part| part.trim().parse().ok())
        .collect();
    let [red, green, blue, alpha] = numbers[..] else {
        return None;
    };
    Some(written(red / 255.0, green / 255.0, blue / 255.0, alpha))
}

/// A shadow: how far across, how far down, how soft, and in what colour.
fn shadow(value: &str) -> Option<String> {
    let (lengths, tint) = value.split_once("rgba(")?;
    let colour = colour(&format!("rgba({tint}"))?;
    let numbers: Vec<f32> = lengths
        .split_whitespace()
        .map(|part| part.trim_end_matches("px").parse().unwrap_or(f32::NAN))
        .collect();
    let [across, down, soft] = numbers[..] else {
        return None;
    };
    if [across, down, soft].iter().any(|number| number.is_nan()) {
        return None;
    }
    Some(format!(
        "Shadow {{ across: {across:?}, down: {down:?}, soft: {soft:?}, tint: {colour} }}"
    ))
}

/// A colour as the drawing wants it: four numbers between nought and one.
fn written(red: f32, green: f32, blue: f32, alpha: f32) -> String {
    format!("Colour {{ red: {red:?}, green: {green:?}, blue: {blue:?}, alpha: {alpha:?} }}")
}
