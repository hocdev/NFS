use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use crate::types::{DistroType, Image, Plan};

#[derive(Debug, Default, Clone)]
pub struct PacmanPackageInfo {
    pub name: String,
    pub version: String,
    pub arch: String,
    pub dependencies: Vec<String>,
    pub provides: Vec<String>,
}

pub fn check_arch_package(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err("Package file was not found.".to_string());
    }
    let name_str = path.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    let is_zst = name_str.ends_with(".pkg.tar.zst");
    let is_xz = name_str.ends_with(".pkg.tar.xz");
    if !is_zst && !is_xz {
        return Err("Choose an Arch Linux package (*.pkg.tar.zst or *.pkg.tar.xz).".to_string());
    }

    let mut file = File::open(path).map_err(|e| format!("Cannot open package file: {}", e))?;
    let mut header = [0u8; 6];
    file.read_exact(&mut header).map_err(|e| format!("Cannot read package header: {}", e))?;

    if is_zst {
        if header[0] != 0x28 || header[1] != 0xb5 || header[2] != 0x2f || header[3] != 0xfd {
            return Err(format!("Invalid Zstandard package archive: {}", path.display()));
        }
    } else if header[0] != 0xfd || header[1] != b'7' || header[2] != b'z' || header[3] != b'X' || header[4] != b'Z' {
        return Err(format!("Invalid XZ package archive: {}", path.display()));
    }

    Ok(())
}

pub fn read_pacman_package_info(path: &Path) -> Result<PacmanPackageInfo, String> {
    check_arch_package(path)?;
    let file = File::open(path).map_err(|e| e.to_string())?;

    let name_str = path.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    let reader: Box<dyn Read> = if name_str.ends_with(".pkg.tar.zst") {
        Box::new(zstd::Decoder::new(file).map_err(|e| format!("Cannot create Zstandard decoder: {}", e))?)
    } else {
        Box::new(liblzma::read::XzDecoder::new(file))
    };

    let mut archive = tar::Archive::new(reader);
    let mut info = PacmanPackageInfo::default();
    let mut found = false;

    for entry in archive.entries().map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let entry_path = entry.path().map_err(|e| e.to_string())?;
        if entry_path == Path::new(".PKGINFO") || entry_path == Path::new("./.PKGINFO") {
            let buf_reader = BufReader::new(entry);
            for line in buf_reader.lines() {
                let line = line.map_err(|e| e.to_string())?;
                let line = line.trim();
                if line.starts_with('#') || line.is_empty() {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    let key = k.trim();
                    let val = v.trim();
                    match key {
                        "pkgname" => info.name = val.to_string(),
                        "pkgver" => info.version = val.to_string(),
                        "arch" => info.arch = val.to_string(),
                        "depend" => info.dependencies.push(val.to_string()),
                        "provides" => info.provides.push(val.to_string()),
                        _ => {}
                    }
                }
            }
            found = true;
            break;
        }
    }

    if !found {
        return Err("Cannot locate .PKGINFO in package archive.".to_string());
    }

    Ok(info)
}

pub fn pacman_dependency_name(dep: &str) -> &str {
    if let Some(idx) = dep.find(&['<', '=', '>'][..]) {
        &dep[..idx]
    } else {
        dep
    }
}

pub fn check_package_for_image(path: &Path, image: &Image) -> Result<(), String> {
    let is_arch = image.distro == DistroType::ArchLinux;
    let name_str = path.file_name().unwrap_or_default().to_string_lossy().to_ascii_lowercase();
    let target = if image.release.is_empty() { "Linux" } else { &image.release };

    if is_arch && !name_str.ends_with(".pkg.tar.zst") && !name_str.ends_with(".pkg.tar.xz") {
        return Err(format!("Gói không tương thích với {}. ISO này cần gói .pkg.tar.zst hoặc .pkg.tar.xz.", target));
    }
    if !is_arch && !name_str.ends_with(".deb") {
        return Err(format!("Gói không tương thích với {}. ISO này cần gói .deb.", target));
    }

    if is_arch {
        let info = read_pacman_package_info(path)?;
        if info.arch.is_empty() {
            return Err("Không xác định được kiến trúc của gói từ tệp .PKGINFO.".to_string());
        }
        let valid_archs = ["any", "x86_64", "x86_64_v3", "x86_64_v4", "znver4", "znver5"];
        if !valid_archs.contains(&info.arch.as_str()) {
            return Err(format!(
                "Gói không tương thích với {} (x86_64). Kiến trúc của gói: {}.",
                target, info.arch
            ));
        }
    } else {
        crate::package::deb::check_deb(path)?;
    }

    Ok(())
}

pub fn missing_package_dependencies(plan: &Plan) -> String {
    if plan.image.distro != DistroType::ArchLinux || plan.debs.is_empty() {
        return String::new();
    }

    let mut added: BTreeMap<String, PacmanPackageInfo> = BTreeMap::new();
    for path in &plan.debs {
        if let Ok(info) = read_pacman_package_info(path) {
            added.insert(info.name.clone(), info);
        }
    }

    let mut available: BTreeSet<String> = BTreeSet::new();
    for package in &plan.image.packages {
        if !plan.remove.contains(&package.name) && !added.contains_key(&package.name) {
            available.insert(package.name.clone());
        }
    }

    for (name, info) in &added {
        available.insert(name.clone());
        for provide in &info.provides {
            available.insert(pacman_dependency_name(provide).to_string());
        }
    }

    let mut missing = String::new();
    for (name, info) in &added {
        let mut deps = String::new();
        for dependency in &info.dependencies {
            let dep_name = pacman_dependency_name(dependency);
            if available.contains(dep_name) {
                continue;
            }
            if !deps.is_empty() {
                deps.push_str(", ");
            }
            deps.push_str(dependency);
        }
        if !deps.is_empty() {
            missing.push_str(&format!("\n• {}: {}", name, deps));
        }
    }

    missing
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_real_pkg_tar_zst() {
        let pkg_path = Path::new("pkg/enchant-2.8.21-1-x86_64.pkg.tar.zst");
        if pkg_path.exists() {
            assert!(check_arch_package(pkg_path).is_ok());
            let info = read_pacman_package_info(pkg_path).expect("Failed to read package info");
            assert_eq!(info.name, "enchant");
            assert_eq!(info.arch, "x86_64");
            println!("Verified package: {} {}", info.name, info.version);
        }
    }
}
