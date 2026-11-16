//! Gives the Windows executable its icon.
//!
//! The window's icon is set at run time, but a pinned taskbar button, a shortcut and
//! Explorer show the icon in the executable's resources. For a Windows MSVC target this
//! writes `packaging/guitarscore.ico` as a resource file (`src/windows_icon.rs`) and links
//! it into the `guitarscore` binary. Other targets need nothing.

#[path = "src/windows_icon.rs"]
mod windows_icon;

const ICON: &str = "../../packaging/guitarscore.ico";

fn main() -> Result<(), String> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/windows_icon.rs");
    println!("cargo:rerun-if-changed={ICON}");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return Ok(());
    }
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_env != "msvc" {
        // GNU ld wants a COFF object from windres instead; releases build with MSVC.
        println!("cargo:warning=no executable icon for the {target_env} target (MSVC only)");
        return Ok(());
    }
    let ico = std::fs::read(ICON).map_err(|e| format!("cannot read {ICON}: {e}"))?;
    let res = windows_icon::icon_res(&ico).map_err(|e| format!("{ICON}: {e}"))?;
    let out = std::env::var("OUT_DIR").map_err(|e| format!("OUT_DIR: {e}"))?;
    let path = std::path::Path::new(&out).join("guitarscore.res");
    std::fs::write(&path, res).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    // link.exe and lld-link take a .res file as an input like an object file.
    println!("cargo:rustc-link-arg-bin=guitarscore={}", path.display());
    Ok(())
}
