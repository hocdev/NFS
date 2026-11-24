use crate::squash::native::{Node, Squash};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
type Result<T> = std::result::Result<T, String>;
pub const CATEGORIES: [&str; 12] = [
    "LC_CTYPE",
    "LC_NUMERIC",
    "LC_TIME",
    "LC_COLLATE",
    "LC_MONETARY",
    "LC_MESSAGES/SYS_LC_MESSAGES",
    "LC_PAPER",
    "LC_NAME",
    "LC_ADDRESS",
    "LC_TELEPHONE",
    "LC_MEASUREMENT",
    "LC_IDENTIFICATION",
];
fn number(b: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        b.get(offset..offset + 4)?.try_into().ok()?,
    ))
}
pub fn archived_locales(bytes: &[u8]) -> Vec<String> {
    if number(bytes, 0) != Some(0xde020109) {
        return Vec::new();
    }
    let Some(start) = number(bytes, 8) else {
        return Vec::new();
    };
    let Some(count) = number(bytes, 16) else {
        return Vec::new();
    };
    if count > 100000 || start as usize + count as usize * 12 > bytes.len() {
        return Vec::new();
    }
    let mut names = BTreeSet::new();
    for p in (start as usize..start as usize + count as usize * 12).step_by(12) {
        if number(bytes, p + 8) == Some(0) {
            continue;
        }
        if let Some(offset) = number(bytes, p + 4) {
            if let Some(tail) = bytes.get(offset as usize..) {
                if let Some(n) = tail.iter().position(|&b| b == 0) {
                    if let Ok(name) = std::str::from_utf8(&tail[..n]) {
                        let name = name.replace(".utf8", ".UTF-8");
                        if crate::language::validate_locale(&name).is_ok() {
                            names.insert(name);
                        }
                    }
                }
            }
        }
    }
    names.into_iter().collect()
}
pub fn glibc_abi(version: &str) -> Option<String> {
    let version = version.split(':').next_back()?;
    let mut parts = version.split('.');
    let a = parts.next()?;
    let b: String = parts
        .next()?
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if !a.is_empty()
        && !b.is_empty()
        && a.bytes().all(|b| b.is_ascii_digit())
        && b.bytes().all(|b| b.is_ascii_digit())
    {
        Some(format!("{a}.{b}"))
    } else {
        None
    }
}
pub fn catalog(abi: &str, architecture: &str) -> BTreeMap<String, (String, String)> {
    crate::locale_registry::PACKS
        .iter()
        .filter(|pack| {
            pack.abi == abi && (pack.architecture == architecture || pack.architecture == "all")
        })
        .flat_map(|pack| pack.catalog.lines())
        .filter_map(|line| {
            let mut p = line.split('\t');
            Some((
                p.next()?.to_owned(),
                (p.next()?.to_owned(), p.next()?.to_owned()),
            ))
        })
        .collect()
}
fn safe_path(path: &str) -> Result<String> {
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err("Payload escapes filesystem root".into());
                }
            }
            _ => {
                if part.contains(['\\', '\0', ':']) {
                    return Err("Unsafe language payload path".into());
                }
                parts.push(part);
            }
        }
    }
    Ok(parts.join("/"))
}
fn octal(field: &[u8]) -> Result<usize> {
    let s = std::str::from_utf8(field)
        .map_err(|e| e.to_string())?
        .trim_matches(['\0', ' ']);
    if s.is_empty() {
        Ok(0)
    } else {
        usize::from_str_radix(s, 8).map_err(|e| e.to_string())
    }
}
fn cstring(field: &[u8]) -> Result<String> {
    let n = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    String::from_utf8(field[..n].to_vec()).map_err(|e| e.to_string())
}
/// Decode only data, never execute maintainer scripts or install a package.
pub fn tar_payload(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    tar_filtered(bytes, None)
}
fn tar_filtered(
    bytes: &[u8],
    folders: Option<&BTreeSet<String>>,
) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut entries: BTreeMap<String, (u8, &[u8], String)> = BTreeMap::new();
    let mut pos = 0;
    let mut long_name = None;
    while pos + 512 <= bytes.len() {
        let h = &bytes[pos..pos + 512];
        if h.iter().all(|&b| b == 0) {
            break;
        }
        let expected = octal(&h[148..156])?;
        let actual: usize = h
            .iter()
            .enumerate()
            .map(|(i, &b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    b as usize
                }
            })
            .sum();
        if expected != actual {
            return Err("Invalid language TAR checksum".into());
        }
        let size = octal(&h[124..136])?;
        let start = pos + 512;
        let data = bytes
            .get(start..start.checked_add(size).ok_or("TAR size overflow")?)
            .ok_or("Truncated language TAR")?;
        pos = start + size.div_ceil(512) * 512;
        if h[156] == b'L' {
            long_name = Some(cstring(data)?);
            continue;
        }
        if matches!(h[156], b'x' | b'g') {
            return Err("PAX language archives are unsupported; use USTAR/XZ payload".into());
        }
        let name = if let Some(name) = long_name.take() {
            name
        } else {
            let base = cstring(&h[..100])?;
            let prefix = cstring(&h[345..500])?;
            if prefix.is_empty() {
                base
            } else {
                format!("{prefix}/{base}")
            }
        };
        let name = safe_path(&name)?;
        if matches!(h[156], 0 | b'0' | b'1' | b'2') {
            entries.insert(name, (h[156], data, cstring(&h[157..257])?));
        }
    }
    fn resolve<'a>(
        name: &str,
        entries: &BTreeMap<String, (u8, &'a [u8], String)>,
        depth: usize,
    ) -> Result<&'a [u8]> {
        if depth > 16 {
            return Err("Language payload link cycle".into());
        }
        let (kind, data, link) = entries
            .get(name)
            .ok_or_else(|| format!("Missing language link target: {name}"))?;
        if matches!(*kind, 0 | b'0') {
            return Ok(data);
        }
        let target = if *kind == b'2' {
            let parent = name.rsplit_once('/').map(|p| p.0).unwrap_or("");
            safe_path(&format!("{parent}/{link}"))?
        } else {
            safe_path(link)?
        };
        resolve(&target, entries, depth + 1)
    }
    let mut result = BTreeMap::new();
    for name in entries.keys() {
        if allowed(name)
            && folders.is_none_or(|f| {
                f.iter()
                    .any(|folder| name.starts_with(&format!("usr/lib/locale/{folder}/")))
            })
        {
            result.insert(name.clone(), resolve(name, &entries, 0)?.to_vec());
        }
    }
    Ok(result)
}
fn allowed(path: &str) -> bool {
    path.starts_with("usr/lib/locale/")
        || path.starts_with("usr/share/locale/")
        || path.starts_with("usr/share/fonts/")
        || (path.starts_with("usr/share/")
            && path.contains("/translations/")
            && path.ends_with(".qm"))
}
pub fn deb_payload(path: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    deb_bytes(&bytes, None)
}
fn deb_bytes(
    bytes: &[u8],
    folders: Option<&BTreeSet<String>>,
) -> Result<BTreeMap<String, Vec<u8>>> {
    if !bytes.starts_with(b"!<arch>\n") {
        return Err("Invalid language .deb".into());
    }
    let mut pos = 8;
    while pos + 60 <= bytes.len() {
        let h = &bytes[pos..pos + 60];
        if &h[58..60] != b"`\n" {
            return Err("Invalid Debian member header".into());
        }
        let size = std::str::from_utf8(&h[48..58])
            .map_err(|e| e.to_string())?
            .trim()
            .parse::<usize>()
            .map_err(|e| e.to_string())?;
        let name = std::str::from_utf8(&h[..16])
            .map_err(|e| e.to_string())?
            .trim()
            .trim_end_matches('/');
        let data = bytes
            .get(pos + 60..pos + 60 + size)
            .ok_or("Truncated Debian member")?;
        if name == "data.tar.xz" {
            return tar_filtered(&crate::xz::decode(data, 512 * 1024 * 1024)?, folders);
        }
        if name == "data.tar" {
            return tar_filtered(data, folders);
        }
        if name.starts_with("data.tar.") {
            return Err("Gói dịch cần data.tar.xz hoặc data.tar; không cài gói lúc boot.".into());
        }
        pos += 60 + size + size % 2;
    }
    Err("Language package has no data archive".into())
}
pub fn integrate(
    fs: &mut Squash,
    paths: &BTreeMap<String, Node>,
    abi: &str,
    architecture: &str,
    locales: &BTreeSet<String>,
    packages: &[PathBuf],
    added: &mut BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    for package in packages {
        for (path, data) in deb_payload(package)? {
            added.insert(path, data);
        }
    }
    let archived = fs
        .read("usr/lib/locale/locale-archive")
        .ok()
        .map(|b| archived_locales(&b))
        .unwrap_or_default();
    let missing: BTreeSet<String> = locales
        .iter()
        .filter(|locale| {
            !archived.contains(locale)
                && !CATEGORIES.iter().all(|cat| {
                    paths.contains_key(&format!(
                        "usr/lib/locale/{}/{}",
                        locale.replace(".UTF-8", ".utf8"),
                        cat
                    ))
                })
        })
        .cloned()
        .collect();
    let index = catalog(abi, architecture);
    let mut unresolved = missing.clone();
    for pack in crate::locale_registry::PACKS
        .iter()
        .filter(|p| p.abi == abi && (p.architecture == architecture || p.architecture == "all"))
    {
        let entries: BTreeMap<_, _> = pack
            .catalog
            .lines()
            .filter_map(|line| {
                let mut p = line.split('\t');
                Some((p.next()?.to_owned(), p.next()?.to_owned()))
            })
            .collect();
        let folders: BTreeSet<_> = unresolved
            .iter()
            .filter_map(|code| entries.get(code).cloned())
            .collect();
        if folders.is_empty() {
            continue;
        }
        for (path, data) in deb_bytes(pack.data, Some(&folders))? {
            let Some(rest) = path.strip_prefix("usr/lib/locale/") else {
                continue;
            };
            let Some((folder, category)) = rest.split_once('/') else {
                continue;
            };
            if let Some((locale, _)) = index.iter().find(|(_, p)| p.0 == folder) {
                added
                    .entry(format!(
                        "usr/lib/locale/{}/{category}",
                        locale.replace(".UTF-8", ".utf8")
                    ))
                    .or_insert(data);
            }
        }
        unresolved.retain(|code| !entries.contains_key(code));
    }
    for locale in &missing {
        let normalized = locale.replace(".UTF-8", ".utf8");
        let base = format!("usr/lib/locale/{normalized}/");
        if !archived.contains(locale) {
            for category in CATEGORIES {
                let path = format!("{base}{category}");
                let data=added.get(&path).cloned().or_else(||paths.get(&path).and_then(|n|fs.read_node(n).ok())).ok_or_else(||format!("Thiếu locale đã biên dịch {locale}/{category}. Cần payload đúng glibc {abi}; không tạo hoặc cài locale khi boot."))?;
                let reference = fs.read(&format!("usr/lib/locale/C.UTF-8/{category}")).ok();
                if data.len() < 8
                    || reference
                        .as_ref()
                        .is_some_and(|r| r.get(..4) != data.get(..4))
                {
                    return Err(format!(
                        "Locale không cùng định dạng glibc: {locale}/{category}"
                    ));
                }
            }
        }
    }
    if !locales.is_empty() {
        added.insert(
            "usr/share/doc/isolite-language-data/COPYRIGHT".into(),
            include_bytes!("../pkg/languages/COPYRIGHT.txt").to_vec(),
        );
        added.insert(
            "usr/share/doc/isolite-language-data/SOURCE".into(),
            include_bytes!("../pkg/languages/SOURCE.txt").to_vec(),
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_uses_package_metadata() {
        assert!(!crate::locale_registry::PACKS.is_empty());
        for pack in crate::locale_registry::PACKS {
            assert!(!catalog(pack.abi, pack.architecture).is_empty());
            assert!(catalog("unsupported", pack.architecture).is_empty());
        }
    }
    #[test]
    fn full_catalog_extracts_only_requested_languages() {
        let pack = &crate::locale_registry::PACKS[0];
        let index = catalog(pack.abi, pack.architecture);
        let folders: BTreeSet<_> = index.values().take(2).map(|p| p.0.clone()).collect();
        let files = deb_bytes(pack.data, Some(&folders)).unwrap();
        assert_eq!(files.len(), folders.len() * CATEGORIES.len());
        for folder in folders {
            for category in CATEGORIES {
                assert!(files
                    .get(&format!("usr/lib/locale/{folder}/{category}"))
                    .is_some_and(|v| v.len() > 8));
            }
        }
    }
    #[test]
    fn rejects_escape_and_parses_version() {
        assert!(safe_path("../../etc/passwd").is_err());
        assert_eq!(glibc_abi("2.31-13+deb11u3+m11+1nfs5"), Some("2.31".into()));
    }
}
