use crate::iso::reader::{Extent, IsoReader};
use crate::iso::writer::repack_iso;
use crate::squash::native::{Kind, Node, Squash};
use crate::types::{Image, IsoReplacement, Package, Plan};
use crate::{checksum, language};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
type Result<T> = std::result::Result<T, String>;
fn text(fs: &mut Squash, path: &str) -> Option<String> {
    String::from_utf8(fs.read(path).ok()?).ok()
}
fn fields(record: &str) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    let mut last = String::new();
    for line in record.lines() {
        if line.starts_with([' ', '\t']) {
            if let Some(v) = values.get_mut(&last) {
                *v += line.trim();
            }
        } else if let Some((key, value)) = line.split_once(':') {
            last = key.into();
            values.insert(last.clone(), value.trim().into());
        }
    }
    values
}
fn dep_names(value: &str) -> Vec<Vec<String>> {
    value
        .split(',')
        .map(|part| {
            part.split('|')
                .filter_map(|alt| alt.split_whitespace().next())
                .map(|name| name.split(':').next().unwrap_or(name).to_owned())
                .collect()
        })
        .collect()
}
struct Inventory {
    records: BTreeMap<String, BTreeMap<String, String>>,
    lists: BTreeMap<String, Vec<String>>,
    protected: BTreeSet<String>,
    apps: BTreeSet<String>,
    providers: BTreeMap<String, BTreeSet<String>>,
}
fn indexed_text(fs: &mut Squash, paths: &BTreeMap<String,Node>, path: &str) -> Option<String> {
    let node=paths.get(path)?;
    if matches!(node.kind,Kind::File{..}) {String::from_utf8(fs.read_node(node).ok()?).ok()}
    else {text(fs,path)}
}
fn inventory(fs: &mut Squash, paths: &BTreeMap<String, Node>) -> Result<Inventory> {
    let status =
        indexed_text(fs, paths, "var/lib/dpkg/status").ok_or("ISO không có cơ sở dữ liệu dpkg được hỗ trợ")?;
    let mut inv = Inventory {
        records: BTreeMap::new(),
        lists: BTreeMap::new(),
        protected: BTreeSet::new(),
        apps: BTreeSet::new(),
        providers: BTreeMap::new(),
    };
    for record in status.replace("\r\n", "\n").split("\n\n") {
        let data = fields(record);
        if data
            .get("Status")
            .is_some_and(|s| s == "install ok installed")
        {
            if let Some(name) = data.get("Package") {
                inv.records.insert(name.clone(), data.clone());
            }
        }
    }
    for (name, data) in &inv.records {
        inv.providers
            .entry(name.clone())
            .or_default()
            .insert(name.clone());
        for p in dep_names(data.get("Provides").map(String::as_str).unwrap_or("")) {
            for provided in p {
                inv.providers
                    .entry(provided)
                    .or_default()
                    .insert(name.clone());
            }
        }
        if data.get("Essential").is_some_and(|s| s == "yes")
            || data.get("Protected").is_some_and(|s| s == "yes")
            || data
                .get("Priority")
                .is_some_and(|s| matches!(s.as_str(), "required" | "important"))
        {
            inv.protected.insert(name.clone());
        }
        let list_path = format!("var/lib/dpkg/info/{name}.list");
        let list = indexed_text(fs, paths, &list_path)
            .or_else(|| {
                data.get("Architecture")
                    .and_then(|arch| indexed_text(fs, paths, &format!("var/lib/dpkg/info/{name}:{arch}.list")))
            })
            .unwrap_or_default();
        let owned: Vec<String> = list
            .lines()
            .map(|p| p.trim().trim_start_matches('/').to_owned())
            .filter(|p| !p.is_empty() && p != ".")
            .collect();
        for path in &owned {
            if path.starts_with("usr/share/applications/") && path.ends_with(".desktop") {
                if let Some(desktop) = indexed_text(fs, paths, path) {
                    if !desktop
                        .lines()
                        .any(|l| matches!(l.trim(), "NoDisplay=true" | "Hidden=true"))
                    {
                        inv.apps.insert(name.clone());
                    }
                }
            }
            if path.ends_with(".service")
                || path.starts_with("usr/share/xsessions/")
                || path.starts_with("usr/share/wayland-sessions/")
                || path.starts_with("boot/")
                || path.starts_with("etc/calamares/")
                || path.starts_with("usr/share/initramfs-tools/")
                || matches!(path.as_str(), "etc/os-release" | "usr/lib/os-release")
            {
                inv.protected.insert(name.clone());
            }
        }
        inv.lists.insert(name.clone(), owned);
    }
    loop {
        let before = inv.protected.len();
        for name in inv.protected.clone() {
            let data = &inv.records[&name];
            for key in ["Depends", "Pre-Depends"] {
                for group in dep_names(data.get(key).map(String::as_str).unwrap_or("")) {
                    for alt in group {
                        if let Some(providers) = inv.providers.get(&alt) {
                            inv.protected.extend(providers.iter().cloned());
                        }
                    }
                }
            }
        }
        if before == inv.protected.len() {
            break;
        }
    }
    let _ = paths;
    Ok(inv)
}
pub fn inspect_iso<F: Fn(&str)>(path: &Path, log: F) -> Result<Image> {
    log("Đang nhận diện SquashFS và thông tin hệ điều hành từ ISO...");
    let mut iso = IsoReader::new(path).map_err(|e| e.to_string())?;
    let files = iso.files()?;
    let candidates: Vec<_> = files
        .iter()
        .filter_map(|(name, _)| {
            let offset = iso.squash(name);
            if offset > 0 {
                Some((name.clone(), offset))
            } else {
                None
            }
        })
        .collect();
    if candidates.len() != 1 {
        return Err(format!(
            "Tìm thấy {} SquashFS. Cần ISO có một root filesystem để tránh sửa nhầm ảnh.",
            candidates.len()
        ));
    }
    let (squash_iso_path, squash_offset) = candidates[0].clone();
    let mut fs = Squash::open(path, squash_offset)?;
    let paths = fs.paths()?;
    let os = text(&mut fs, "etc/os-release")
        .or_else(|| text(&mut fs, "usr/lib/os-release"))
        .unwrap_or_default();
    let release_fields: BTreeMap<_, _> = os
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.to_owned(), v.trim_matches(['\"', '\'']).to_owned()))
        .collect();
    let release = release_fields
        .get("PRETTY_NAME")
        .or_else(|| release_fields.get("NAME"))
        .cloned()
        .filter(|s| !s.is_empty())
        .or_else(|| iso.read_text("/.disk/info").map(|s| s.trim().into()))
        .unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
    let distro_name = release_fields
        .get("ID")
        .cloned()
        .unwrap_or_else(|| "Không xác định".into());
    log(&format!(
        "Hệ điều hành: {release}; ID={distro_name}; root={squash_iso_path}; block={} XZ",
        fs.block_size
    ));
    log("Đang nhận diện ứng dụng, dịch vụ hệ thống và phụ thuộc từ dpkg...");
    let inv = inventory(&mut fs, &paths)?;
    let mut packages = Vec::new();
    for (name, data) in &inv.records {
        packages.push(Package {
            name: name.clone(),
            version: data.get("Version").cloned().unwrap_or_default(),
            protected_package: inv.protected.contains(name) || !inv.apps.contains(name),
            group: if inv.apps.contains(name) {
                name.clone()
            } else {
                String::new()
            },
            installed_kib: data.get("Installed-Size").and_then(|s| s.parse().ok()),
        });
    }
    let supported_locales = paths
        .keys()
        .filter_map(|p| p.strip_prefix("usr/share/i18n/locales/"))
        .filter(|p| !p.contains('/'))
        .map(|base| format!("{base}.UTF-8"))
        .filter(|l| language::validate_locale(l).is_ok())
        .collect();
    let flatpak_bytes = paths
        .iter()
        .filter(|(p, _)| p.starts_with("var/lib/flatpak/"))
        .filter_map(|(_, n)| {
            if let Kind::File { size, .. } = n.kind {
                Some(size)
            } else {
                None
            }
        })
        .sum();
    Ok(Image {
        path: path.into(),
        release,
        distro_name,
        supported_locales,
        packages,
        has_flatpak: flatpak_bytes > 0,
        flatpak_bytes,
        squash_offset,
        squash_iso_path,
    })
}
pub fn extract_iso<F: Fn(&str)>(path: &Path, dest: &Path, log: F) -> Result<()> {
    log("Đang trích xuất ISO...");
    IsoReader::new(path)
        .map_err(|e| e.to_string())?
        .extract_to(dest)
}
pub fn export_package_files<F: Fn(&str)>(
    image: &Image,
    name: &str,
    dest: &Path,
    log: F,
) -> Result<()> {
    let mut fs = Squash::open(&image.path, image.squash_offset)?;
    let paths = fs.paths()?;
    let inv = inventory(&mut fs, &paths)?;
    for path in inv.lists.get(name).ok_or("Không tìm thấy gói")? {
        if path
            .split('/')
            .any(|p| p == ".." || p.contains(['\\', ':']))
        {
            return Err("Đường dẫn gói không an toàn trên Windows".into());
        }
        if let Some(node) = paths.get(path) {
            if matches!(node.kind, Kind::File { .. }) {
                let target = dest.join(path);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                fs::write(target, fs.read_node(node)?).map_err(|e| e.to_string())?;
            }
        }
    }
    log("Đã xuất tệp thường; liên kết và quyền Linux chỉ được bảo toàn trong ISO.");
    Ok(())
}
fn validate_removals(inv: &Inventory, remove: &BTreeSet<String>) -> Result<()> {
    for name in remove {
        if !inv.records.contains_key(name) {
            return Err(format!("Không tìm thấy gói: {name}"));
        }
        if inv.protected.contains(name) || !inv.apps.contains(name) {
            return Err(format!("Gói hệ thống được bảo vệ: {name}"));
        }
    }
    for (name, data) in &inv.records {
        if remove.contains(name) {
            continue;
        }
        for key in ["Depends", "Pre-Depends"] {
            for group in dep_names(data.get(key).map(String::as_str).unwrap_or("")) {
                let providers: BTreeSet<_> = group
                    .iter()
                    .filter_map(|alt| inv.providers.get(alt))
                    .flatten()
                    .cloned()
                    .collect();
                if !providers.is_empty() && providers.iter().all(|p| remove.contains(p)) {
                    return Err(format!(
                        "Không thể gỡ {}: gói còn lại {name} cần phụ thuộc này.",
                        group.join(" | ")
                    ));
                }
            }
        }
    }
    Ok(())
}
fn stage_packages(
    packages: impl Iterator<Item = PathBuf>,
    added: &mut BTreeMap<String, Vec<u8>>,
) -> Result<()> {
    for (i, path) in packages.enumerate() {
        let bytes = fs::read(&path).map_err(|e| e.to_string())?;
        if !bytes.starts_with(b"!<arch>\n") || path.extension().is_none_or(|s| s != "deb") {
            return Err(format!("Chỉ hỗ trợ gói Debian .deb: {}", path.display()));
        }
        added.insert(
            format!("usr/share/isolite/language-packs/{i:04}.deb"),
            bytes,
        );
    }
    Ok(())
}
pub fn build_iso<L: Fn(&str), P: Fn(i32, &str)>(
    plan: &Plan,
    dest: &Path,
    log: L,
    progress: P,
) -> Result<()> {
    if dest.exists() {
        return Err("Tệp đầu ra đã tồn tại; chọn tên mới để tránh ghi đè.".into());
    }
    let parent = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let stage = parent.join(format!(
        ".isolite-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    ));
    fs::create_dir(&stage).map_err(|e| e.to_string())?;
    let result = (|| {
        progress(2, "Đang kiểm tra nguồn và kế hoạch...");
        let source_meta = fs::metadata(&plan.image.path).map_err(|e| e.to_string())?;
        let mut source_iso = IsoReader::new(&plan.image.path).map_err(|e| e.to_string())?;
        let source_files = source_iso.files()?;
        let root_extent = source_iso
            .find(&plan.image.squash_iso_path)
            .ok_or("Không tìm thấy root filesystem")?;
        if root_extent.offset != plan.image.squash_offset {
            return Err("ISO nguồn đã thay đổi; hãy mở lại ISO.".into());
        }
        let mut squash = Squash::open(&plan.image.path, root_extent.offset)?;
        let source_super = squash.superblock.clone();
        let paths = squash.paths()?;
        let inv = inventory(&mut squash, &paths)?;
        validate_removals(&inv, &plan.remove)?;
        let locales = language::selected_locales(&plan.locales, plan.default_locale.as_deref())?;
        let mut added = BTreeMap::new();
        let mut removed = BTreeSet::new();
        let mut retained_files = BTreeSet::new();
        for (name, owned) in &inv.lists {
            if !plan.remove.contains(name) {
                retained_files.extend(owned.iter().cloned());
            }
        }
        for name in &plan.remove {
            for path in &inv.lists[name] {
                if !retained_files.contains(path)
                    && paths
                        .get(path)
                        .is_some_and(|n| !matches!(n.kind, Kind::Dir { .. }))
                {
                    removed.insert(path.clone());
                }
            }
            for path in paths.keys() {
                if let Some(file) = path.strip_prefix("var/lib/dpkg/info/") {
                    if file.starts_with(&format!("{name}."))
                        || file.starts_with(&format!("{name}:"))
                    {
                        removed.insert(path.clone());
                    }
                }
            }
        }
        if !plan.remove.is_empty() {
            let status = text(&mut squash, "var/lib/dpkg/status").ok_or("Missing dpkg status")?;
            let status = status
                .split("\n\n")
                .filter(|r| {
                    fields(r)
                        .get("Package")
                        .is_none_or(|n| !plan.remove.contains(n))
                })
                .collect::<Vec<_>>()
                .join("\n\n");
            added.insert("var/lib/dpkg/status".into(), status.into_bytes());
        }
        if plan.clean_flatpak {
            for path in paths.keys() {
                if path.starts_with("var/lib/flatpak/")
                    && !matches!(paths[path].kind, Kind::Dir { .. })
                {
                    removed.insert(path.clone());
                }
            }
        }
        for (name, folder) in &plan.replaced_folders {
            log(&format!("Áp dụng tệp thay thế: {name}"));
            collect_overlay(folder, folder, &mut added)?;
        }
        stage_packages(
            plan.debs.iter().chain(plan.language_debs.iter()).cloned(),
            &mut added,
        )?;
        let configure =
            !locales.is_empty() || !plan.debs.is_empty() || !plan.language_debs.is_empty();
        if configure {
            for locale in &locales {
                let base = locale.trim_end_matches(".UTF-8");
                if !paths.contains_key(&format!("usr/share/i18n/locales/{base}")) {
                    return Err(format!("ISO thiếu định nghĩa locale {base}"));
                }
            }
            let old = text(&mut squash, "etc/locale.gen").unwrap_or_default();
            added.insert(
                "etc/locale.gen".into(),
                language::merge_locale_gen(&old, &locales).into_bytes(),
            );
            added.insert(
                "usr/share/isolite/locales".into(),
                format!(
                    "{}\n",
                    locales.iter().cloned().collect::<Vec<_>>().join("\n")
                )
                .into_bytes(),
            );
            added.insert(
                "usr/lib/isolite/setup-locales.sh".into(),
                language::SETUP_SCRIPT.replace("\r\n", "\n").into_bytes(),
            );
            added.insert(
                "etc/systemd/system/isolite-locales.service".into(),
                language::SETUP_SERVICE.as_bytes().into(),
            );
            // A target drop-in activates the service without replacing vendor
            // units or fabricating a regular file inside a .wants directory.
            added.insert(
                "etc/systemd/system/multi-user.target.d/isolite-locales.conf".into(),
                b"[Unit]\nWants=isolite-locales.service\n".to_vec(),
            );
            if paths.contains_key("var/lib/isolite/locales-ready") {
                removed.insert("var/lib/isolite/locales-ready".into());
            }
        }
        if let Some(locale) = &plan.default_locale {
            for path in ["etc/default/locale", "etc/locale.conf", "etc/environment"] {
                let old = text(&mut squash, path).unwrap_or_default();
                added.insert(
                    path.into(),
                    language::set_language(&old, locale).into_bytes(),
                );
            }
        }
        if plan.image.release != plan.original_release {
            return Err("Đổi tên bản phát hành tạm tắt để giữ metadata nhận diện gốc.".into());
        }
        progress(20, "Đang đóng gói SquashFS với thông số gốc...");
        let root_new = stage.join("filesystem.squashfs");
        squash.overlay(&root_new, &added, &removed)?;
        progress(45, "Đang kiểm tra nội dung và metadata SquashFS...");
        let mut verified = Squash::open(&root_new, 0)?;
        verify_squash(&paths, &mut verified, &source_super, &added, &removed)?;
        verify_original_data(
            &mut squash,
            &mut verified,
            &paths,
            &added,
            &removed,
            &plan.image.path,
            plan.image.squash_offset,
            &root_new,
        )?;
        let mut replacements = vec![IsoReplacement {
            iso_path: plan.image.squash_iso_path.clone(),
            local: root_new.clone(),
        }];
        if let Some(locale) = &plan.default_locale {
            for (i, (path, _)) in source_files.iter().enumerate() {
                if path.ends_with(".cfg") {
                    if let Some(old) = source_iso.read_text(path) {
                        let new = language::patch_boot_config(&old, locale);
                        if old != new {
                            let local = stage.join(format!("boot-{i}.cfg"));
                            fs::write(&local, new).map_err(|e| e.to_string())?;
                            replacements.push(IsoReplacement {
                                iso_path: path.clone(),
                                local,
                            });
                        }
                    }
                }
            }
        }
        // Update package manifests already present beside the root filesystem.
        let prefix = plan
            .image
            .squash_iso_path
            .rsplit_once('/')
            .map(|p| p.0)
            .unwrap_or("");
        for suffix in ["filesystem.manifest", "filesystem.packages"] {
            let path = format!("{prefix}/{suffix}");
            if let Some(old) = source_iso.read_text(&path) {
                let new = old
                    .lines()
                    .filter(|line| {
                        line.split_whitespace()
                            .next()
                            .is_none_or(|name| !plan.remove.contains(name))
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
                    + "\n";
                if new != old {
                    let local = stage.join(suffix);
                    fs::write(&local, new).map_err(|e| e.to_string())?;
                    replacements.push(IsoReplacement {
                        iso_path: path,
                        local,
                    });
                }
            }
        }
        let size_path = format!("{prefix}/filesystem.size");
        if source_iso.find(&size_path).is_some() {
            let current = verified.paths()?;
            let bytes: u64 = current
                .values()
                .filter_map(|n| {
                    if let Kind::File { size, .. } = n.kind {
                        Some(size)
                    } else {
                        None
                    }
                })
                .sum();
            let local = stage.join("filesystem.size");
            fs::write(&local, format!("{bytes}\n")).map_err(|e| e.to_string())?;
            replacements.push(IsoReplacement {
                iso_path: size_path,
                local,
            });
        }
        progress(55, "Đang cập nhật checksum của các tệp thay đổi...");
        let mut hashes = BTreeMap::new();
        for replacement in &replacements {
            hashes.insert(
                replacement.iso_path.clone(),
                checksum::md5_file(&replacement.local)?,
            );
        }
        for (i, path) in ["/md5sum.txt", "/MD5SUMS"].iter().enumerate() {
            if let Some(old) = source_iso.read_text(path) {
                let mut new = String::new();
                for line in old.lines() {
                    let updated = if line.len() > 34 && line.is_char_boundary(32) {
                        let name = line[32..]
                            .trim_start()
                            .trim_start_matches('*')
                            .trim_start_matches("./");
                        hashes
                            .get(&format!("/{}", name.trim_start_matches('/')))
                            .map(|hash| format!("{hash}{}", &line[32..]))
                    } else {
                        None
                    };
                    new.push_str(updated.as_deref().unwrap_or(line));
                    new.push('\n');
                }
                let local = stage.join(format!("checksum-{i}"));
                fs::write(&local, new).map_err(|e| e.to_string())?;
                // MD5SUMS may itself contain the digest of md5sum.txt.
                hashes.insert((*path).to_string(), checksum::md5_file(&local)?);
                replacements.push(IsoReplacement {
                    iso_path: (*path).into(),
                    local,
                });
            }
        }
        if source_iso.find("/.md5sum.txt").is_some() {
            log("ISO có chữ ký riêng của nhà phát hành. Chữ ký này không thể được ký lại khi tùy biến; cần thử bộ cài trong VM.");
        }
        let partial = stage.join("output.iso");
        progress(65, "Đang ghi ISO mới...");
        repack_iso(
            &plan.image.path,
            &partial,
            &replacements,
            &[],
            Some(&|pct, msg| progress(65 + pct / 5, msg)),
        )?;
        progress(87, "Đang đối chiếu toàn bộ tệp và cấu trúc boot...");
        verify_iso(&plan.image.path, &partial, &source_files, &replacements)?;
        let after = fs::metadata(&plan.image.path).map_err(|e| e.to_string())?;
        if source_meta.len() != after.len() || source_meta.modified().ok() != after.modified().ok()
        {
            return Err("ISO nguồn bị thay đổi trong lúc xử lý; không xuất kết quả.".into());
        }
        if dest.exists() {
            return Err("Đích đã xuất hiện; không ghi đè.".into());
        }
        fs::rename(&partial, dest).map_err(|e| e.to_string())?;
        progress(100, "Hoàn tất, đã kiểm tra ISO mới.");
        log(&format!("Đã tạo {}", dest.display()));
        Ok(())
    })();
    if result.is_ok() {
        let _ = fs::remove_dir_all(&stage);
    } else {
        log(&format!(
            "Không công bố ISO lỗi. Thư mục chẩn đoán: {}",
            stage.display()
        ));
    }
    result
}
fn collect_overlay(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) -> Result<()> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_symlink() {
            return Err("Không nhận symlink Windows trong thư mục thay thế".into());
        }
        if ty.is_dir() {
            collect_overlay(root, &path, out)?;
        } else {
            let rel = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            out.insert(rel, fs::read(path).map_err(|e| e.to_string())?);
        }
    }
    Ok(())
}
fn verify_squash(
    original: &BTreeMap<String, Node>,
    fs: &mut Squash,
    sb: &[u8],
    added: &BTreeMap<String, Vec<u8>>,
    removed: &BTreeSet<String>,
) -> Result<()> {
    if fs.superblock[12..32] != sb[12..32] {
        return Err("Thông số SquashFS gốc bị thay đổi".into());
    }
    let current = fs.paths()?;
    let mut link_counts = BTreeMap::new();
    for node in current.values() {
        *link_counts.entry(node.reference).or_insert(0u32) += 1;
    }

    for (path, old) in original {
        let gone = removed
            .iter()
            .any(|p| path == p || path.starts_with(&format!("{p}/")));
        if gone {
            if current.contains_key(path) {
                return Err(format!("Tệp chưa được gỡ: {path}"));
            }
            continue;
        }
        let node = current
            .get(path)
            .ok_or_else(|| format!("Mất tệp ngoài kế hoạch: {path}"))?;
        if !added.contains_key(path) && !matches!(old.kind, Kind::Dir { .. }) {
            let mut expected = old.raw.clone();
            if let Kind::File { .. } = old.kind {
                let end = if u16::from_le_bytes(old.header[..2].try_into().unwrap()) == 2 {
                    20
                } else {
                    24
                };
                expected[16..end].copy_from_slice(&node.raw[16..end]);
            }
            let kind = u16::from_le_bytes(old.header[..2].try_into().unwrap());
            if kind == 9 {
                expected[40..44].copy_from_slice(&link_counts[&node.reference].to_le_bytes());
            } else if matches!(kind, 3..=7 | 10..=14) {
                expected[16..20].copy_from_slice(&link_counts[&node.reference].to_le_bytes());
            }
            if expected != node.raw {
                return Err(format!("Metadata tệp ngoài kế hoạch thay đổi: {path}"));
            }
        }
        if old.header[2..12] != node.header[2..12] {
            return Err(format!(
                "Quyền/UID/GID/mtime thay đổi ngoài kế hoạch: {path}"
            ));
        }
    }
    for (path, expected) in added {
        if fs.read(path)? != *expected {
            return Err(format!("Sai nội dung tệp mới: {path}"));
        }
    }
    Ok(())
}
fn compare(a: &mut File, ao: u64, b: &mut File, bo: u64, length: u64) -> Result<()> {
    a.seek(SeekFrom::Start(ao)).map_err(|e| e.to_string())?;
    b.seek(SeekFrom::Start(bo)).map_err(|e| e.to_string())?;
    let mut left = length;
    let mut aa = vec![0; 1024 * 1024];
    let mut bb = vec![0; 1024 * 1024];
    while left > 0 {
        let n = (left as usize).min(aa.len());
        a.read_exact(&mut aa[..n]).map_err(|e| e.to_string())?;
        b.read_exact(&mut bb[..n]).map_err(|e| e.to_string())?;
        if aa[..n] != bb[..n] {
            return Err("Nội dung ISO không khớp khi kiểm tra".into());
        }
        left -= n as u64;
    }
    Ok(())
}
fn verify_iso(
    source: &Path,
    target: &Path,
    files: &[(String, Extent)],
    replacements: &[IsoReplacement],
) -> Result<()> {
    let mut iso = IsoReader::new(target).map_err(|e| e.to_string())?;
    let output_files = iso.files()?;
    if output_files.len() != files.len() {
        return Err("Danh sách tệp ISO bị thay đổi ngoài kế hoạch".into());
    }
    let mut original = IsoReader::new(source).map_err(|e| e.to_string())?;
    if original.has_el_torito_boot() != iso.has_el_torito_boot()
        || original.has_bootable_structures() != iso.has_bootable_structures()
    {
        return Err("Cấu trúc boot ISO không khớp".into());
    }
    let mut a = File::open(source).map_err(|e| e.to_string())?;
    let mut b = File::open(target).map_err(|e| e.to_string())?;
    for (path, old) in files {
        let new = iso
            .find(path)
            .ok_or_else(|| format!("Mất tệp ISO: {path}"))?;
        if let Some(replacement) = replacements.iter().find(|r| r.iso_path == *path) {
            let mut expected = File::open(&replacement.local).map_err(|e| e.to_string())?;
            if expected.metadata().map_err(|e| e.to_string())?.len() != new.size {
                return Err(format!("Sai kích thước tệp: {path}"));
            }
            compare(&mut expected, 0, &mut b, new.offset, new.size)?;
        } else {
            if old.size != new.size {
                return Err(format!("Tệp bị đổi kích thước ngoài kế hoạch: {path}"));
            }
            compare(&mut a, old.offset, &mut b, new.offset, old.size)?;
        }
    }
    Ok(())
}

fn verify_original_data(
    source_fs: &mut Squash,
    target_fs: &mut Squash,
    original: &BTreeMap<String, Node>,
    added: &BTreeMap<String, Vec<u8>>,
    removed: &BTreeSet<String>,
    source_path: &Path,
    source_base: u64,
    target_path: &Path,
) -> Result<()> {
    let current = target_fs.paths()?;
    let mut source = File::open(source_path).map_err(|e| e.to_string())?;
    let mut target = File::open(target_path).map_err(|e| e.to_string())?;
    let mut checked = BTreeSet::new();
    let mut fragments = BTreeSet::new();
    for (path, node) in original {
        if added.contains_key(path) || removed.contains(path) {
            continue;
        }
        if let Kind::File {
            start,
            fragment,
            ref blocks,
            ..
        } = node.kind
        {
            let Some(new) = current.get(path) else {
                continue;
            };
            let Kind::File {
                start: new_start, ..
            } = new.kind
            else {
                return Err("File type changed".into());
            };
            let length: u64 = blocks.iter().map(|b| (b & 0xffffff) as u64).sum();
            if length > 0 && checked.insert((start, length)) {
                compare(
                    &mut source,
                    source_base + start,
                    &mut target,
                    new_start,
                    length,
                )?;
            }
            if fragment != u32::MAX && fragments.insert(fragment) {
                let (old_at, old_descriptor) = source_fs.fragment_info(fragment)?;
                let (new_at, new_descriptor) = target_fs.fragment_info(fragment)?;
                if old_descriptor != new_descriptor {
                    return Err("Fragment compression changed".into());
                }
                compare(
                    &mut source,
                    source_base + old_at,
                    &mut target,
                    new_at,
                    (old_descriptor & 0xffffff) as u64,
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Inventory {
        let mut records = BTreeMap::new();
        records.insert(
            "desktop-app".into(),
            fields("Package: desktop-app\nDepends: helper | alternative\n"),
        );
        records.insert("helper".into(), fields("Package: helper"));
        records.insert("alternative".into(), fields("Package: alternative"));
        Inventory {
            records,
            lists: BTreeMap::new(),
            protected: BTreeSet::new(),
            apps: ["desktop-app", "helper", "alternative"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            providers: [
                ("helper".into(), ["helper".into()].into_iter().collect()),
                (
                    "alternative".into(),
                    ["alternative".into()].into_iter().collect(),
                ),
            ]
            .into_iter()
            .collect(),
        }
    }
    #[test]
    fn refuses_broken_dependencies_and_unknown_packages() {
        let inv = sample();
        assert!(validate_removals(
            &inv,
            &["helper".into(), "alternative".into()]
                .into_iter()
                .collect()
        )
        .is_err());
        assert!(validate_removals(&inv, &["missing".into()].into_iter().collect()).is_err());
        assert!(validate_removals(&inv, &["helper".into()].into_iter().collect()).is_ok());
    }
    #[test]
    fn protected_metadata_blocks_removal() {
        let mut inv = sample();
        inv.protected.insert("desktop-app".into());
        assert!(validate_removals(&inv, &["desktop-app".into()].into_iter().collect()).is_err());
    }
    #[test]
    fn parses_arch_qualifiers_and_alternatives() {
        assert_eq!(
            dep_names("helper:any (>= 1), other | third"),
            vec![vec!["helper"], vec!["other", "third"]]
        );
        assert_eq!(
            fields("Package: demo\nDescription: a\n b")["Description"],
            "ab"
        );
    }
}
