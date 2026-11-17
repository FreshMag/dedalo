//! Zipped packages (spec 11.4): a plugin directory zipped with the
//! extension `.dedalopkg`, with `dedalo-plugin.toml` at its root.
//!
//! The reader accepts plain ZIP archives with stored or deflated entries
//! and checks every size and checksum. The writer is deterministic: the
//! same files always give the same bytes.

use std::collections::BTreeMap;

use crate::{MANIFEST, MAX_PACKAGE};

/// The file extension of a zipped package.
pub const EXTENSION: &str = "dedalopkg";
/// Most files in one package.
const MAX_FILES: usize = 2000;

const LOCAL: u32 = 0x0403_4b50;
const CENTRAL: u32 = 0x0201_4b50;
const END: u32 = 0x0605_4b50;

fn u16_at(bytes: &[u8], at: usize) -> Option<usize> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?) as usize)
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// A path inside a package: relative, with forward slashes, never leaving
/// the package.
fn safe_path(name: &str) -> Result<String, String> {
    let bad = name.is_empty()
        || name.starts_with('/')
        || name.contains('\\')
        || name.contains(':')
        || name.contains('\0')
        || name
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..");
    if bad {
        return Err(format!("the archive holds the unsafe path `{name}`"));
    }
    Ok(name.to_string())
}

/// Whether bytes look like a ZIP archive.
pub fn is_archive(bytes: &[u8]) -> bool {
    u32_at(bytes, 0) == Some(LOCAL)
}

/// The files of a zipped package, by path.
pub fn unpack(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let cut = || "the archive is cut short or damaged".to_string();
    // The end record sits at the end, after an optional comment.
    let earliest = bytes.len().saturating_sub(22 + 65_535);
    let end = (earliest..=bytes.len().saturating_sub(22))
        .rev()
        .find(|at| u32_at(bytes, *at) == Some(END))
        .ok_or("this is not a ZIP archive: it has no end record")?;
    let count = u16_at(bytes, end + 10).ok_or_else(cut)?;
    let directory = u32_at(bytes, end + 16).ok_or_else(cut)? as usize;
    if count == 0xffff || directory == 0xffff_ffff {
        return Err("ZIP64 archives are not supported; a package is at most 32 MB".to_string());
    }
    if count > MAX_FILES {
        return Err(format!(
            "the archive holds {count} files; the limit is {MAX_FILES}"
        ));
    }
    let mut files = BTreeMap::new();
    let mut total = 0usize;
    let mut at = directory;
    for _ in 0..count {
        if u32_at(bytes, at) != Some(CENTRAL) {
            return Err(cut());
        }
        let flags = u16_at(bytes, at + 8).ok_or_else(cut)?;
        let method = u16_at(bytes, at + 10).ok_or_else(cut)?;
        let crc = u32_at(bytes, at + 16).ok_or_else(cut)?;
        let packed = u32_at(bytes, at + 20).ok_or_else(cut)? as usize;
        let size = u32_at(bytes, at + 24).ok_or_else(cut)? as usize;
        let name_len = u16_at(bytes, at + 28).ok_or_else(cut)?;
        let extra_len = u16_at(bytes, at + 30).ok_or_else(cut)?;
        let comment_len = u16_at(bytes, at + 32).ok_or_else(cut)?;
        let local = u32_at(bytes, at + 42).ok_or_else(cut)? as usize;
        let name = bytes.get(at + 46..at + 46 + name_len).ok_or_else(cut)?;
        let name = std::str::from_utf8(name)
            .map_err(|_| "the archive holds a file name that is not UTF-8".to_string())?;
        at += 46 + name_len + extra_len + comment_len;
        if name.ends_with('/') {
            continue;
        }
        let path = safe_path(name)?;
        if flags & 1 != 0 {
            return Err(format!(
                "`{path}` is encrypted; packages are plain archives"
            ));
        }
        total += size;
        if total > MAX_PACKAGE {
            return Err(format!(
                "the package is larger than {} MB",
                MAX_PACKAGE / 1024 / 1024
            ));
        }
        if u32_at(bytes, local) != Some(LOCAL) {
            return Err(cut());
        }
        let start = local
            + 30
            + u16_at(bytes, local + 26).ok_or_else(cut)?
            + u16_at(bytes, local + 28).ok_or_else(cut)?;
        let data = bytes.get(start..start + packed).ok_or_else(cut)?;
        let content = match method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, size)
                .map_err(|_| format!("`{path}` does not decompress to its stated size"))?,
            other => {
                return Err(format!(
                    "`{path}` uses compression method {other}; only stored and deflated files are read"
                ));
            }
        };
        if content.len() != size || crc32fast::hash(&content) != crc {
            return Err(format!(
                "`{path}` fails its checksum: the archive is damaged"
            ));
        }
        if files.insert(path.clone(), content).is_some() {
            return Err(format!("the archive holds `{path}` twice"));
        }
    }
    // An archive of a folder, instead of its contents, is unwrapped.
    if !files.contains_key(MANIFEST) {
        let mut tops = files
            .keys()
            .map(|p| p.split('/').next().unwrap_or_default());
        let first = tops.next().unwrap_or_default().to_string();
        let prefix = format!("{first}/");
        if !first.is_empty()
            && tops.all(|t| t == first)
            && files.contains_key(&format!("{prefix}{MANIFEST}"))
        {
            files = files
                .into_iter()
                .map(|(path, bytes)| (path[prefix.len()..].to_string(), bytes))
                .collect();
        } else {
            return Err(format!("the package has no `{MANIFEST}` at its root"));
        }
    }
    Ok(files)
}

/// Zips the files of a package.
pub fn pack(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, String> {
    if !files.contains_key(MANIFEST) {
        return Err(format!("the package has no `{MANIFEST}` at its root"));
    }
    zip(files)
}

fn zip(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = Vec::new();
    let mut directory: Vec<u8> = Vec::new();
    for (path, content) in files {
        let path = safe_path(path)?;
        let deflated = miniz_oxide::deflate::compress_to_vec(content, 9);
        let (method, data): (u16, &[u8]) = if deflated.len() < content.len() {
            (8, &deflated)
        } else {
            (0, content)
        };
        let too_big = || format!("`{path}` is too large for a package");
        let offset = u32::try_from(out.len()).map_err(|_| too_big())?;
        let packed = u32::try_from(data.len()).map_err(|_| too_big())?;
        let size = u32::try_from(content.len()).map_err(|_| too_big())?;
        let name_len = u16::try_from(path.len()).map_err(|_| too_big())?;
        // Shared by the local and the central header: version 2.0, UTF-8
        // names, and a fixed date so that packing is reproducible.
        let mut common: Vec<u8> = Vec::new();
        common.extend(20u16.to_le_bytes());
        common.extend(0x0800u16.to_le_bytes());
        common.extend(method.to_le_bytes());
        common.extend(0u16.to_le_bytes());
        common.extend(0x0021u16.to_le_bytes());
        common.extend(crc32fast::hash(content).to_le_bytes());
        common.extend(packed.to_le_bytes());
        common.extend(size.to_le_bytes());
        common.extend(name_len.to_le_bytes());
        common.extend(0u16.to_le_bytes());

        out.extend(LOCAL.to_le_bytes());
        out.extend(&common);
        out.extend(path.as_bytes());
        out.extend(data);

        directory.extend(CENTRAL.to_le_bytes());
        directory.extend(20u16.to_le_bytes());
        directory.extend(&common);
        // Comment length, disk, internal and external attributes.
        directory.extend([0u8; 10]);
        directory.extend(offset.to_le_bytes());
        directory.extend(path.as_bytes());
    }
    let count = u16::try_from(files.len())
        .ok()
        .filter(|n| (*n as usize) <= MAX_FILES)
        .ok_or_else(|| format!("a package holds at most {MAX_FILES} files"))?;
    let start = u32::try_from(out.len()).map_err(|_| "the package is too large".to_string())?;
    let length = directory.len() as u32;
    out.extend(directory);
    out.extend(END.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend(count.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(length.to_le_bytes());
    out.extend(start.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            (MANIFEST.to_string(), b"[plugin]\nid = \"x\"\n".to_vec()),
            ("themes/a.toml".to_string(), vec![b'a'; 5000]),
            ("empty".to_string(), Vec::new()),
            ("noise.bin".to_string(), (0..=255u8).collect()),
        ])
    }

    #[test]
    fn packing_round_trips_and_is_reproducible() {
        let files = sample();
        let archive = pack(&files).unwrap();
        assert!(is_archive(&archive));
        assert_eq!(archive, pack(&files).unwrap());
        assert_eq!(unpack(&archive).unwrap(), files);
        // The repeated bytes were deflated.
        assert!(archive.len() < 2000, "{}", archive.len());
    }

    #[test]
    fn damaged_and_hostile_archives_are_refused() {
        let archive = pack(&sample()).unwrap();
        assert!(unpack(b"not a zip").is_err());
        assert!(unpack(&archive[..archive.len() - 5]).is_err());
        // Flipping a byte of file data breaks a checksum.
        let mut flipped = archive.clone();
        let at = flipped.len() / 3;
        flipped[at] ^= 0x55;
        assert!(unpack(&flipped).is_err());
        for name in ["../escape", "/etc/passwd", "a/../../b", "c:\\x", "a//b"] {
            assert!(safe_path(name).is_err(), "{name}");
        }
        let no_manifest = BTreeMap::from([("a.txt".to_string(), vec![1u8])]);
        assert!(pack(&no_manifest).is_err());
    }

    #[test]
    fn an_archive_of_the_folder_itself_is_unwrapped() {
        let files = sample();
        let wrapped: BTreeMap<String, Vec<u8>> = files
            .iter()
            .map(|(path, bytes)| (format!("my_plugin/{path}"), bytes.clone()))
            .collect();
        assert_eq!(unpack(&zip(&wrapped).unwrap()).unwrap(), files);
        // Two folders at the top are not a package.
        let mut two = wrapped;
        two.insert("other/file".to_string(), vec![1]);
        assert!(unpack(&zip(&two).unwrap()).is_err());
    }
}
