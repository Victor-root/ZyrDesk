use std::fmt::Write as _;

/// The design system, read from the one file that holds it.
///
/// Colours, spacings, radii, shadows and text sizes are written once, in
/// `design.css`, and the interface drawn by this program reads them from
/// there rather than keeping a second copy. Two copies of a palette is
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
    // The program's icon is a Windows resource made here, from that
    // file. Without this line, Cargo does not redo that work when the
    // drawing changes: the executable keeps the old icon, and one
    // spends a long time looking for why the new logo does not reach
    // the taskbar.
    println!("cargo:rerun-if-changed=../../packaging/brand/zyrdesk.ico");

    let css = std::fs::read_to_string(DESIGN)
        .unwrap_or_else(|e| panic!("le système de design {DESIGN} n'a pas pu être lu : {e}"));
    let written = design(&plain(&css));
    let out = std::path::Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join("design.rs");
    std::fs::write(&out, written).expect("le système de design n'a pas pu être écrit");

    // The Windows resource: the manifest, the icon and what the program
    // says about itself. What comes back is said and not kept quiet: on
    // the other systems it is "nothing to do", and on Windows a refusal
    // would leave a program without its icon and without its modern
    // controls, which takes a long time to track down.
    println!("cargo:rerun-if-changed=zyrdesk.manifest");
    let resource_path =
        std::path::Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join(RESOURCE);
    std::fs::write(&resource_path, resource())
        .expect("la ressource Windows n'a pas pu être écrite");
    let compiled = embed_resource::compile(&resource_path, embed_resource::NONE);
    if let Err(e) = compiled.manifest_optional() {
        println!("cargo:warning=ressource Windows non gravée : {e}");
    }
}

/// What Windows reads in the program before starting it.
const RESOURCE: &str = "zyrdesk.rc";

/// The program's Windows resource, written here because it carries the
/// version, which is the package's and so does not have to be copied
/// out.
///
/// The manifest under number one, which is the one Windows reads in a
/// program. The icon under 32512, which is the number of an application
/// icon: it is under that one that the system looks for it for the
/// taskbar, and under that one that `icon.rs` asks for it again at the
/// exact size it needs. And the name the Task Manager shows, which is
/// the package's description: ZyrDesk runs several programs on one
/// machine, and each one has to say which it is.
fn resource() -> String {
    let folder = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let manifest = format!("{folder}/zyrdesk.manifest").replace('\\', "/");
    let icon = format!("{folder}/../../packaging/brand/zyrdesk.ico").replace('\\', "/");
    let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    let numbers = version
        .split('.')
        .chain(std::iter::repeat("0"))
        .take(4)
        .collect::<Vec<_>>()
        .join(",");
    let what = std::env::var("CARGO_PKG_DESCRIPTION").expect("CARGO_PKG_DESCRIPTION");
    format!(
        r#"#pragma code_page(65001)
1 24 "{manifest}"
32512 ICON "{icon}"

1 VERSIONINFO
FILEVERSION {numbers}
PRODUCTVERSION {numbers}
FILEOS 0x4L
FILETYPE 0x1L
{{
BLOCK "StringFileInfo"
{{
BLOCK "040C04B0"
{{
VALUE "CompanyName", "ZyrDesk"
VALUE "FileDescription", "{what}"
VALUE "FileVersion", "{version}"
VALUE "InternalName", "ZyrDesk"
VALUE "OriginalFilename", "ZyrDesk.exe"
VALUE "ProductName", "ZyrDesk"
VALUE "ProductVersion", "{version}"
}}
}}
BLOCK "VarFileInfo"
{{
VALUE "Translation", 0x40C, 1200
}}
}}
"#
    )
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
        .unwrap_or_else(|| panic!("{selector} est introuvable dans {DESIGN}"));
    let open = css[from..]
        .find('{')
        .unwrap_or_else(|| panic!("{selector} n'ouvre pas d'accolade"))
        + from;
    let close = css[open..]
        .find('}')
        .unwrap_or_else(|| panic!("{selector} ne se referme pas"))
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
                    "« {name} » n'est pas de la même sorte dans les deux thèmes"
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
        "// Écrit par build.rs depuis {DESIGN}. Ne pas modifier à la main :\n\
         // c'est la feuille de style qui décide, et elle seule.\n\
         \n\
         /// Ce qu'un thème dit de chaque rôle.\n\
         #[derive(Clone, Copy)]\n\
         pub struct Palette {{\n{palette}}}\n\
         \n\
         /// Le thème sombre, celui que la feuille de style pose d'abord.\n\
         pub const DARK: Palette = Palette {{\n{dark_fields}}};\n\
         \n\
         /// Le thème clair, celui qu'elle redit ensuite.\n\
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
        return Sort::Length(number.trim().parse().unwrap_or_else(|e| {
            panic!("« {name} » vaut « {value} », qui n'est pas une longueur : {e}")
        }));
    }
    if let Some(number) = value.strip_suffix("ms") {
        return Sort::Time(number.trim().parse().unwrap_or_else(|e| {
            panic!("« {name} » vaut « {value} », qui n'est pas une durée : {e}")
        }));
    }
    panic!(
        "« {name} » vaut « {value} », que le dessin ne sait pas lire. \
         L'ajouter à NOT_DRAWN dans build.rs s'il n'a pas à être dessiné."
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
