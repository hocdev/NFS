use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufReader, BufWriter, Cursor};
use std::path::{Path, PathBuf};
use backhand::{FilesystemReader, FilesystemWriter, NodeHeader};

pub fn rebuild_squashfs(
    iso_path: &Path,
    offset: u64,
    output_path: &Path,
    remove_paths: &BTreeSet<String>,
    added_files: &BTreeMap<String, Vec<u8>>,
    clean_flatpak: bool,
    progress: Option<&dyn Fn(i32, &str)>,
) -> Result<(), String> {
    if let Some(cb) = progress {
        cb(10, "Đang đọc cấu trúc SquashFS gốc...");
    }

    let file = File::open(iso_path).map_err(|e| format!("Cannot open ISO file: {}", e))?;
    let reader = BufReader::new(file);
    let mut fs_reader = FilesystemReader::from_reader_with_offset(reader, offset)
        .map_err(|e| format!("Cannot parse SquashFS: {}", e))?;

    if let Some(cb) = progress {
        cb(25, "Đang áp dụng danh sách gỡ bỏ và dọn Flatpak...");
    }

    // Filter nodes directly from reader before constructing writer
    fs_reader.root.nodes.retain(|node| {
        let p_str = node.fullpath.to_string_lossy();
        let p = p_str.trim_start_matches('/');

        if clean_flatpak {
            if p.starts_with("var/lib/flatpak/app")
                || p.starts_with("var/lib/flatpak/runtime")
                || p.starts_with("var/lib/flatpak/exports")
                || p.starts_with("var/lib/flatpak/repo/objects")
            {
                return false;
            }
        }

        for rem in remove_paths {
            let rem_norm = rem.trim_start_matches('/');
            if p == rem_norm || p.starts_with(&format!("{}/", rem_norm)) {
                return false;
            }
        }

        for added in added_files.keys() {
            let add_norm = added.trim_start_matches('/');
            if p == add_norm {
                return false;
            }
        }

        true
    });

    if let Some(cb) = progress {
        cb(35, "Đang khởi tạo trình ghi ảnh SquashFS...");
    }

    let mut writer = FilesystemWriter::from_fs_reader(&fs_reader)
        .map_err(|e| format!("Cannot create FilesystemWriter: {}", e))?;

    if let Some(cb) = progress {
        cb(45, "Đang nạp các tệp và cấu hình mới vào hệ thống...");
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32;

    for (rel_path, data) in added_files {
        let clean_path = PathBuf::from(rel_path.trim_start_matches('/'));
        let header = NodeHeader {
            permissions: 0o644,
            uid: 0,
            gid: 0,
            mtime: now,
        };
        if let Some(parent) = clean_path.parent() {
            if !parent.as_os_str().is_empty() {
                let dir_header = NodeHeader {
                    permissions: 0o755,
                    uid: 0,
                    gid: 0,
                    mtime: now,
                };
                let _ = writer.push_dir_all(parent, dir_header);
            }
        }
        writer
            .push_file(Cursor::new(data.clone()), &clean_path, header)
            .map_err(|e| format!("Cannot push file {}: {}", rel_path, e))?;
    }

    if let Some(cb) = progress {
        cb(55, "Đang nén và ghi tệp SquashFS mới...");
    }

    let out_file = File::create(output_path)
        .map_err(|e| format!("Cannot create output SquashFS: {}", e))?;
    let mut buf_writer = BufWriter::with_capacity(4 * 1024 * 1024, out_file);

    writer
        .write(&mut buf_writer)
        .map_err(|e| format!("Cannot write SquashFS: {}", e))?;

    if let Some(cb) = progress {
        cb(72, "Hoàn tất tạo ảnh SquashFS.");
    }

    Ok(())
}
