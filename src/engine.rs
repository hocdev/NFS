use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use sha2::{Digest, Sha512};

use crate::cache::{save_cache, try_load_cache};
use crate::iso::reader::IsoReader;
use crate::iso::writer::repack_iso;
use crate::package::arch::check_arch_package;
use crate::package::deb::extract_deb_archives;
use crate::squash::reader::{detect_flatpak, read_squash_file, visit_pacman_descriptions};
use crate::squash::writer::rebuild_squashfs;
use crate::types::{
    app_group, arch_app_group, arch_protected_package, protected_package, DistroType, Image,
    IsoReplacement, Package, Plan,
};

pub fn inspect_iso<F: Fn(&str)>(iso_path: &Path, log: F) -> Result<Image, String> {
    if !iso_path.exists() {
        return Err(format!("Tệp ISO không tồn tại: {}", iso_path.display()));
    }

    log(&format!("Đang kiểm tra ảnh ISO: {}", iso_path.display()));

    let meta = fs::metadata(iso_path).map_err(|e| format!("Không thể đọc thông tin tệp ISO: {}", e))?;
    let iso_size = meta.len();
    let mtime = meta.modified()
        .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
        .unwrap_or(0);

    // 1. Check binary cache first
    if let Some(mut cached) = try_load_cache(iso_path, iso_size, mtime) {
        log("Đã nạp nhanh thông tin từ bộ nhớ đệm (Cache hit).");
        // Ensure squash offset is set
        if cached.squash_offset == 0 {
            if let Ok(mut iso) = IsoReader::new(iso_path) {
                let off = iso.squash(&cached.squash_iso_path);
                if off > 0 {
                    cached.squash_offset = off;
                }
            }
        }
        return Ok(cached);
    }

    // 2. Read ISO layout
    log("Đang phân tích cấu trúc hệ thống tập tin ISO 9660 / Joliet / Rock Ridge...");
    let mut iso = IsoReader::new(iso_path).map_err(|e| e.to_string())?;

    let mut image = Image {
        path: iso_path.to_path_buf(),
        distro: DistroType::DebianUbuntu,
        release: String::new(),
        squash_offset: 0,
        squash_iso_path: String::new(),
        packages: Vec::new(),
        has_flatpak: false,
        flatpak_bytes: 0,
    };

    // Detect distro type & locate SquashFS
    let arch_sfs = iso.squash("/arch/x86_64/airootfs.sfs");
    let casper_sfs = iso.squash("/casper/filesystem.squashfs");
    let live_sfs = iso.squash("/live/filesystem.squashfs");

    let (squash_path, distro) = if arch_sfs > 0 {
        image.squash_offset = arch_sfs;
        ("/arch/x86_64/airootfs.sfs", DistroType::ArchLinux)
    } else if casper_sfs > 0 {
        image.squash_offset = casper_sfs;
        ("/casper/filesystem.squashfs", DistroType::DebianUbuntu)
    } else if live_sfs > 0 {
        image.squash_offset = live_sfs;
        ("/live/filesystem.squashfs", DistroType::DebianUbuntu)
    } else {
        return Err("Không tìm thấy tệp SquashFS hợp lệ trong ISO (/casper, /live, hoặc /arch).".to_string());
    };

    image.distro = distro;
    image.squash_iso_path = squash_path.to_string();

    // Detect Flatpak
    log("Đang quét các thành phần Flatpak...");
    let (has_flatpak, flatpak_bytes) = detect_flatpak(iso_path, image.squash_offset);
    image.has_flatpak = has_flatpak;
    image.flatpak_bytes = flatpak_bytes;

    // Detect Release name
    match image.distro {
        DistroType::ArchLinux => {
            if let Some(ver) = iso.read_text("/arch/version") {
                let v = ver.trim();
                image.release = if v.is_empty() {
                    "CachyOS Desktop Linux".to_string()
                } else {
                    format!("CachyOS Desktop Linux {}", v)
                };
            } else if let Some(bytes) = read_squash_file(iso_path, image.squash_offset, "etc/os-release") {
                image.release = parse_pretty_name(&String::from_utf8_lossy(&bytes));
            }
            if image.release.is_empty() {
                image.release = "CachyOS Desktop Linux".to_string();
            }

            log("Đang đọc danh sách gói Pacman...");
            let mut packages_map: BTreeMap<String, Package> = BTreeMap::new();

            // 1. Try reading /arch/pkglist.x86_64.txt directly from ISO (instant, matches C++ behavior)
            if let Some(list_text) = iso.read_text("/arch/pkglist.x86_64.txt") {
                for line in list_text.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let name = parts[0].to_string();
                        let version = parts[1].to_string();
                        let grp = arch_app_group(&name);
                        let prot = grp.is_empty() || arch_protected_package(&name);
                        packages_map.insert(
                            name.clone(),
                            Package {
                                name,
                                version,
                                group: grp,
                                protected_package: prot,
                                installed_kib: None,
                            },
                        );
                    }
                }
            }

            // 2. Fallback: if not found, read from squashfs pacman local db
            if packages_map.is_empty() {
                visit_pacman_descriptions(iso_path, image.squash_offset, |desc_text| {
                    if let Some(pkg) = parse_pacman_desc(desc_text) {
                        packages_map.insert(pkg.name.clone(), pkg);
                    }
                });
            }

            image.packages = packages_map.into_values().collect();
        }
        DistroType::DebianUbuntu => {
            if let Some(info) = iso.read_text("/.disk/info") {
                image.release = info.trim().to_string();
            } else if let Some(bytes) = read_squash_file(iso_path, image.squash_offset, "etc/os-release") {
                image.release = parse_pretty_name(&String::from_utf8_lossy(&bytes));
            }
            if image.release.is_empty() {
                image.release = "Ubuntu Linux".to_string();
            }

            log("Đang đọc cơ sở dữ liệu gói dpkg...");
            // Read dpkg status from squashfs
            let dpkg_status_bytes = read_squash_file(iso_path, image.squash_offset, "var/lib/dpkg/status");
            let mut packages_map: BTreeMap<String, Package> = BTreeMap::new();

            if let Some(ref bytes) = dpkg_status_bytes {
                let status_str = String::from_utf8_lossy(bytes);
                for record in status_str.split("\n\n") {
                    let record = record.trim();
                    if record.is_empty() {
                        continue;
                    }
                    let mut name = String::new();
                    let mut version = String::new();
                    let mut status = String::new();
                    let mut size_kib: Option<u64> = None;

                    for line in record.lines() {
                        if let Some(val) = line.strip_prefix("Package: ") {
                            name = val.trim().to_string();
                        } else if let Some(val) = line.strip_prefix("Version: ") {
                            version = val.trim().to_string();
                        } else if let Some(val) = line.strip_prefix("Status: ") {
                            status = val.trim().to_string();
                        } else if let Some(val) = line.strip_prefix("Installed-Size: ") {
                            size_kib = val.trim().parse::<u64>().ok();
                        }
                    }

                    if !name.is_empty() && status.contains("installed") {
                        let grp = app_group(&name);
                        let prot = grp.is_empty() || protected_package(&name);
                        packages_map.insert(
                            name.clone(),
                            Package {
                                name,
                                version,
                                group: grp,
                                protected_package: prot,
                                installed_kib: size_kib,
                            },
                        );
                    }
                }
            }

            // Fallback: read filesystem.manifest if dpkg status was empty
            if packages_map.is_empty() {
                let manifest_path = if squash_path.starts_with("/live") {
                    "/live/filesystem.manifest"
                } else {
                    "/casper/filesystem.manifest"
                };
                if let Some(manifest) = iso.read_text(manifest_path) {
                    for line in manifest.lines() {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 {
                            let name = parts[0].to_string();
                            let version = parts[1].to_string();
                            let grp = app_group(&name);
                            let prot = grp.is_empty() || protected_package(&name);
                            packages_map.insert(
                                name.clone(),
                                Package {
                                    name,
                                    version,
                                    group: grp,
                                    protected_package: prot,
                                    installed_kib: None,
                                },
                            );
                        }
                    }
                }
            }

            image.packages = packages_map.into_values().collect();
        }
    }

    log(&format!(
        "Tìm thấy {} gói cài đặt trong hệ thống. Bản phát hành: {}",
        image.packages.len(),
        image.release
    ));

    // Save to cache for instant future loads
    save_cache(&image, iso_size, mtime);

    Ok(image)
}

fn parse_pretty_name(os_release: &str) -> String {
    for line in os_release.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("PRETTY_NAME=") {
            return val.trim_matches('"').trim_matches('\'').to_string();
        }
    }
    String::new()
}

fn parse_pacman_desc(desc: &str) -> Option<Package> {
    let mut name = String::new();
    let mut version = String::new();
    let mut size_bytes: Option<u64> = None;

    let mut current_field = "";
    for line in desc.lines() {
        let line = line.trim();
        if line.starts_with('%') && line.ends_with('%') {
            current_field = line;
            continue;
        }
        if line.is_empty() {
            continue;
        }
        match current_field {
            "%NAME%" => name = line.to_string(),
            "%VERSION%" => version = line.to_string(),
            "%SIZE%" | "%ISIZE%" => size_bytes = line.parse::<u64>().ok(),
            _ => {}
        }
    }

    if name.is_empty() {
        return None;
    }

    let grp = arch_app_group(&name);
    let prot = grp.is_empty() || arch_protected_package(&name);
    let kib = size_bytes.map(|b| (b + 1023) / 1024);

    Some(Package {
        name,
        version,
        group: grp,
        protected_package: prot,
        installed_kib: kib,
    })
}

pub fn extract_iso<F: Fn(&str)>(iso_path: &Path, dest_dir: &Path, log: F) -> Result<(), String> {
    log(&format!(
        "Bắt đầu giải nén ISO {} vào {}",
        iso_path.display(),
        dest_dir.display()
    ));
    let mut iso = IsoReader::new(iso_path).map_err(|e| e.to_string())?;
    fs::create_dir_all(dest_dir).map_err(|e| format!("Không thể tạo thư mục đích: {}", e))?;
    iso.extract_to(dest_dir)?;
    log("Giải nén ISO hoàn tất thành công!");
    Ok(())
}

pub fn export_package_files<F: Fn(&str)>(
    image: &Image,
    target_name: &str,
    dest_dir: &Path,
    log: F,
) -> Result<usize, String> {
    log(&format!(
        "Đang trích xuất tệp của gói '{}' vào {}...",
        target_name,
        dest_dir.display()
    ));

    fs::create_dir_all(dest_dir).map_err(|e| format!("Không thể tạo thư mục xuất: {}", e))?;

    let file = File::open(&image.path).map_err(|e| format!("Không thể mở ISO: {}", e))?;
    let reader = BufReader::new(file);
    let fs_reader = backhand::FilesystemReader::from_reader_with_offset(reader, image.squash_offset)
        .map_err(|e| format!("Không thể phân tích SquashFS: {}", e))?;

    let mut count = 0;
    let target_lower = target_name.to_ascii_lowercase();

    for node in fs_reader.files() {
        let p_str = node.fullpath.to_string_lossy();
        let p = p_str.trim_start_matches('/');

        // Match files related to this package name
        let matches = if image.distro == DistroType::ArchLinux {
            p.contains(&format!("/{}/", target_lower))
                || p.ends_with(&format!("/{}", target_lower))
                || p.contains(&target_lower)
        } else {
            p.contains(&format!("/{}/", target_lower)) || p.contains(&target_lower)
        };

        if matches {
            if let backhand::InnerNode::File(ref f) = node.inner {
                let out_path = dest_dir.join(sanitize_path(p));
                if let Some(parent) = out_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let mut f_reader = fs_reader.file(f).reader();
                let mut buf = Vec::new();
                if f_reader.read_to_end(&mut buf).is_ok() {
                    if fs::write(&out_path, &buf).is_ok() {
                        count += 1;
                    }
                }
            }
        }
    }

    log(&format!("Đã trích xuất thành công {} tệp cho '{}'.", count, target_name));
    Ok(count)
}

fn sanitize_path(rel_path: &str) -> PathBuf {
    let mut res = PathBuf::new();
    for part in rel_path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        let clean: String = part
            .chars()
            .map(|c| match c {
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
                _ => c,
            })
            .collect();
        res.push(clean);
    }
    res
}

pub fn build_iso<L: Fn(&str), P: Fn(i32, &str)>(
    plan: &Plan,
    dest_iso: &Path,
    log: L,
    progress: P,
) -> Result<(), String> {
    progress(5, "Khởi tạo môi trường đóng gói...");
    log(&format!(
        "Bắt đầu quy trình tạo ISO tùy chỉnh: {} -> {}",
        plan.image.path.display(),
        dest_iso.display()
    ));

    // Staging workspace
    let temp_dir = std::env::temp_dir().join(format!("isolite_stage_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).map_err(|e| format!("Không thể tạo thư mục tạm: {}", e))?;

    let staging_squash = temp_dir.join("filesystem_new.squashfs");

    let mut remove_paths = BTreeSet::new();
    let mut added_files = BTreeMap::new();

    let iso_file = File::open(&plan.image.path).map_err(|e| format!("Không thể mở ISO: {}", e))?;
    let iso_reader = BufReader::new(iso_file);
    let fs_reader = backhand::FilesystemReader::from_reader_with_offset(iso_reader, plan.image.squash_offset)
        .map_err(|e| format!("Không thể đọc SquashFS: {}", e))?;

    match plan.image.distro {
        DistroType::ArchLinux => {
            progress(10, "Đang nạp các gói Pacman bổ sung...");
            let mut added_arch_pkgs: Vec<(String, String)> = Vec::new();

            for pkg_path in &plan.debs {
                check_arch_package(pkg_path)?;
                log(&format!("Đang trích xuất gói Pacman: {}", pkg_path.display()));

                let ext = pkg_path.file_name().unwrap_or_default().to_string_lossy();
                let f = File::open(pkg_path).map_err(|e| e.to_string())?;
                let reader: Box<dyn Read> = if ext.ends_with(".zst") {
                    Box::new(zstd::Decoder::new(f).map_err(|e| e.to_string())?)
                } else {
                    Box::new(liblzma::read::XzDecoder::new(f))
                };

                let mut archive = tar::Archive::new(reader);
                let mut pkg_name = String::new();
                let mut pkg_ver = String::new();
                let mut pkg_desc = String::new();
                let mut pkg_url = String::new();
                let mut pkg_arch = String::new();
                let mut pkg_size = String::new();
                let mut build_date = String::new();
                let mut licenses = Vec::new();
                let mut depends = Vec::new();

                let mut files_list = Vec::new();

                for entry in archive.entries().map_err(|e| e.to_string())? {
                    let mut entry = entry.map_err(|e| e.to_string())?;
                    let path = entry.path().map_err(|e| e.to_string())?.to_path_buf();
                    let path_str = path.to_string_lossy().to_string();

                    if path_str == ".PKGINFO" || path_str == "./.PKGINFO" {
                        let mut content = String::new();
                        let _ = entry.read_to_string(&mut content);
                        for line in content.lines() {
                            let line = line.trim();
                            if line.starts_with('#') || line.is_empty() {
                                continue;
                            }
                            if let Some((k, v)) = line.split_once('=') {
                                let key = k.trim();
                                let val = v.trim();
                                match key {
                                    "pkgname" => pkg_name = val.to_string(),
                                    "pkgver" => pkg_ver = val.to_string(),
                                    "pkgdesc" => pkg_desc = val.to_string(),
                                    "url" => pkg_url = val.to_string(),
                                    "arch" => pkg_arch = val.to_string(),
                                    "size" => pkg_size = val.to_string(),
                                    "builddate" => build_date = val.to_string(),
                                    "license" => licenses.push(val.to_string()),
                                    "depend" => depends.push(val.to_string()),
                                    _ => {}
                                }
                            }
                        }
                    } else if !path_str.starts_with(".INSTALL")
                        && !path_str.starts_with(".BUILDINFO")
                        && !path_str.starts_with(".MTREE")
                    {
                        let mut buf = Vec::new();
                        let _ = entry.read_to_end(&mut buf);
                        let clean_path = path_str.trim_start_matches("./").trim_start_matches('/').to_string();
                        if !clean_path.is_empty() {
                            files_list.push(format!("/{}", clean_path));
                            added_files.insert(clean_path, buf);
                        }
                    }
                }

                if !pkg_name.is_empty() {
                    let local_pkg_dir = format!("var/lib/pacman/local/{}-{}", pkg_name, pkg_ver);

                    // Build desc file
                    let mut desc = format!("%NAME%\n{}\n\n%VERSION%\n{}\n\n", pkg_name, pkg_ver);
                    if !pkg_desc.is_empty() {
                        desc += &format!("%DESC%\n{}\n\n", pkg_desc);
                    }
                    if !pkg_url.is_empty() {
                        desc += &format!("%URL%\n{}\n\n", pkg_url);
                    }
                    if !pkg_arch.is_empty() {
                        desc += &format!("%ARCH%\n{}\n\n", pkg_arch);
                    }
                    if !build_date.is_empty() {
                        desc += &format!("%BUILDDATE%\n{}\n\n", build_date);
                    }
                    desc += &format!("%INSTALLDATE%\n{}\n\n", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs());
                    if !pkg_size.is_empty() {
                        desc += &format!("%SIZE%\n{}\n\n", pkg_size);
                    }
                    desc += "%REASON%\n0\n\n";
                    if !licenses.is_empty() {
                        desc += "%LICENSE%\n";
                        for l in &licenses {
                            desc += &format!("{}\n", l);
                        }
                        desc += "\n";
                    }
                    desc += "%VALIDATION%\nnone\n\n";
                    if !depends.is_empty() {
                        desc += "%DEPENDS%\n";
                        for d in &depends {
                            desc += &format!("{}\n", d);
                        }
                        desc += "\n";
                    }

                    added_files.insert(format!("{}/desc", local_pkg_dir), desc.into_bytes());

                    // Build files file
                    let mut files_content = "%FILES%\n".to_string();
                    for f in files_list {
                        files_content += &format!("{}\n", f.trim_start_matches('/'));
                    }
                    added_files.insert(format!("{}/files", local_pkg_dir), files_content.into_bytes());

                    added_arch_pkgs.push((pkg_name.clone(), pkg_ver.clone()));
                }
            }

            // Auto-configure Fcitx5 Vietnamese Input Method if integrated
            let has_fcitx5 = plan.debs.iter().any(|p| {
                let name = p.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
                name.contains("fcitx5") || name.contains("bamboo") || name.contains("unikey")
            });

            if has_fcitx5 {
                log("Tự động cấu hình bộ gõ Tiếng Việt Fcitx5 (Wayland / KDE Plasma 6)...");
                let env_content = "XMODIFIERS=@im=fcitx\n";
                added_files.insert("etc/environment".to_string(), env_content.as_bytes().to_vec());

                let autostart = "[Desktop Entry]\nName=Fcitx 5\nGenericName=Input Method\nExec=fcitx5 -d\nIcon=org.fcitx.Fcitx5\nTerminal=false\nType=Application\nCategories=System;Utility;\nStartupNotify=false\n";
                added_files.insert("etc/xdg/autostart/org.fcitx.Fcitx5.desktop".to_string(), autostart.as_bytes().to_vec());

                let profile = "[Groups/0]\nName=Default\nDefault Layout=us\nDefaultIM=bamboo\n\n[Groups/0/Items/0]\nName=keyboard-us\nLayout=\n\n[Groups/0/Items/1]\nName=bamboo\nLayout=\n\n[GroupOrder]\n0=Default\n";
                added_files.insert("etc/skel/.config/fcitx5/profile".to_string(), profile.as_bytes().to_vec());
                added_files.insert("root/.config/fcitx5/profile".to_string(), profile.as_bytes().to_vec());
            }

            // Removals for Arch
            progress(18, "Đang xử lý danh sách gỡ bỏ phần mềm...");
            for rem_pkg in &plan.remove {
                log(&format!("Lập danh sách gỡ bỏ tệp của gói: {}", rem_pkg));
                // Find local files record
                for node in fs_reader.files() {
                    let p_str = node.fullpath.to_string_lossy();
                    let p = p_str.trim_start_matches('/');
                    if p.starts_with("var/lib/pacman/local/") && p.ends_with("/files") {
                        let dir_name = p.strip_prefix("var/lib/pacman/local/").unwrap_or("").split('/').next().unwrap_or("");
                        let pkg_base = dir_name.rsplit_once('-').map(|(n, _)| n).unwrap_or(dir_name);
                        let pkg_base2 = pkg_base.rsplit_once('-').map(|(n, _)| n).unwrap_or(pkg_base);
                        if pkg_base2 == rem_pkg || dir_name.starts_with(&format!("{}-", rem_pkg)) {
                            // Read files list
                            if let backhand::InnerNode::File(ref f) = node.inner {
                                let mut r = fs_reader.file(f).reader();
                                let mut text = String::new();
                                if r.read_to_string(&mut text).is_ok() {
                                    for line in text.lines() {
                                        let line = line.trim();
                                        if !line.is_empty() && line != "%FILES%" {
                                            remove_paths.insert(line.trim_start_matches('/').to_string());
                                        }
                                    }
                                }
                            }
                            remove_paths.insert(format!("var/lib/pacman/local/{}", dir_name));
                        }
                    }
                }
            }
        }
        DistroType::DebianUbuntu => {
            progress(10, "Đang nạp các gói .deb bổ sung...");
            // Staging Debs
            for deb_path in &plan.debs {
                log(&format!("Đang trích xuất gói .deb: {}", deb_path.display()));
                let deb_stage = temp_dir.join(format!("deb_{}", deb_path.file_stem().unwrap().to_string_lossy()));
                let _ = fs::create_dir_all(&deb_stage);
                extract_deb_archives(deb_path, &deb_stage)?;

                // Find data.tar.* and control.tar.*
                for entry in fs::read_dir(&deb_stage).map_err(|e| e.to_string())? {
                    let entry = entry.map_err(|e| e.to_string())?;
                    let fn_name = entry.file_name().to_string_lossy().to_string();
                    if fn_name.starts_with("data.tar") {
                        let f = File::open(entry.path()).map_err(|e| e.to_string())?;
                        let reader: Box<dyn Read> = if fn_name.ends_with(".gz") {
                            Box::new(flate2::read::GzDecoder::new(f))
                        } else if fn_name.ends_with(".xz") {
                            Box::new(liblzma::read::XzDecoder::new(f))
                        } else if fn_name.ends_with(".zst") {
                            Box::new(zstd::Decoder::new(f).map_err(|e| e.to_string())?)
                        } else {
                            Box::new(f)
                        };

                        let mut tar = tar::Archive::new(reader);
                        for tentry in tar.entries().map_err(|e| e.to_string())? {
                            let mut tentry = tentry.map_err(|e| e.to_string())?;
                            let path = tentry.path().map_err(|e| e.to_string())?.to_path_buf();
                            let clean_path = path.to_string_lossy().trim_start_matches("./").trim_start_matches('/').to_string();
                            if !clean_path.is_empty() {
                                let mut buf = Vec::new();
                                let _ = tentry.read_to_end(&mut buf);
                                added_files.insert(clean_path, buf);
                            }
                        }
                    }
                }
            }

            // Removals for Debian/Ubuntu
            progress(18, "Đang xử lý danh sách gỡ bỏ phần mềm...");
            for rem_pkg in &plan.remove {
                log(&format!("Lập danh sách gỡ bỏ tệp của gói: {}", rem_pkg));
                let info_list = format!("var/lib/dpkg/info/{}.list", rem_pkg);
                for node in fs_reader.files() {
                    let p_str = node.fullpath.to_string_lossy();
                    let p = p_str.trim_start_matches('/');
                    if p == info_list || p.starts_with(&format!("var/lib/dpkg/info/{}:", rem_pkg)) {
                        if let backhand::InnerNode::File(ref f) = node.inner {
                            let mut r = fs_reader.file(f).reader();
                            let mut text = String::new();
                            if r.read_to_string(&mut text).is_ok() {
                                for line in text.lines() {
                                    let line = line.trim();
                                    if !line.is_empty() && line.starts_with('/') {
                                        remove_paths.insert(line.trim_start_matches('/').to_string());
                                    }
                                }
                            }
                        }
                    }
                }
                remove_paths.insert(info_list);
            }
        }
    }

    // Apply release rename inside squashfs
    if !plan.image.release.is_empty() && plan.image.release != plan.original_release {
        log(&format!("Cập nhật tên bản phát hành thành: {}", plan.image.release));
        let issue = format!("{} \\n \\l\n\n", plan.image.release);
        added_files.insert("etc/issue".to_string(), issue.into_bytes());

        // Update os-release
        if let Some(os_bytes) = read_squash_file(&plan.image.path, plan.image.squash_offset, "etc/os-release") {
            let cur = String::from_utf8_lossy(&os_bytes);
            let mut new_os = String::new();
            let mut replaced = false;
            for line in cur.lines() {
                if line.starts_with("PRETTY_NAME=") {
                    new_os += &format!("PRETTY_NAME=\"{}\"\n", plan.image.release);
                    replaced = true;
                } else {
                    new_os += &format!("{}\n", line);
                }
            }
            if !replaced {
                new_os += &format!("PRETTY_NAME=\"{}\"\n", plan.image.release);
            }
            added_files.insert("etc/os-release".to_string(), new_os.into_bytes());
        }
    }

    // Apply custom package folder replacements (Custom overlays)
    if !plan.replaced_folders.is_empty() {
        log("Đang nạp các tệp tùy chỉnh thay thế...");
        for (pkg_name, folder) in &plan.replaced_folders {
            if !folder.is_dir() {
                continue;
            }
            log(&format!(
                "Đang nạp tệp thay thế cho '{}' từ {}...",
                pkg_name,
                folder.display()
            ));

            fn collect_files(root: &Path, current: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
                if let Ok(entries) = fs::read_dir(current) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            collect_files(root, &path, out);
                        } else if path.is_file() {
                            if let Ok(rel) = path.strip_prefix(root) {
                                let rel_str = rel.to_string_lossy().replace('\\', "/");
                                let clean = rel_str.trim_start_matches('/').to_string();
                                if !clean.is_empty() {
                                    if let Ok(data) = fs::read(&path) {
                                        out.insert(clean, data);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            collect_files(folder, folder, &mut added_files);
        }
    }

    drop(fs_reader);

    // Rebuild SquashFS
    progress(25, "Đang tái cấu trúc và nén lại ảnh SquashFS...");
    rebuild_squashfs(
        &plan.image.path,
        plan.image.squash_offset,
        &staging_squash,
        &remove_paths,
        &added_files,
        plan.clean_flatpak,
        Some(&|pct, msg| progress(25 + pct / 2, msg)),
    )?;

    progress(75, "Đang chuẩn bị cập nhật cấu trúc bảng đĩa ISO...");
    log("Chuẩn bị danh sách ánh xạ thay thế trong ảnh ISO...");

    let mut replacements = Vec::new();

    match plan.image.distro {
        DistroType::ArchLinux => {
            // Replace /arch/x86_64/airootfs.sfs
            replacements.push(IsoReplacement {
                iso_path: "/arch/x86_64/airootfs.sfs".to_string(),
                local: staging_squash.clone(),
            });

            // Compute SHA-512 of new airootfs.sfs
            progress(80, "Đang tính mã băm SHA-512 của airootfs.sfs...");
            log("Tính mã băm SHA-512 cho ảnh airootfs.sfs mới...");
            let mut sfs_file = File::open(&staging_squash).map_err(|e| e.to_string())?;
            let mut hasher = Sha512::new();
            let mut buffer = [0u8; 1024 * 1024];
            loop {
                let n = sfs_file.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
            let hash_hex = format!("{:x}", hasher.finalize());
            let sha512_text = format!("{}  airootfs.sfs\n", hash_hex);
            let sha_file = temp_dir.join("airootfs.sha512");
            fs::write(&sha_file, sha512_text).map_err(|e| e.to_string())?;

            replacements.push(IsoReplacement {
                iso_path: "/arch/x86_64/airootfs.sha512".to_string(),
                local: sha_file,
            });

            // Update /arch/version if release changed
            if !plan.image.release.is_empty() && plan.image.release != plan.original_release {
                let ver_file = temp_dir.join("version");
                fs::write(&ver_file, format!("{}\n", plan.image.release)).map_err(|e| e.to_string())?;
                replacements.push(IsoReplacement {
                    iso_path: "/arch/version".to_string(),
                    local: ver_file,
                });
            }
        }
        DistroType::DebianUbuntu => {
            let squash_iso_path = if plan.image.squash_iso_path.is_empty() {
                "/casper/filesystem.squashfs"
            } else {
                &plan.image.squash_iso_path
            };

            replacements.push(IsoReplacement {
                iso_path: squash_iso_path.to_string(),
                local: staging_squash.clone(),
            });

            // Update filesystem.size
            if let Ok(meta) = fs::metadata(&staging_squash) {
                let size_file = temp_dir.join("filesystem.size");
                fs::write(&size_file, format!("{}\n", meta.len())).map_err(|e| e.to_string())?;
                let iso_size_path = if squash_iso_path.starts_with("/live") {
                    "/live/filesystem.size"
                } else {
                    "/casper/filesystem.size"
                };
                replacements.push(IsoReplacement {
                    iso_path: iso_size_path.to_string(),
                    local: size_file,
                });
            }

            // Update .disk/info
            if !plan.image.release.is_empty() && plan.image.release != plan.original_release {
                let disk_info = temp_dir.join("disk_info");
                fs::write(&disk_info, format!("{}\n", plan.image.release)).map_err(|e| e.to_string())?;
                replacements.push(IsoReplacement {
                    iso_path: "/.disk/info".to_string(),
                    local: disk_info,
                });
            }
        }
    }

    // Repack the ISO using in-place repack engine
    progress(85, "Đang ghi ảnh đĩa ISO bootable hoàn chỉnh...");
    log("Đang đóng gói lại ISO (Cập nhật MBR, GPT, El Torito, Joliet, Rock Ridge)...");

    repack_iso(
        &plan.image.path,
        dest_iso,
        &replacements,
        &[],
        Some(&|pct, msg| progress(85 + pct / 7, msg)),
    )?;

    progress(100, "Hoàn tất tạo ISO tùy chỉnh!");
    log(&format!("🎉 Tạo ISO tùy chỉnh thành công tại: {}", dest_iso.display()));

    // Clean up staging folder
    let _ = fs::remove_dir_all(&temp_dir);

    Ok(())
}
