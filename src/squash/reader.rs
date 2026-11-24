use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use backhand::{FilesystemReader, InnerNode};

pub fn read_squash_file(iso_path: &Path, offset: u64, path: &str) -> Option<Vec<u8>> {
    let file = File::open(iso_path).ok()?;
    let reader = BufReader::new(file);
    let fs = FilesystemReader::from_reader_with_offset(reader, offset).ok()?;

    let normalized = path.trim_start_matches('/');
    let target = PathBuf::from(normalized);

    for node in fs.files() {
        let node_path = node.fullpath.strip_prefix("/").unwrap_or(&node.fullpath);
        if node_path == target {
            if let InnerNode::File(ref file_entry) = node.inner {
                let mut reader = fs.file(file_entry).reader();
                let mut buf = Vec::new();
                if reader.read_to_end(&mut buf).is_ok() {
                    return Some(buf);
                }
            }
        }
    }
    None
}

pub fn detect_flatpak(iso_path: &Path, offset: u64) -> (bool, u64) {
    let file = match File::open(iso_path) {
        Ok(f) => f,
        Err(_) => return (false, 0),
    };
    let reader = BufReader::new(file);
    let fs = match FilesystemReader::from_reader_with_offset(reader, offset) {
        Ok(f) => f,
        Err(_) => return (false, 0),
    };

    let mut has_flatpak = false;
    let mut total_bytes = 0u64;

    for node in fs.files() {
        let p_str = node.fullpath.to_string_lossy();
        let p = p_str.trim_start_matches('/');
        if p.starts_with("var/lib/flatpak/app") || p.starts_with("var/lib/flatpak/runtime") {
            has_flatpak = true;
        }
        if p.starts_with("var/lib/flatpak") {
            if let InnerNode::File(ref f) = node.inner {
                total_bytes += f.file_len() as u64;
            }
        }
    }

    if has_flatpak && total_bytes == 0 {
        total_bytes = 1;
    }

    (has_flatpak, total_bytes)
}

pub fn visit_pacman_descriptions<F: FnMut(&str)>(iso_path: &Path, offset: u64, mut visit: F) {
    let file = match File::open(iso_path) {
        Ok(f) => f,
        Err(_) => return,
    };
    let reader = BufReader::new(file);
    let fs = match FilesystemReader::from_reader_with_offset(reader, offset) {
        Ok(f) => f,
        Err(_) => return,
    };

    for node in fs.files() {
        let p_str = node.fullpath.to_string_lossy();
        let p = p_str.trim_start_matches('/');
        if p.starts_with("var/lib/pacman/local/") && p.ends_with("/desc") {
            if let InnerNode::File(ref f) = node.inner {
                let mut reader = fs.file(f).reader();
                let mut content = String::new();
                if reader.read_to_string(&mut content).is_ok() {
                    visit(&content);
                }
            }
        }
    }
}
