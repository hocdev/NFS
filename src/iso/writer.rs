use crate::types::IsoReplacement;
use crate::xz::Hasher;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub const ISO_SECTOR: u64 = 2048;

pub fn read_iso_32(p: &[u8]) -> u32 {
    u32::from_le_bytes([p[0], p[1], p[2], p[3]])
}

pub fn read_iso_64(p: &[u8]) -> u64 {
    u64::from_le_bytes([p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7]])
}

pub fn write_iso_le(p: &mut [u8], value: u64, size: usize) {
    for i in 0..size {
        p[i] = ((value >> (8 * i)) & 0xff) as u8;
    }
}

pub fn write_iso_both(p: &mut [u8], value: u32) {
    write_iso_le(&mut p[0..4], value as u64, 4);
    let be = value.to_be_bytes();
    p[4..8].copy_from_slice(&be);
}

#[derive(Debug, Clone, Copy)]
pub struct Record {
    pub sector: u64,
    pub offset: usize,
    pub base: u64,
}

#[derive(Debug, Default)]
struct Edit {
    local: std::path::PathBuf,
    entries: Vec<Vec<Record>>,
}

#[derive(Debug, Clone)]
struct Extent {
    start: u64,
    end: u64,
    path: String,
    record: Record,
    #[allow(dead_code)]
    movable: bool,
}

fn read_at(file: &mut File, offset: u64, buf: &mut [u8]) -> Result<(), String> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    file.read_exact(buf).map_err(|e| e.to_string())?;
    Ok(())
}

fn write_at(file: &mut File, offset: u64, buf: &[u8]) -> Result<(), String> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    file.write_all(buf).map_err(|e| e.to_string())?;
    Ok(())
}

fn scan_dir(
    base: u64,
    sector: u64,
    bytes: u32,
    joliet: bool,
    path: &str,
    visited: &mut BTreeSet<(u64, u64)>,
    directories: &mut BTreeMap<u64, [u8; ISO_SECTOR as usize]>,
    extents: &mut Vec<Extent>,
    edits: &mut BTreeMap<String, Edit>,
    input: &mut File,
) -> Result<(), String> {
    if !visited.insert((base, sector)) {
        return Ok(());
    }

    let mut pos = 0u64;
    let mut pending_entry: Option<usize> = None;

    while pos < bytes as u64 {
        let lba = sector + pos / ISO_SECTOR;
        let mut block = [0u8; ISO_SECTOR as usize];
        read_at(input, lba * ISO_SECTOR, &mut block)?;
        directories.entry(lba).or_insert(block);
        let limit = std::cmp::min(ISO_SECTOR, bytes as u64 - pos) as usize;

        let mut i = 0;
        while i < limit && directories.get(&lba).unwrap()[i] != 0 {
            let block_ref = directories.get(&lba).unwrap();
            let length = block_ref[i] as usize;
            if length < 34 || length > limit - i {
                return Err("Invalid ISO directory record.".to_string());
            }
            let name_size = block_ref[i + 32] as usize;
            if name_size == 0 || 33 + name_size > length {
                return Err("Invalid ISO directory record.".to_string());
            }
            let offset = i;
            let rec = &block_ref[i..i + length];
            i += length;

            if name_size == 1 && rec[33] < 2 {
                continue;
            }

            let mut name = String::new();
            if joliet {
                let mut j = 0;
                while j + 1 < name_size {
                    let c = if rec[33 + j] != 0 {
                        '?'
                    } else {
                        rec[34 + j] as char
                    };
                    name.push(c);
                    j += 2;
                }
            } else {
                name = String::from_utf8_lossy(&rec[33..33 + name_size]).to_string();
            }

            if let Some(version_idx) = name.find(';') {
                name.truncate(version_idx);
            }
            if name.ends_with('.') {
                name.pop();
            }
            name = name.to_ascii_lowercase();

            if !joliet {
                let mut j = 33 + name_size + if name_size % 2 == 0 { 1 } else { 0 };
                while j + 4 <= length {
                    let field = &rec[j..];
                    let flen = field[2] as usize;
                    if flen < 4 || j + flen > length {
                        break;
                    }
                    if field[0] == b'N' && field[1] == b'M' && flen >= 5 && field[4] == 0 {
                        name = String::from_utf8_lossy(&field[5..flen]).to_string();
                        break;
                    }
                    j += flen;
                }
            }

            let full_path = format!("{}/{}", path, name);
            let start = base + read_iso_32(&rec[2..6]) as u64;
            let file_bytes = read_iso_32(&rec[10..14]);
            let blocks = (file_bytes as u64 + ISO_SECTOR - 1) / ISO_SECTOR;

            if blocks != 0 {
                let movable =
                    (rec[25] & (2 | 128)) == 0 && rec[1] == 0 && rec[26] == 0 && rec[27] == 0;
                extents.push(Extent {
                    start,
                    end: start + rec[1] as u64 + blocks,
                    path: full_path.clone(),
                    record: Record {
                        sector: lba,
                        offset,
                        base,
                    },
                    movable,
                });
            }

            if (rec[25] & 2) != 0 {
                scan_dir(
                    base,
                    start + rec[1] as u64,
                    file_bytes,
                    joliet,
                    &full_path,
                    visited,
                    directories,
                    extents,
                    edits,
                    input,
                )?;
            } else if let Some(edit) = edits.get_mut(&full_path) {
                if pending_entry.is_none() {
                    edit.entries.push(Vec::new());
                    pending_entry = Some(edit.entries.len() - 1);
                }
                let p_idx = pending_entry.unwrap();
                edit.entries[p_idx].push(Record {
                    sector: lba,
                    offset,
                    base,
                });
                if (rec[25] & 128) == 0 {
                    pending_entry = None;
                }
            } else if pending_entry.is_some() {
                return Err("Invalid ISO multi-extent file.".to_string());
            }
        }
        pos += ISO_SECTOR;
    }

    if pending_entry.is_some() {
        return Err("Incomplete ISO multi-extent file.".to_string());
    }

    Ok(())
}

pub fn repack_iso(
    source: &Path,
    target: &Path,
    replacements: &[IsoReplacement],
    removals: &[String],
    progress: Option<&dyn Fn(i32, &str)>,
) -> Result<(), String> {
    let source_meta =
        fs::metadata(source).map_err(|e| format!("Cannot inspect source ISO: {}", e))?;
    let source_size = source_meta.len();
    if source_size % ISO_SECTOR != 0 {
        return Err("The source ISO is not sector aligned.".to_string());
    }
    let source_end = source_size / ISO_SECTOR;

    let mut edits: BTreeMap<String, Edit> = BTreeMap::new();
    for item in replacements {
        edits.entry(item.iso_path.clone()).or_default().local = item.local.clone();
    }
    for name in removals {
        edits.entry(name.clone()).or_default();
    }

    let mut input = File::open(source).map_err(|e| format!("Cannot open source ISO: {}", e))?;

    let mut directories: BTreeMap<u64, [u8; ISO_SECTOR as usize]> = BTreeMap::new();
    let mut descriptors: BTreeMap<u64, (u64, [u8; ISO_SECTOR as usize])> = BTreeMap::new();
    let mut extents: Vec<Extent> = Vec::new();
    let mut boot_sectors: BTreeSet<u64> = BTreeSet::new();
    let mut visited: BTreeSet<(u64, u64)> = BTreeSet::new();

    let scan_volume = |base: u64,
                       visited: &mut BTreeSet<(u64, u64)>,
                       directories: &mut BTreeMap<u64, [u8; ISO_SECTOR as usize]>,
                       descriptors: &mut BTreeMap<u64, (u64, [u8; ISO_SECTOR as usize])>,
                       extents: &mut Vec<Extent>,
                       boot_sectors: &mut BTreeSet<u64>,
                       edits: &mut BTreeMap<String, Edit>,
                       input: &mut File|
     -> Result<(), String> {
        let max_lba = std::cmp::min(base + 64, source_end);
        for lba in base + 16..max_lba {
            let mut block = [0u8; ISO_SECTOR as usize];
            read_at(input, lba * ISO_SECTOR, &mut block)?;
            if &block[1..6] != b"CD001" || block[6] != 1 || block[0] == 255 {
                break;
            }
            if block[0] == 0 && &block[7..30] == b"EL TORITO SPECIFICATION" {
                let catalog = base + read_iso_32(&block[71..75]) as u64;
                let mut entries = [0u8; ISO_SECTOR as usize];
                read_at(input, catalog * ISO_SECTOR, &mut entries)?;
                boot_sectors.insert(catalog);
                for p in (32..entries.len()).step_by(32) {
                    if entries[p] == 0x88 {
                        boot_sectors.insert(base + read_iso_32(&entries[p + 8..p + 12]) as u64);
                    }
                }
            }
            let joliet = block[0] == 2 && block[88] == b'%' && block[89] == b'/';
            if block[0] != 1 && !joliet {
                continue;
            }
            if block[128] != 0 || block[129] != 8 || block[156] < 34 {
                return Err("Unsupported ISO volume descriptor.".to_string());
            }
            descriptors.insert(lba, (base, block));
            let root_sector = base + read_iso_32(&block[158..162]) as u64 + block[157] as u64;
            let root_bytes = read_iso_32(&block[166..170]);
            scan_dir(
                base,
                root_sector,
                root_bytes,
                joliet,
                "",
                visited,
                directories,
                extents,
                edits,
                input,
            )?;
        }
        Ok(())
    };

    scan_volume(
        0,
        &mut visited,
        &mut directories,
        &mut descriptors,
        &mut extents,
        &mut boot_sectors,
        &mut edits,
        &mut input,
    )?;

    if descriptors.is_empty() {
        return Err("Cannot find ISO volume descriptors.".to_string());
    }

    if source_end > 32 && !descriptors.contains_key(&32) {
        let mut block = [0u8; ISO_SECTOR as usize];
        read_at(&mut input, 32 * ISO_SECTOR, &mut block)?;
        if block[0] == 1 && &block[1..6] == b"CD001" {
            scan_volume(
                16,
                &mut visited,
                &mut directories,
                &mut descriptors,
                &mut extents,
                &mut boot_sectors,
                &mut edits,
                &mut input,
            )?;
        }
    }

    if let Some(cb) = progress {
        cb(75, "Đang sao chép cấu trúc tệp ISO gốc...");
    }

    fs::copy(source, target).map_err(|e| format!("Cannot copy source ISO: {}", e))?;

    let mut output = OpenOptions::new()
        .read(true)
        .write(true)
        .open(target)
        .map_err(|e| format!("Cannot open target ISO for writing: {}", e))?;

    let mut end = source_end;
    const EXTENT_LIMIT: u64 = 0xfffff800;
    let mut erased: BTreeMap<u64, BTreeSet<usize>> = BTreeMap::new();

    let existing_range = |path: &str,
                          edit: &Edit,
                          dirs: &BTreeMap<u64, [u8; ISO_SECTOR as usize]>,
                          ext_list: &[Extent]|
     -> (u64, u64) {
        let mut range = (0u64, 0u64);
        for entry in &edit.entries {
            let mut start = 0u64;
            let mut sectors = 0u64;
            for record in entry {
                let rec = &dirs.get(&record.sector).unwrap()[record.offset..];
                let lba = record.base + read_iso_32(&rec[2..6]) as u64;
                let bytes = read_iso_32(&rec[10..14]);
                let blocks = (bytes as u64 + ISO_SECTOR - 1) / ISO_SECTOR;
                if rec[1] != 0
                    || rec[26] != 0
                    || rec[27] != 0
                    || blocks == 0
                    || lba >= source_end
                    || blocks > source_end - lba
                {
                    return (0, 0);
                }
                if sectors == 0 {
                    start = lba;
                }
                if lba != start + sectors {
                    return (0, 0);
                }
                sectors += blocks;
            }
            if range.1 == 0 {
                range = (start, sectors);
            }
            if range != (start, sectors) {
                return (0, 0);
            }
        }
        for extent in ext_list {
            if extent.path != path && extent.start < range.0 + range.1 && extent.end > range.0 {
                return (0, 0);
            }
        }
        range
    };

    let mut total_repl_bytes = 0u64;
    for edit in edits.values() {
        if !edit.local.as_os_str().is_empty() {
            if let Ok(meta) = fs::metadata(&edit.local) {
                total_repl_bytes += meta.len();
            }
        }
    }
    let mut written_repl = 0u64;
    let mut last_progress = -1;

    for (path, edit) in &edits {
        if edit.local.as_os_str().is_empty() {
            for entry in &edit.entries {
                for record in entry {
                    erased
                        .entry(record.sector)
                        .or_default()
                        .insert(record.offset);
                }
            }
            continue;
        }

        if edit.entries.is_empty() {
            return Err(format!("Cannot locate ISO file to replace: {}", path));
        }

        let size = fs::metadata(&edit.local)
            .map_err(|e| {
                format!(
                    "Cannot read replacement file {}: {}",
                    edit.local.display(),
                    e
                )
            })?
            .len();
        let sections = std::cmp::max(1, (size + EXTENT_LIMIT - 1) / EXTENT_LIMIT);
        let sectors = (size + ISO_SECTOR - 1) / ISO_SECTOR;

        let (old_start, old_sectors) = existing_range(path, edit, &directories, &extents);

        let mut reuse = false;
        if old_sectors > 0 {
            if sectors <= old_sectors {
                reuse = true;
            }
        }

        let start = if reuse { old_start } else { end };
        if start + sectors > u32::MAX as u64 {
            return Err("The output ISO exceeds the ISO9660 volume size limit.".to_string());
        }

        for entry in &edit.entries {
            if entry.len() < sections as usize {
                return Err(format!(
                    "The ISO file has too few extents for its replacement: {}",
                    path
                ));
            }
            let mut position = 0u64;
            for (i, record) in entry.iter().enumerate() {
                if i >= sections as usize {
                    erased
                        .entry(record.sector)
                        .or_default()
                        .insert(record.offset);
                    continue;
                }
                let block = directories.get_mut(&record.sector).unwrap();
                let rec = &mut block[record.offset..];
                let length = std::cmp::min(size - position, EXTENT_LIMIT);
                rec[1] = 0;
                write_iso_both(
                    &mut rec[2..10],
                    (start + position / ISO_SECTOR - record.base) as u32,
                );
                write_iso_both(&mut rec[10..18], length as u32);
                let flag = if i + 1 < sections as usize { 128 } else { 0 };
                rec[25] = (rec[25] & !128) | flag;
                position += length;
            }
        }

        let mut replacement = File::open(&edit.local)
            .map_err(|e| format!("Cannot open replacement {}: {}", edit.local.display(), e))?;
        let mut buffer = vec![0u8; 1024 * 1024];
        let mut pos = 0u64;
        while pos < size {
            let count = std::cmp::min(size - pos, buffer.len() as u64) as usize;
            replacement
                .read_exact(&mut buffer[..count])
                .map_err(|e| e.to_string())?;
            write_at(&mut output, start * ISO_SECTOR + pos, &buffer[..count])?;
            pos += count as u64;
            written_repl += count as u64;
            if let Some(cb) = progress {
                if total_repl_bytes > 0 {
                    let p = 85 + ((written_repl * 10) / total_repl_bytes) as i32;
                    if p != last_progress {
                        cb(p, "Đang ghi ảnh hệ thống mới vào ISO...");
                        last_progress = p;
                    }
                }
            }
        }

        let pad = (sectors * ISO_SECTOR - size) as usize;
        if pad > 0 {
            let zeros = vec![0u8; pad];
            write_at(&mut output, start * ISO_SECTOR + size, &zeros)?;
        }

        if old_sectors > 0 && !reuse {
            let zeros = vec![0u8; 1024 * 1024];
            let mut p = 0u64;
            let limit = old_sectors * ISO_SECTOR;
            while p < limit {
                let count = std::cmp::min(limit - p, zeros.len() as u64) as usize;
                write_at(&mut output, old_start * ISO_SECTOR + p, &zeros[..count])?;
                p += count as u64;
            }
        }

        if !reuse {
            end += sectors;
        }
    }

    // Compact erased records
    for (sector, offsets) in &erased {
        let block = directories.get_mut(sector).unwrap();
        let mut kept = 0usize;
        let mut pos = 0usize;
        while pos < block.len() && block[pos] != 0 {
            let length = block[pos] as usize;
            if length > block.len() - pos {
                return Err("Invalid ISO directory record.".to_string());
            }
            if !offsets.contains(&pos) {
                block.copy_within(pos..pos + length, kept);
                kept += length;
            }
            pos += length;
        }
        block[kept..].fill(0);
    }

    for (sector, block) in &directories {
        write_at(&mut output, sector * ISO_SECTOR, block)?;
    }

    if let Some(cb) = progress {
        cb(95, "Đang đồng bộ bảng phân vùng và boot record ISO...");
    }

    if end != source_end {
        let mut mbr = [0u8; 512];
        let mut gpt = [0u8; 512];
        read_at(&mut output, 0, &mut mbr)?;
        read_at(&mut output, 512, &mut gpt)?;

        if &gpt[0..8] == b"EFI PART" {
            let header_size = read_iso_32(&gpt[12..16]) as usize;
            let part_lba = read_iso_64(&gpt[72..80]);
            let part_bytes = read_iso_32(&gpt[80..84]) as u64 * read_iso_32(&gpt[84..88]) as u64;
            if header_size >= 92
                && header_size <= gpt.len()
                && part_bytes > 0
                && part_bytes <= 1024 * 1024
            {
                let mut parts = vec![0u8; part_bytes as usize];
                read_at(&mut output, part_lba * 512, &mut parts)?;
                let part_sectors = (part_bytes + 511) / 512;
                let backup_lba = end * 4 + part_sectors;
                let disk_end = (backup_lba + 4) / 4 * 4;
                let backup_header = disk_end - 1;
                let backup_parts = backup_header - part_sectors;

                let update_header = |hdr: &mut [u8], current: u64, alternate: u64, entries: u64| {
                    write_iso_le(&mut hdr[16..20], 0, 4);
                    write_iso_le(&mut hdr[24..32], current, 8);
                    write_iso_le(&mut hdr[32..40], alternate, 8);
                    write_iso_le(&mut hdr[48..56], backup_parts - 1, 8);
                    write_iso_le(&mut hdr[72..80], entries, 8);
                    let mut h = Hasher::new();
                    h.update(&hdr[..header_size]);
                    let crc = h.finalize();
                    write_iso_le(&mut hdr[16..20], crc as u64, 4);
                };

                update_header(&mut gpt, 1, backup_header, part_lba);
                write_at(&mut output, 512, &gpt)?;

                let mut backup_gpt = gpt;
                update_header(&mut backup_gpt, backup_header, 1, backup_parts);
                write_at(&mut output, backup_parts * 512, &parts)?;
                write_at(&mut output, backup_header * 512, &backup_gpt)?;

                if mbr[510] == 0x55 && mbr[511] == 0xaa {
                    for offset in (446..510).step_by(16) {
                        let part = &mut mbr[offset..offset + 16];
                        if part[4] == 0xee {
                            let part_start = read_iso_32(&part[8..12]) as u64;
                            let new_len = std::cmp::min(disk_end - part_start, u32::MAX as u64);
                            write_iso_le(&mut part[12..16], new_len, 4);
                        }
                    }
                    write_at(&mut output, 0, &mbr)?;
                }
            }
        }

        for (&sector, (base, mut block)) in &descriptors {
            write_iso_both(&mut block[80..88], (end - base) as u32);
            write_at(&mut output, sector * ISO_SECTOR, &block)?;
        }
    }

    Ok(())
}
