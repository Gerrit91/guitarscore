//! The app icon that `main.rs` sets on the window and the release packages ship.

#![allow(clippy::expect_used, unused_crate_dependencies)]

const APP_ICON: &[u8] = include_bytes!("../../../packaging/guitarscore.png");
const APP_ICON_SVG: &str = include_str!("../../../packaging/guitarscore.svg");

#[test]
fn the_app_icon_is_a_square_256_pixel_png() {
    let icon = eframe::icon_data::from_png_bytes(APP_ICON).expect("the icon is a valid PNG");
    assert_eq!((icon.width, icon.height), (256, 256));
    assert_eq!(icon.rgba.len(), 256 * 256 * 4);
    // Transparent outside the rounded tile, opaque in its middle.
    let alpha = |x: usize, y: usize| icon.rgba[(y * 256 + x) * 4 + 3];
    assert_eq!(alpha(0, 0), 0);
    assert_eq!(alpha(128, 128), 255);
}

#[test]
fn the_svg_source_has_the_same_size() {
    assert!(APP_ICON_SVG.contains(r#"viewBox="0 0 256 256""#));
}
