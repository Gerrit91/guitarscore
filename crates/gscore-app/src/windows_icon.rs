//! The app icon as a Windows resource file (`.res`), for the build script to link into
//! `guitarscore.exe`.
//!
//! Windows takes the icon of a pinned taskbar button, a shortcut and Explorer from the
//! executable's resources, not from the running window: without them a pinned GuitarScore
//! shows the generic program icon. The MSVC linker (and `lld-link`) take a `.res` file as
//! an input, so no resource compiler is needed: this writes one from the `.ico` in
//! `packaging/`. Shared with the unit tests through `#[path]`, like `gscore-version`'s
//! `choose.rs`.

/// `RT_ICON`: one image of an icon.
const RT_ICON: u16 = 3;
/// `RT_GROUP_ICON`: the directory of an icon's images.
const RT_GROUP_ICON: u16 = 14;
/// US English, what the Windows resource compiler uses by default.
const LANGUAGE: u16 = 0x0409;
/// `MOVEABLE | DISCARDABLE`, as the resource compiler marks icon images.
const IMAGE_FLAGS: u16 = 0x1010;
/// `MOVEABLE | PURE | DISCARDABLE`, as it marks icon groups.
const GROUP_FLAGS: u16 = 0x1030;
/// The icon group's id; Explorer shows the executable's first group.
const GROUP_ID: u16 = 1;

/// One image of an `.ico` file: its directory entry (without the offset) and its data.
struct Image<'a> {
    /// Width, height, colour count, reserved, planes and bit count, as in the file.
    width: u8,
    height: u8,
    colors: u8,
    planes: u16,
    bits: u16,
    data: &'a [u8],
}

/// The `.res` file holding `ico` as the executable's icon (group 1, images 1, 2, ...).
pub(crate) fn icon_res(ico: &[u8]) -> Result<Vec<u8>, String> {
    let images = read_ico(ico)?;
    let mut res = Vec::new();
    // A 32-bit resource file starts with an empty entry.
    push_entry(&mut res, 0, 0, 0, 0, &[]);
    let mut group = Vec::new();
    push_u16(&mut group, 0);
    push_u16(&mut group, 1);
    push_u16(&mut group, len_u16(images.len())?);
    for (index, image) in images.iter().enumerate() {
        let id = len_u16(index + 1)?;
        push_entry(&mut res, RT_ICON, id, IMAGE_FLAGS, LANGUAGE, image.data);
        group.extend_from_slice(&[image.width, image.height, image.colors, 0]);
        push_u16(&mut group, image.planes);
        push_u16(&mut group, image.bits);
        push_u32(&mut group, len_u32(image.data.len())?);
        push_u16(&mut group, id);
    }
    push_entry(
        &mut res,
        RT_GROUP_ICON,
        GROUP_ID,
        GROUP_FLAGS,
        LANGUAGE,
        &group,
    );
    Ok(res)
}

/// The images of an `.ico` file, checked to lie inside it.
fn read_ico(ico: &[u8]) -> Result<Vec<Image<'_>>, String> {
    let header = ico.get(..6).ok_or("the icon file is too short")?;
    if u16_at(header, 0) != 0 || u16_at(header, 2) != 1 {
        return Err("not an .ico file".to_owned());
    }
    let count = usize::from(u16_at(header, 4));
    if count == 0 {
        return Err("the icon file has no images".to_owned());
    }
    (0..count)
        .map(|index| {
            let start = 6 + 16 * index;
            let entry = ico
                .get(start..start + 16)
                .ok_or("the icon file's directory is cut off")?;
            let size = usize::try_from(u32_at(entry, 8)).map_err(|e| e.to_string())?;
            let offset = usize::try_from(u32_at(entry, 12)).map_err(|e| e.to_string())?;
            let data = offset
                .checked_add(size)
                .and_then(|end| ico.get(offset..end))
                .filter(|data| !data.is_empty())
                .ok_or_else(|| format!("image {} of the icon file is cut off", index + 1))?;
            Ok(Image {
                width: byte_at(entry, 0),
                height: byte_at(entry, 1),
                colors: byte_at(entry, 2),
                planes: u16_at(entry, 4),
                bits: u16_at(entry, 6),
                data,
            })
        })
        .collect()
}

/// Appends one resource with a numbered type and name, its data padded to four bytes.
fn push_entry(res: &mut Vec<u8>, kind: u16, id: u16, flags: u16, language: u16, data: &[u8]) {
    /// Sizes, type, name, data version, flags, language, version and characteristics.
    const HEADER_SIZE: u32 = 32;
    // Resource data is far below 4 GiB (the icon file is read whole).
    push_u32(res, u32::try_from(data.len()).unwrap_or(u32::MAX));
    push_u32(res, HEADER_SIZE);
    push_u16(res, 0xFFFF);
    push_u16(res, kind);
    push_u16(res, 0xFFFF);
    push_u16(res, id);
    push_u32(res, 0);
    push_u16(res, flags);
    push_u16(res, language);
    push_u32(res, 0);
    push_u32(res, 0);
    res.extend_from_slice(data);
    while !res.len().is_multiple_of(4) {
        res.push(0);
    }
}

fn len_u16(len: usize) -> Result<u16, String> {
    u16::try_from(len).map_err(|_| "the icon file has too many images".to_owned())
}

fn len_u32(len: usize) -> Result<u32, String> {
    u32::try_from(len).map_err(|_| "an icon image is too large".to_owned())
}

fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// The byte at `at`; callers checked the length (0 past the end).
fn byte_at(bytes: &[u8], at: usize) -> u8 {
    bytes.get(at).copied().unwrap_or(0)
}

/// The little-endian `u16` at `at`; callers checked the length (0 past the end).
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([byte_at(bytes, at), byte_at(bytes, at + 1)])
}

/// The little-endian `u32` at `at`; callers checked the length (0 past the end).
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from(u16_at(bytes, at)) | (u32::from(u16_at(bytes, at + 2)) << 16)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    const ICO: &[u8] = include_bytes!("../../../packaging/guitarscore.ico");

    /// The resources in `res`: (type, id, data).
    fn entries(res: &[u8]) -> Vec<(u16, u16, &[u8])> {
        let mut found = Vec::new();
        let mut at = 0;
        while at < res.len() {
            let size = u32_at(res, at) as usize;
            let header = u32_at(res, at + 4) as usize;
            assert_eq!(header, 32);
            assert_eq!(u16_at(res, at + 8), 0xFFFF);
            assert_eq!(u16_at(res, at + 12), 0xFFFF);
            let data = &res[at + header..at + header + size];
            found.push((u16_at(res, at + 10), u16_at(res, at + 14), data));
            at += (header + size).div_ceil(4) * 4;
        }
        found
    }

    #[test]
    fn the_app_icon_becomes_an_icon_group_with_every_image() {
        let images = read_ico(ICO).unwrap();
        // Every size Windows asks for, up to the 256 px one (stored as PNG).
        let sizes: Vec<u8> = images.iter().map(|image| image.width).collect();
        assert_eq!(sizes, [16, 20, 24, 32, 40, 48, 64, 0]);
        assert!(images.last().unwrap().data.starts_with(b"\x89PNG"));

        let res = icon_res(ICO).unwrap();
        let found = entries(&res);
        assert_eq!(found.len(), images.len() + 2);
        assert_eq!(found[0], (0, 0, &[][..]));
        for (index, image) in images.iter().enumerate() {
            assert_eq!(found[index + 1], (RT_ICON, index as u16 + 1, image.data));
        }
        let (kind, id, group) = *found.last().unwrap();
        assert_eq!((kind, id), (RT_GROUP_ICON, GROUP_ID));
        assert_eq!(group.len(), 6 + 14 * images.len());
        assert_eq!(usize::from(u16_at(group, 4)), images.len());
        for (index, image) in images.iter().enumerate() {
            let entry = &group[6 + 14 * index..6 + 14 * (index + 1)];
            assert_eq!(entry[0], image.width);
            assert_eq!(u32_at(entry, 8) as usize, image.data.len());
            assert_eq!(usize::from(u16_at(entry, 12)), index + 1);
        }
    }

    #[test]
    fn a_broken_icon_file_is_refused() {
        assert!(icon_res(b"").is_err());
        assert!(icon_res(b"\x89PNG\r\n\x1a\n").is_err());
        assert!(icon_res(&[0, 0, 1, 0, 0, 0]).is_err());
        // An image that runs past the end of the file.
        let mut cut = ICO[..ICO.len() - 1].to_vec();
        assert!(icon_res(&cut).is_err());
        cut.truncate(20);
        assert!(icon_res(&cut).is_err());
    }
}
