#[allow(dead_code)]
#[path = "src/xz.rs"]
mod xz;

mod datasets {
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

    fn usable_name(name: &str) -> bool {
        !name.trim().is_empty() && !name.to_ascii_lowercase().contains("unknown")
    }

    fn identification_name(data: &[u8]) -> Option<String> {
        fn item(data: &[u8], index: usize) -> Option<&str> {
            let count = u32::from_le_bytes(data.get(4..8)?.try_into().ok()?) as usize;
            if index >= count {
                return None;
            }
            let at = 8 + index * 4;
            let offset = u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?) as usize;
            let tail = data.get(offset..)?;
            let end = tail.iter().position(|&b| b == 0)?;
            std::str::from_utf8(&tail[..end]).ok()
        }
        let language = item(data, 7)?.trim();
        let territory = item(data, 8).unwrap_or("").trim();
        if language.is_empty() {
            return item(data, 0).filter(|s| usable_name(s)).map(str::to_owned);
        }
        Some(if territory.is_empty() {
            language.into()
        } else {
            format!("{language} ({territory})")
        })
    }

    #[cfg(windows)]
    fn display_name(base: &str, fallback: &str) -> String {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetLocaleInfoEx(name: *const u16, kind: u32, data: *mut u16, size: i32) -> i32;
        }
        let tag: Vec<u16> = base.replace('_', "-").encode_utf16().chain([0]).collect();
        let mut out = [0u16; 256];
        let n = unsafe { GetLocaleInfoEx(tag.as_ptr(), 0x72, out.as_mut_ptr(), out.len() as i32) };
        if n > 1 {
            let name = String::from_utf16_lossy(&out[..n as usize - 1]);
            if usable_name(&name) {
                name
            } else {
                fallback.into()
            }
        } else {
            fallback.into()
        }
    }

    #[cfg(not(windows))]
    fn display_name(_base: &str, fallback: &str) -> String {
        fallback.into()
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
            let identification = format!("usr/lib/locale/{folder}/LC_IDENTIFICATION");
            let fallback = if entries.contains_key(&identification) {
                identification_name(resolve(&identification, &entries, 0))
                    .unwrap_or_else(|| base.into())
            } else {
                base.into()
            };
            catalog.insert(locale, (folder.to_string(), display_name(base, &fallback)));
        }
        catalog
            .into_iter()
            .map(|(code, (folder, label))| format!("{code}\t{folder}\t{label}\n"))
            .collect()
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
            let catalog = if catalog
                .lines()
                .filter_map(|l| l.split('\t').nth(2))
                .any(|label| !usable_name(label))
            {
                let derived = derive_catalog(&bytes);
                let names: BTreeMap<_, _> = derived
                    .lines()
                    .filter_map(|l| {
                        let mut p = l.split('\t');
                        Some((p.next()?, p.nth(1)?))
                    })
                    .collect();
                catalog
                    .lines()
                    .map(|l| {
                        let mut p = l.split('\t');
                        let code = p.next().unwrap_or("");
                        let folder = p.next().unwrap_or("");
                        let label = p.next().unwrap_or("");
                        format!(
                            "{code}\t{folder}\t{}\n",
                            if usable_name(label) {
                                label
                            } else {
                                names.get(code).copied().unwrap_or(code)
                            }
                        )
                    })
                    .collect::<String>()
            } else {
                catalog
            };
            let catalog_file = out.join(format!("locale-catalog-{index}.tsv"));
            std::fs::write(&catalog_file, catalog).expect("Write catalog");
            let payload = path.canonicalize().expect("Dataset path");
            generated.push_str(&format!(
                "LocalePack {{ abi: {abi:?}, architecture: {architecture:?}, catalog: include_str!({:?}), data: include_bytes!({:?}) }},\n",
                catalog_file.to_string_lossy(),
                payload.to_string_lossy()
            ));
        }
        generated.push_str("];\n");
        std::fs::write(out.join("locale-packs.rs"), generated).expect("Write locale registry");
    }
}

mod win_resource {
    // Native AMD64 COFF resource object (icon + manifest), authored with Rust std only.
    use std::{collections::BTreeMap, path::Path};

    fn u16le(b: &[u8], p: usize) -> u16 {
        u16::from_le_bytes(b[p..p + 2].try_into().unwrap())
    }
    fn u32le(b: &[u8], p: usize) -> u32 {
        u32::from_le_bytes(b[p..p + 4].try_into().unwrap())
    }
    fn set16(b: &mut [u8], p: usize, n: u16) {
        b[p..p + 2].copy_from_slice(&n.to_le_bytes());
    }
    fn set32(b: &mut [u8], p: usize, n: u32) {
        b[p..p + 4].copy_from_slice(&n.to_le_bytes());
    }
    fn alloc(bytes: &mut Vec<u8>, n: usize) -> usize {
        while bytes.len() % 4 != 0 {
            bytes.push(0);
        }
        let p = bytes.len();
        bytes.resize(p + n, 0);
        p
    }

    fn dir(
        bytes: &mut Vec<u8>,
        relocs: &mut Vec<u32>,
        items: &[(u16, u16, u16, Vec<u8>)],
        level: usize,
    ) -> usize {
        let mut groups: BTreeMap<u16, Vec<_>> = BTreeMap::new();
        for item in items {
            let id = match level {
                0 => item.0,
                1 => item.1,
                _ => item.2,
            };
            groups.entry(id).or_default().push(item.clone());
        }
        let at = alloc(bytes, 16 + groups.len() * 8);
        set16(bytes, at + 14, groups.len() as u16);
        for (i, (id, group)) in groups.into_iter().enumerate() {
            let entry = at + 16 + i * 8;
            set32(bytes, entry, id as u32);
            if level < 2 {
                let child = dir(bytes, relocs, &group, level + 1);
                set32(bytes, entry + 4, child as u32 | 0x80000000);
            } else {
                let data_entry = alloc(bytes, 16);
                let data = alloc(bytes, group[0].3.len());
                bytes[data..data + group[0].3.len()].copy_from_slice(&group[0].3);
                set32(bytes, data_entry, data as u32);
                set32(bytes, data_entry + 4, group[0].3.len() as u32);
                relocs.push(data_entry as u32);
                set32(bytes, entry + 4, data_entry as u32);
            }
        }
        at
    }

    pub fn write(path: &Path, ico: &[u8], manifest: &[u8]) -> std::io::Result<()> {
        if ico.len() < 6 || u16le(ico, 0) != 0 || u16le(ico, 2) != 1 {
            return Err(std::io::Error::other("Invalid ICO"));
        }
        let count = u16le(ico, 4) as usize;
        if count == 0 || 6 + count * 16 > ico.len() {
            return Err(std::io::Error::other("Invalid icon entries"));
        }
        let mut group = ico[..6].to_vec();
        let mut items = Vec::new();
        for i in 0..count {
            let at = 6 + i * 16;
            let size = u32le(ico, at + 8) as usize;
            let offset = u32le(ico, at + 12) as usize;
            let payload = ico
                .get(offset..offset + size)
                .ok_or_else(|| std::io::Error::other("Truncated icon"))?;
            group.extend_from_slice(&ico[at..at + 12]);
            group.extend_from_slice(&((i + 1) as u16).to_le_bytes());
            items.push((3, (i + 1) as u16, 1033, payload.to_vec()));
        }
        items.push((14, 1, 1033, group));
        items.push((24, 1, 1033, manifest.to_vec()));

        let mut resource = Vec::new();
        let mut relocs = Vec::new();
        dir(&mut resource, &mut relocs, &items, 0);
        let reloc_start = 60 + resource.len();
        let sym_start = reloc_start + relocs.len() * 10;
        let mut object = vec![0u8; 60];
        set16(&mut object, 0, 0x8664);
        set16(&mut object, 2, 1);
        set32(&mut object, 8, sym_start as u32);
        set32(&mut object, 12, 1);
        object[20..25].copy_from_slice(b".rsrc");
        set32(&mut object, 36, resource.len() as u32);
        set32(&mut object, 40, 60);
        set32(&mut object, 44, reloc_start as u32);
        set16(&mut object, 52, relocs.len() as u16);
        set32(&mut object, 56, 0x40300040);
        object.extend_from_slice(&resource);
        for at in relocs {
            object.extend_from_slice(&at.to_le_bytes());
            object.extend_from_slice(&0u32.to_le_bytes());
            object.extend_from_slice(&3u16.to_le_bytes());
        }
        let mut symbol = [0u8; 18];
        symbol[..5].copy_from_slice(b".rsrc");
        set16(&mut symbol, 12, 1);
        symbol[16] = 3;
        object.extend_from_slice(&symbol);
        object.extend_from_slice(&4u32.to_le_bytes());
        std::fs::write(path, object)
    }
}

fn generate_installer_style_ico() -> Vec<u8> {
    fn lerp(a: f32, b: f32, t: f32) -> f32 {
        a + (b - a) * t.clamp(0.0, 1.0)
    }
    fn rounded_rect_cov(px: f32, py: f32, s: f32, r: f32) -> f32 {
        let qx = (px - s * 0.5).abs() - (s * 0.5 - r);
        let qy = (py - s * 0.5).abs() - (s * 0.5 - r);
        let dist = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - r;
        ((-dist).clamp(0.0, 1.5) / 1.5).clamp(0.0, 1.0)
    }
    fn draw_fg(fx: f32, fy: f32, s: f32) -> Option<(u8, u8, u8, u8)> {
        let cx = s / 2.0;
        let arrow_w = s * 0.12;
        let arrow_tip_y = s * 0.62;
        let arrow_top = s * 0.18;
        let head_top = s * 0.44;
        let head_half = s * 0.24;
        let in_stem =
            fx >= cx - arrow_w && fx <= cx + arrow_w && fy >= arrow_top && fy <= head_top;
        let dy_head = arrow_tip_y - fy;
        let half_at = if dy_head >= 0.0 {
            dy_head / (arrow_tip_y - head_top) * head_half
        } else {
            0.0
        };
        let in_head = fy >= head_top && fy <= arrow_tip_y && (fx - cx).abs() <= half_at;
        let tray_top = s * 0.72;
        let tray_bot = s * 0.82;
        let tray_left = cx - s * 0.30;
        let tray_right = cx + s * 0.30;
        let tray_h = s * 0.05;
        let in_tray_h =
            fy >= tray_top && fy <= tray_top + tray_h && fx >= tray_left && fx <= tray_right;
        let in_tray_l =
            fx >= tray_left && fx <= tray_left + tray_h && fy >= tray_top && fy <= tray_bot;
        let in_tray_r =
            fx >= tray_right - tray_h && fx <= tray_right && fy >= tray_top && fy <= tray_bot;
        if in_stem || in_head || in_tray_h || in_tray_l || in_tray_r {
            Some((255, 255, 255, 235))
        } else {
            None
        }
    }

    let sizes: &[u32] = &[16, 32, 48, 256];
    let mut entries: Vec<(u8, Vec<u8>)> = Vec::new();
    for &sz in sizes {
        let s = sz as f32;
        let radius = s * 0.18;
        let mut pixels = vec![0u8; (sz * sz * 4) as usize];
        for y in 0..sz {
            for x in 0..sz {
                let fx = x as f32 + 0.5;
                let fy = y as f32 + 0.5;
                let cov = rounded_rect_cov(fx, fy, s, radius);
                let idx = ((y * sz + x) * 4) as usize;
                if cov < 0.01 {
                    continue;
                }
                let t = fy / s;
                let hl = ((0.18 - ((fx / s).powi(2) + (fy / s).powi(2)).sqrt()).max(0.0) * 60.0)
                    as u8;
                let r = (lerp(0x1e as f32, 0x25 as f32, t) as u8).saturating_add(hl);
                let g = (lerp(0x29 as f32, 0x63 as f32, t) as u8).saturating_add(hl);
                let b = (lerp(0x3b as f32, 0xeb as f32, t) as u8).saturating_add(hl);
                let alpha = (cov * 255.0) as u8;
                let (pr, pg, pb, pa) = if let Some((fr, fg, fb, fa)) = draw_fg(fx, fy, s) {
                    let a = fa as f32 / 255.0;
                    (
                        lerp(r as f32, fr as f32, a) as u8,
                        lerp(g as f32, fg as f32, a) as u8,
                        lerp(b as f32, fb as f32, a) as u8,
                        alpha,
                    )
                } else {
                    (r, g, b, alpha)
                };
                pixels[idx] = pr;
                pixels[idx + 1] = pg;
                pixels[idx + 2] = pb;
                pixels[idx + 3] = pa;
            }
        }
        let mut bmp = Vec::new();
        let pixel_bytes = sz * sz * 4;
        let mask_bytes = sz.div_ceil(32) * 4 * sz;
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&(sz as i32).to_le_bytes());
        bmp.extend_from_slice(&((sz * 2) as i32).to_le_bytes());
        bmp.extend_from_slice(&1u16.to_le_bytes());
        bmp.extend_from_slice(&32u16.to_le_bytes());
        bmp.extend_from_slice(&0u32.to_le_bytes());
        bmp.extend_from_slice(&pixel_bytes.to_le_bytes());
        bmp.extend_from_slice(&[0u8; 16]);
        for row in (0..sz).rev() {
            for col in 0..sz {
                let i = ((row * sz + col) * 4) as usize;
                bmp.push(pixels[i + 2]);
                bmp.push(pixels[i + 1]);
                bmp.push(pixels[i]);
                bmp.push(pixels[i + 3]);
            }
        }
        bmp.resize(bmp.len() + mask_bytes as usize, 0);
        entries.push((if sz >= 256 { 0 } else { sz as u8 }, bmp));
    }

    let mut ico = vec![0, 0, 1, 0];
    ico.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    let mut off = 6u32 + 16 * entries.len() as u32;
    for (sb, bmp) in &entries {
        ico.extend_from_slice(&[*sb, *sb, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(bmp.len() as u32).to_le_bytes());
        ico.extend_from_slice(&off.to_le_bytes());
        off += bmp.len() as u32;
    }
    for (_, bmp) in entries {
        ico.extend_from_slice(&bmp);
    }
    ico
}

fn main() {
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    datasets::generate(&out);
    slint_build::compile("ui/app.slint").expect("Slint compilation failed");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=ui/icons/app.ico");
    if std::env::var("TARGET")
        .unwrap_or_default()
        .starts_with("x86_64-pc-windows")
    {
        let ico = std::fs::read("ui/icons/app.ico")
            .ok()
            .filter(|b| !b.is_empty())
            .unwrap_or_else(generate_installer_style_ico);
        let manifest = br#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0" xmlns:asmv3="urn:schemas-microsoft-com:asm.v3">
<trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
  <security>
    <requestedPrivileges>
      <requestedExecutionLevel level="asInvoker" uiAccess="false" />
    </requestedPrivileges>
  </security>
</trustInfo>
<asmv3:application>
  <asmv3:windowsSettings>
    <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
    <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
  </asmv3:windowsSettings>
</asmv3:application>
</assembly>
"#;
        let path = out.join("app-icon.obj");
        win_resource::write(&path, &ico, manifest).expect("Build icon resource");
        println!("cargo:rustc-link-arg-bin=isolite={}", path.display());
    }
}
