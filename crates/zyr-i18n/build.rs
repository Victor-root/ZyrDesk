//! Lists every language the product speaks.
//!
//! One file per language in `words/`, named by its code: adding a
//! language is adding a file there, and nothing in the code names one.

use std::fmt::Write;
use std::path::Path;

fn main() {
    let root = std::env::var("CARGO_MANIFEST_DIR").expect("cargo names the brick's folder");
    let folder = Path::new(&root).join("words");
    // A folder watched is a folder scanned: a language added is seen.
    println!("cargo::rerun-if-changed={}", folder.display());

    let mut codes: Vec<String> = std::fs::read_dir(&folder)
        .unwrap_or_else(|e| panic!("{}: {e}", folder.display()))
        .map(|entry| entry.expect("the words folder lists its files").path())
        .filter(|path| path.extension().is_some_and(|ending| ending == "txt"))
        .map(|path| {
            let code = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or_default()
                .to_string();
            assert!(
                !code.is_empty() && code.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{}: a language's file is named by its code, such as fr or pt-br",
                path.display()
            );
            code
        })
        .collect();
    // English first: it is the base the others graft onto.
    codes.sort_by_key(|code| (code != "en", code.clone()));
    assert_eq!(
        codes.first().map(String::as_str),
        Some("en"),
        "English is the base every other language grafts onto: words/en.txt"
    );

    let mut listed = String::from("const LANGUAGES: &[(&str, &str)] = &[\n");
    for code in &codes {
        writeln!(
            listed,
            "    (\"{code}\", include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/words/{code}.txt\"))),"
        )
        .expect("writing to a string");
    }
    listed.push_str("];\n");

    let out = std::env::var("OUT_DIR").expect("cargo names the build's folder");
    let path = Path::new(&out).join("languages.rs");
    std::fs::write(&path, listed).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}
