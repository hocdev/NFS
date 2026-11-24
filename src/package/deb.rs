use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub fn check_deb(path: &Path) -> Result<(), String> {
    if !path.is_file() {
        return Err("Package file was not found.".to_string());
    }
    if !path.extension().map_or(false, |ext| ext.eq_ignore_ascii_case("deb")) {
        return Err("Choose a Linux .deb package.".to_string());
    }

    let mut file = File::open(path).map_err(|e| format!("Cannot open package file: {}", e))?;
    let mut magic = [0u8; 8];
    file.read_exact(&mut magic).map_err(|e| format!("Cannot read package header: {}", e))?;
    if &magic != b"!<arch>\n" {
        return Err(format!("Invalid .deb archive: {}", path.display()));
    }

    let file_len = file.metadata().map_err(|e| e.to_string())?.len();
    let mut remaining = file_len - 8;
    let mut version = false;
    let mut control = false;
    let mut data = false;

    let mut header = [0u8; 60];
    while remaining >= 60 {
        file.read_exact(&mut header).map_err(|e| format!("Invalid .deb member: {}", e))?;
        if header[58] != b'`' || header[59] != b'\n' {
            return Err("Invalid .deb member header.".to_string());
        }
        remaining -= 60;

        let size_str = std::str::from_utf8(&header[48..58]).unwrap_or("").trim();
        let size: u64 = size_str.parse().map_err(|_| "Invalid .deb member size.")?;
        if size > remaining || (size % 2 != 0 && size == remaining) {
            return Err("Invalid .deb member size.".to_string());
        }

        let name = std::str::from_utf8(&header[0..16])
            .unwrap_or("")
            .trim_end_matches(&[' ', '/'][..]);

        if name == "debian-binary" && size == 4 {
            let mut text = [0u8; 4];
            file.read_exact(&mut text).map_err(|e| e.to_string())?;
            version = &text == b"2.0\n";
        } else {
            if name == "control.tar" || name.starts_with("control.tar.") {
                control = true;
            }
            if name == "data.tar" || name.starts_with("data.tar.") {
                data = true;
            }
            file.seek(SeekFrom::Current((size + size % 2) as i64)).map_err(|e| e.to_string())?;
        }
        remaining -= size + size % 2;
    }

    if remaining != 0 || !version || !control || !data {
        return Err(format!("Incomplete .deb archive: {}", path.display()));
    }

    Ok(())
}

pub fn extract_deb_archives(package: &Path, folder: &Path) -> Result<(), String> {
    let mut input = File::open(package).map_err(|e| format!("Cannot open Debian package: {}", e))?;
    let mut magic = [0u8; 8];
    input.read_exact(&mut magic).map_err(|e| e.to_string())?;
    if &magic != b"!<arch>\n" {
        return Err("Invalid Debian package archive.".to_string());
    }

    let file_len = input.metadata().map_err(|e| e.to_string())?.len();
    let mut remaining = file_len - 8;

    let mut header = [0u8; 60];
    while remaining >= 60 {
        input.read_exact(&mut header).map_err(|e| e.to_string())?;
        if header[58] != b'`' || header[59] != b'\n' {
            return Err("Invalid Debian package member header.".to_string());
        }
        remaining -= 60;

        let size_str = std::str::from_utf8(&header[48..58]).unwrap_or("").trim();
        let size: u64 = size_str.parse().map_err(|_| "Invalid Debian package member size.")?;
        if size > remaining {
            return Err("Truncated Debian package member.".to_string());
        }

        let name = std::str::from_utf8(&header[0..16])
            .unwrap_or("")
            .trim_end_matches(&[' ', '/'][..]);

        if name == "data.tar"
            || name == "control.tar"
            || name.starts_with("data.tar.")
            || name.starts_with("control.tar.")
        {
            let target = folder.join(name);
            let mut out = File::create(&target).map_err(|e| format!("Cannot create {}: {}", target.display(), e))?;
            let mut buf = vec![0u8; 65536];
            let mut copied = 0u64;
            while copied < size {
                let chunk = std::cmp::min(buf.len() as u64, size - copied) as usize;
                input.read_exact(&mut buf[..chunk]).map_err(|e| e.to_string())?;
                out.write_all(&buf[..chunk]).map_err(|e| e.to_string())?;
                copied += chunk as u64;
            }
        } else {
            input.seek(SeekFrom::Current(size as i64)).map_err(|e| e.to_string())?;
        }

        if size % 2 != 0 {
            input.seek(SeekFrom::Current(1)).map_err(|e| e.to_string())?;
            remaining -= 1;
        }
        remaining -= size;
    }

    Ok(())
}
