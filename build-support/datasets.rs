use std::{collections::BTreeMap, path::Path};
fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b)
        .trim_matches(['\0', ' '])
        .to_owned()
}
fn tar_entries(b: &[u8]) -> Vec<(String, &[u8], u8, String)> {
    let mut entries = Vec::new();
    let mut p = 0;
    while p + 512 <= b.len() {
        let h = &b[p..p + 512];
        if h.iter().all(|&c| c == 0) {
            break;
        }
        let n = usize::from_str_radix(&text(&h[124..136]), 8).expect("TAR size");
        let start = p + 512;
        let data = b.get(start..start + n).expect("TAR bounds");
        let base = text(&h[..100]);
        let prefix = text(&h[345..500]);
        let name = if prefix.is_empty() {
            base
        } else {
            format!("{prefix}/{base}")
        };
        entries.push((
            name.trim_start_matches("./").into(),
            data,
            h[156],
            text(&h[157..257]),
        ));
        p = start + n.div_ceil(512) * 512;
    }
    entries
}
fn member<'a>(bytes: &'a [u8], prefix: &str) -> (String, &'a [u8]) {
    assert!(bytes.starts_with(b"!<arch>\n"));
    let mut p = 8;
    while p + 60 <= bytes.len() {
        let h = &bytes[p..p + 60];
        let n = text(&h[48..58]).parse::<usize>().expect("ar size");
        let name = text(&h[..16]).trim_end_matches('/').to_owned();
        let data = bytes.get(p + 60..p + 60 + n).expect("ar bounds");
        if name.starts_with(prefix) {
            return (name, data);
        }
        p += 60 + n + n % 2;
    }
    panic!("Missing archive member {prefix}")
}
fn unpack(name: &str, bytes: &[u8]) -> Vec<u8> {
    if name.ends_with(".xz") {
        crate::xz::decode(bytes, 512 * 1024 * 1024).expect("XZ data")
    } else if name.ends_with(".tar") {
        bytes.to_vec()
    } else {
        panic!("Dataset requires tar.xz or uncompressed tar")
    }
}
fn abi(version: &str) -> String {
    let mut p = version.rsplit(':').next().unwrap_or(version).split('.');
    let a = p.next().unwrap_or("");
    let b: String = p
        .next()
        .unwrap_or("")
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    assert!(
        !a.is_empty() && !b.is_empty() && a.bytes().all(|b| b.is_ascii_digit()),
        "Cannot derive locale ABI from version metadata"
    );
    format!("{a}.{b}")
}
fn derive_catalog(bytes: &[u8]) -> String {
    let (name, data) = member(bytes, "data.tar");
    let tar = unpack(&name, data);
    let mut catalog = BTreeMap::new();
    let entries: BTreeMap<_, _> = tar_entries(&tar)
        .into_iter()
        .map(|(n, d, k, l)| (n, (d, k, l)))
        .collect();
    fn normalize(path: &str) -> String {
        let mut parts = Vec::new();
        for p in path.split('/') {
            match p {
                "" | "." => {}
                ".." => {
                    assert!(parts.pop().is_some(), "Dataset link escapes root");
                }
                _ => parts.push(p),
            }
        }
        parts.join("/")
    }
    fn resolve<'a>(
        name: &str,
        entries: &BTreeMap<String, (&'a [u8], u8, String)>,
        depth: usize,
    ) -> &'a [u8] {
        assert!(depth < 16, "Dataset link cycle");
        let (data, kind, link) = entries.get(name).expect("Dataset link target");
        if matches!(*kind, 0 | b'0') {
            return data;
        }
        let target = if *kind == b'2' {
            let parent = name.rsplit_once('/').map(|p| p.0).unwrap_or("");
            normalize(&format!("{parent}/{link}"))
        } else {
            normalize(link)
        };
        resolve(&target, entries, depth + 1)
    }
    for (name, (_, kind, _)) in &entries {
        if !matches!(*kind, 0 | b'0' | b'1' | b'2') {
            continue;
        }
        let Some(folder) = name
            .strip_prefix("usr/lib/locale/")
            .and_then(|s| s.strip_suffix("/LC_CTYPE"))
        else {
            continue;
        };
        let data = resolve(name, &entries, 0);
        if folder.contains('/') || data.len() < 68 {
            continue;
        }
        let off = u32::from_le_bytes(data[64..68].try_into().unwrap()) as usize;
        let Some(value) = data.get(off..) else {
            continue;
        };
        if !value.starts_with(b"UTF-8\0") {
            continue;
        }
        let base = folder.split('.').next().unwrap_or(folder);
        let Some((lang, territory)) = base.split_once('_') else {
            continue;
        };
        if !(2..=3).contains(&lang.len())
            || territory.len() != 2
            || !lang.bytes().all(|c| c.is_ascii_lowercase())
            || !territory.bytes().all(|c| c.is_ascii_uppercase())
        {
            continue;
        }
        let locale = format!("{base}.UTF-8");
        catalog.insert(locale, (folder.to_string(), display_name(base)));
    }
    catalog
        .into_iter()
        .map(|(code, (folder, label))| format!("{code}\t{folder}\t{label}\n"))
        .collect()
}
#[cfg(windows)]
fn display_name(base: &str) -> String {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocaleInfoEx(name: *const u16, kind: u32, data: *mut u16, size: i32) -> i32;
    }
    let tag: Vec<u16> = base.replace('_', "-").encode_utf16().chain([0]).collect();
    let mut out = [0u16; 256];
    let n = unsafe { GetLocaleInfoEx(tag.as_ptr(), 0x72, out.as_mut_ptr(), out.len() as i32) };
    if n > 1 {
        String::from_utf16_lossy(&out[..n as usize - 1])
    } else {
        base.into()
    }
}
#[cfg(not(windows))]
fn display_name(base: &str) -> String {
    base.into()
}
pub fn generate(out: &Path) {
    let folder = Path::new("pkg/languages");
    println!("cargo:rerun-if-changed={}", folder.display());
    let mut files: Vec<_> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|s| s == "deb"))
        .collect();
    files.sort();
    let mut generated = String::from("pub static PACKS: &[LocalePack] = &[\n");
    for (index, path) in files.iter().enumerate() {
        let bytes = std::fs::read(path).expect("Read dataset");
        let (name, control) = member(&bytes, "control.tar");
        let control = unpack(&name, control);
        let (_, control, _, _) = tar_entries(&control)
            .into_iter()
            .find(|e| e.0 == "control")
            .expect("Package control metadata");
        let fields: BTreeMap<_, _> = String::from_utf8_lossy(control)
            .lines()
            .filter_map(|line| line.split_once(':'))
            .map(|(k, v)| (k.to_owned(), v.trim().to_owned()))
            .collect();
        let version = fields.get("Version").expect("Package Version");
        let architecture = fields.get("Architecture").expect("Package Architecture");
        let abi = abi(version);
        let crc = crate::xz::crc32(&bytes);
        let marker = format!("# crc32={crc:08x}");
        let hint = path.with_extension("catalog.tsv");
        let catalog = std::fs::read_to_string(&hint)
            .ok()
            .filter(|s| s.lines().next() == Some(marker.as_str()))
            .map(|s| s.lines().skip(1).collect::<Vec<_>>().join("\n") + "\n")
            .unwrap_or_else(|| derive_catalog(&bytes));
        let catalog_file = out.join(format!("locale-catalog-{index}.tsv"));
        std::fs::write(&catalog_file, catalog).expect("Write catalog");
        let payload = path.canonicalize().expect("Dataset path");
        generated.push_str(&format!("LocalePack {{ abi: {abi:?}, architecture: {architecture:?}, catalog: include_str!({:?}), data: include_bytes!({:?}) }},\n",catalog_file.to_string_lossy(),payload.to_string_lossy()));
    }
    generated.push_str("];\n");
    std::fs::write(out.join("locale-packs.rs"), generated).expect("Write locale registry");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_follows_locale_hard_links() {
        let path = std::fs::read_dir("pkg/languages")
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| p.extension().is_some_and(|s| s == "deb"))
            .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        let derived = derive_catalog(&bytes);
        let hint = std::fs::read_to_string(path.with_extension("catalog.tsv")).unwrap();
        let keys = |s: &str| {
            s.lines()
                .filter_map(|l| {
                    let mut p = l.split('\t');
                    Some((p.next()?.to_owned(), p.next()?.to_owned()))
                })
                .collect::<BTreeMap<_, _>>()
        };
        assert_eq!(keys(&derived), keys(&hint));
    }
}
