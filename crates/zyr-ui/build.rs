fn main() {
    // The program's icon is a Windows resource made here, from the
    // brand's own file. Without this line, Cargo does not redo that work
    // when the drawing changes: the executable keeps the old icon, and
    // one spends a long time looking for why the new logo does not reach
    // the taskbar.
    println!("cargo:rerun-if-changed=../../packaging/brand/zyrdesk.ico");

    // The Windows resource: the manifest, the icon and what the program
    // says about itself. What comes back is said and not kept quiet: on
    // the other systems it is "nothing to do", and on Windows a refusal
    // would leave a program without its icon and without its modern
    // controls, which takes a long time to track down.
    println!("cargo:rerun-if-changed=zyrdesk.manifest");
    let resource_path =
        std::path::Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR")).join(RESOURCE);
    std::fs::write(&resource_path, resource()).expect("the Windows resource is written");
    let compiled = embed_resource::compile(&resource_path, embed_resource::NONE);
    if let Err(e) = compiled.manifest_optional() {
        println!("cargo:warning=Windows resource not embedded: {e}");
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
BLOCK "040904B0"
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
VALUE "Translation", 0x409, 1200
}}
}}
"#
    )
}
