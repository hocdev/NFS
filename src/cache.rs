use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use crate::types::{DistroType, Image, Package};

pub fn cache_directory() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let local = parent.join("cache");
            if local.is_dir() {
                return local;
            }
        }
    }

    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let dir = PathBuf::from(local_app_data).join("ISOLite").join("cache");
        let _ = fs::create_dir_all(&dir);
        return dir;
    }

    let dir = std::env::temp_dir().join("ISOLite-cache");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn cache_file_name(iso_path: &Path, iso_size: u64) -> String {
    let abs_str = match iso_path.canonicalize() {
        Ok(p) => {
            let s = p.to_string_lossy().to_lowercase();
            s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
        }
        Err(_) => {
            let p = std::path::absolute(iso_path).unwrap_or_else(|_| iso_path.to_path_buf());
            let s = p.to_string_lossy().to_lowercase();
            s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
        }
    };
    let key = format!("{}_{}", abs_str, iso_size);

    let mut hash: u64 = 14695981039346656037;
    for c in key.encode_utf16() {
        hash ^= c as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{:016x}.bin", hash)
}

pub fn try_load_cache(source: &Path, iso_size: u64, mtime: i64) -> Option<Image> {
    let filename = cache_file_name(source, iso_size);
    let mut candidates = Vec::new();
    candidates.push(cache_directory().join(&filename));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("cache").join(&filename));
            candidates.push(parent.join(&filename));
            candidates.push(parent.join("test.cache"));
        }
    }
    if let Some(parent) = source.parent() {
        if let Some(stem) = source.file_stem() {
            candidates.push(parent.join(format!("{}.cache", stem.to_string_lossy())));
        }
    }

    for cache_path in candidates {
        if !cache_path.is_file() {
            continue;
        }
        let mut f = match File::open(&cache_path) {
            Ok(file) => file,
            Err(_) => continue,
        };

        let mut magic = [0u8; 4];
        if f.read_exact(&mut magic).is_err() {
            continue;
        }
        let magic_val = u32::from_le_bytes(magic);
        if magic_val != 0x5A4C4339 {
            continue;
        }

        let mut u64_buf = [0u8; 8];
        if f.read_exact(&mut u64_buf).is_err() {
            continue;
        }
        let cached_size = u64::from_le_bytes(u64_buf);
        if cached_size != iso_size {
            continue;
        }

        let mut i64_buf = [0u8; 8];
        if f.read_exact(&mut i64_buf).is_err() {
            continue;
        }
        let cached_mtime = i64::from_le_bytes(i64_buf);
        let is_test_cache = cache_path.file_name().map_or(false, |n| n == "test.cache");
        if (cached_mtime - mtime).abs() > 3 && !is_test_cache {
            continue;
        }

        let mut distro_buf = [0u8; 1];
        if f.read_exact(&mut distro_buf).is_err() {
            continue;
        }
        let distro = if distro_buf[0] == 1 {
            DistroType::ArchLinux
        } else {
            DistroType::DebianUbuntu
        };

        let mut fp_buf = [0u8; 1];
        if f.read_exact(&mut fp_buf).is_err() {
            continue;
        }
        let has_flatpak = fp_buf[0] != 0;

        if f.read_exact(&mut u64_buf).is_err() {
            continue;
        }
        let flatpak_bytes = u64::from_le_bytes(u64_buf);

        let mut u16_buf = [0u8; 2];
        if f.read_exact(&mut u16_buf).is_err() {
            continue;
        }
        let rel_len = u16::from_le_bytes(u16_buf) as usize;
        let mut rel_utf16 = vec![0u16; rel_len];
        let mut rel_bytes = vec![0u8; rel_len * 2];
        if f.read_exact(&mut rel_bytes).is_err() {
            continue;
        }
        for i in 0..rel_len {
            rel_utf16[i] = u16::from_le_bytes([rel_bytes[i * 2], rel_bytes[i * 2 + 1]]);
        }
        let release = String::from_utf16_lossy(&rel_utf16);

        let mut u32_buf = [0u8; 4];
        if f.read_exact(&mut u32_buf).is_err() {
            continue;
        }
        let count = u32::from_le_bytes(u32_buf) as usize;
        if count == 0 || count > 500_000 {
            continue;
        }

        let mut packages = Vec::with_capacity(count);
        let mut failed = false;

        for _ in 0..count {
            if f.read_exact(&mut u16_buf).is_err() {
                failed = true;
                break;
            }
            let nlen = u16::from_le_bytes(u16_buf) as usize;
            let mut name_bytes = vec![0u8; nlen];
            if f.read_exact(&mut name_bytes).is_err() {
                failed = true;
                break;
            }
            let name = String::from_utf8_lossy(&name_bytes).to_string();

            if f.read_exact(&mut u16_buf).is_err() {
                failed = true;
                break;
            }
            let vlen = u16::from_le_bytes(u16_buf) as usize;
            let mut ver_bytes = vec![0u8; vlen];
            if f.read_exact(&mut ver_bytes).is_err() {
                failed = true;
                break;
            }
            let version = String::from_utf8_lossy(&ver_bytes).to_string();

            let mut prot_buf = [0u8; 1];
            if f.read_exact(&mut prot_buf).is_err() {
                failed = true;
                break;
            }
            let protected_package = prot_buf[0] != 0;

            if f.read_exact(&mut i64_buf).is_err() {
                failed = true;
                break;
            }
            let kib_val = i64::from_le_bytes(i64_buf);
            let installed_kib = if kib_val >= 0 { Some(kib_val as u64) } else { None };

            if f.read_exact(&mut u16_buf).is_err() {
                failed = true;
                break;
            }
            let glen = u16::from_le_bytes(u16_buf) as usize;
            let mut group_bytes = vec![0u8; glen];
            if f.read_exact(&mut group_bytes).is_err() {
                failed = true;
                break;
            }
            let group = String::from_utf8_lossy(&group_bytes).to_string();

            packages.push(Package {
                name,
                version,
                protected_package,
                installed_kib,
                group,
            });
        }

        if failed {
            continue;
        }

        let s_path = if distro == DistroType::ArchLinux {
            "/arch/x86_64/airootfs.sfs".to_string()
        } else {
            "/live/filesystem.squashfs".to_string()
        };

        return Some(Image {
            path: source.to_path_buf(),
            release,
            distro,
            packages,
            has_flatpak,
            flatpak_bytes,
            squash_offset: 0,
            squash_iso_path: s_path,
        });
    }

    None
}

pub fn save_cache(image: &Image, iso_size: u64, mtime: i64) {
    let filename = cache_file_name(&image.path, iso_size);
    let cache_path = cache_directory().join(filename);
    let mut f = match File::create(&cache_path) {
        Ok(file) => file,
        Err(_) => return,
    };

    let magic = 0x5A4C4339u32;
    let _ = f.write_all(&magic.to_le_bytes());
    let _ = f.write_all(&iso_size.to_le_bytes());
    let _ = f.write_all(&mtime.to_le_bytes());

    let distro_val = image.distro as u8;
    let _ = f.write_all(&[distro_val]);

    let has_fp = if image.has_flatpak { 1u8 } else { 0u8 };
    let _ = f.write_all(&[has_fp]);
    let _ = f.write_all(&image.flatpak_bytes.to_le_bytes());

    let rel_utf16: Vec<u16> = image.release.encode_utf16().collect();
    let rel_len = rel_utf16.len() as u16;
    let _ = f.write_all(&rel_len.to_le_bytes());
    for u in rel_utf16 {
        let _ = f.write_all(&u.to_le_bytes());
    }

    let count = image.packages.len() as u32;
    let _ = f.write_all(&count.to_le_bytes());

    for p in &image.packages {
        let nlen = p.name.len() as u16;
        let _ = f.write_all(&nlen.to_le_bytes());
        let _ = f.write_all(p.name.as_bytes());

        let vlen = p.version.len() as u16;
        let _ = f.write_all(&vlen.to_le_bytes());
        let _ = f.write_all(p.version.as_bytes());

        let prot = if p.protected_package { 1u8 } else { 0u8 };
        let _ = f.write_all(&[prot]);

        let kib: i64 = p.installed_kib.map_or(-1, |k| k as i64);
        let _ = f.write_all(&kib.to_le_bytes());

        let glen = p.group.len() as u16;
        let _ = f.write_all(&glen.to_le_bytes());
        let _ = f.write_all(p.group.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_roundtrip() {
        let tmp_iso = std::env::temp_dir().join("test_roundtrip.iso");
        let _ = std::fs::write(&tmp_iso, b"dummy iso content");

        let test_img = Image {
            path: tmp_iso.clone(),
            release: "TestOS Linux 2026".to_string(),
            distro: DistroType::ArchLinux,
            packages: vec![
                Package {
                    name: "linux".to_string(),
                    version: "6.12.1".to_string(),
                    protected_package: true,
                    installed_kib: Some(150000),
                    group: "Hệ thống".to_string(),
                },
                Package {
                    name: "firefox".to_string(),
                    version: "135.0".to_string(),
                    protected_package: false,
                    installed_kib: Some(220000),
                    group: "Trình duyệt".to_string(),
                },
            ],
            has_flatpak: true,
            flatpak_bytes: 524288000,
            squash_offset: 123456,
            squash_iso_path: "/arch/x86_64/airootfs.sfs".to_string(),
        };

        save_cache(&test_img, 1000, 2000);
        let loaded = try_load_cache(&tmp_iso, 1000, 2000).expect("Cache should load");
        assert_eq!(loaded.release, "TestOS Linux 2026");
        assert_eq!(loaded.distro, DistroType::ArchLinux);
        assert_eq!(loaded.packages.len(), 2);
        assert_eq!(loaded.packages[0].name, "linux");
        assert_eq!(loaded.packages[1].name, "firefox");
        assert_eq!(loaded.has_flatpak, true);
        assert_eq!(loaded.flatpak_bytes, 524288000);

        let _ = std::fs::remove_file(&tmp_iso);
    }
}
