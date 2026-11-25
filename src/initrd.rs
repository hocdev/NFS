use std::collections::BTreeMap;

const LZ4_LEGACY_MAGIC: [u8; 4] = [0x02, 0x21, 0x4c, 0x18];
const LZ4_LEGACY_CHUNK: usize = 8 * 1024 * 1024;
const MAX_INITRD_DECOMPRESSED: usize = 1024 * 1024 * 1024;

type Result<T> = std::result::Result<T, String>;

fn decode_lz4_block(src: &[u8], out: &mut Vec<u8>) -> Result<()> {
    let mut i = 0usize;
    let n = src.len();
    while i < n {
        let token = src[i];
        i += 1;
        let mut lit_len = (token >> 4) as usize;
        if lit_len == 15 {
            loop {
                let &b = src.get(i).ok_or("Truncated LZ4 literal length")?;
                i += 1;
                lit_len += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        let lit_end = i.checked_add(lit_len).ok_or("LZ4 literal overflow")?;
        if lit_end > n {
            return Err("Truncated LZ4 literal payload".into());
        }
        out.extend_from_slice(&src[i..lit_end]);
        i = lit_end;
        if i >= n {
            break;
        }
        if i + 2 > n {
            return Err("Truncated LZ4 match offset".into());
        }
        let offset = u16::from_le_bytes([src[i], src[i + 1]]) as usize;
        i += 2;
        if offset == 0 || offset > out.len() {
            return Err("Invalid LZ4 match offset".into());
        }
        let mut match_len = (token & 0x0f) as usize + 4;
        if (token & 0x0f) == 15 {
            loop {
                let &b = src.get(i).ok_or("Truncated LZ4 match length")?;
                i += 1;
                match_len += b as usize;
                if b != 255 {
                    break;
                }
            }
        }
        if out.len() + match_len > MAX_INITRD_DECOMPRESSED {
            return Err("Decompressed initrd exceeds limit".into());
        }
        let start = out.len() - offset;
        for k in 0..match_len {
            let b = out[start + k];
            out.push(b);
        }
    }
    Ok(())
}

/// Decode one or more concatenated LZ4_LEGACY frames (`0x184C2102`).
pub fn lz4_legacy_decode(input: &[u8]) -> Result<Vec<u8>> {
    if !input.starts_with(&LZ4_LEGACY_MAGIC) {
        return Err("Initrd không dùng định dạng LZ4 legacy (0x184C2102)".into());
    }
    let mut pos = 0usize;
    let mut out = Vec::new();
    while pos + 4 <= input.len() {
        if input[pos..pos + 4] == LZ4_LEGACY_MAGIC {
            pos += 4;
            continue;
        }
        let blen = u32::from_le_bytes(input[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        if blen == 0 || pos.checked_add(blen).is_none_or(|e| e > input.len()) {
            return Err(format!("Kích thước khối LZ4 không hợp lệ: {blen}"));
        }
        decode_lz4_block(&input[pos..pos + blen], &mut out)?;
        pos += blen;
    }
    if pos != input.len() {
        return Err("Dữ liệu thừa ở cuối luồng LZ4 initrd".into());
    }
    Ok(out)
}

fn push_lz4_len(out: &mut Vec<u8>, mut len: usize) {
    while len >= 255 {
        out.push(255);
        len -= 255;
    }
    out.push(len as u8);
}

fn encode_lz4_block(src: &[u8]) -> Vec<u8> {
    const HASH_LOG: usize = 14;
    const HASH_SIZE: usize = 1 << HASH_LOG;
    let mut out = Vec::with_capacity(src.len() / 2 + 64);
    let n = src.len();
    if n < 13 {
        let token = ((n.min(15)) as u8) << 4;
        out.push(token);
        if n >= 15 {
            push_lz4_len(&mut out, n - 15);
        }
        out.extend_from_slice(src);
        return out;
    }
    let mut table = vec![u32::MAX; HASH_SIZE];
    let limit = n - 5;
    let mut anchor = 0usize;
    let mut ip = 0usize;
    while ip + 4 <= limit {
        let v = u32::from_le_bytes(src[ip..ip + 4].try_into().unwrap());
        let h = (v.wrapping_mul(2654435761u32) >> (32 - HASH_LOG)) as usize;
        let cand = table[h];
        table[h] = ip as u32;
        if cand != u32::MAX {
            let ref_idx = cand as usize;
            let dist = ip - ref_idx;
            if dist > 0 && dist <= 65535 && src[ref_idx..ref_idx + 4] == src[ip..ip + 4] {
                let mut mlen = 4usize;
                let max_match = limit - ip;
                while mlen < max_match && src[ref_idx + mlen] == src[ip + mlen] {
                    mlen += 1;
                }
                let lit_len = ip - anchor;
                let mut token = ((lit_len.min(15)) as u8) << 4;
                let ml_code = mlen - 4;
                token |= (ml_code.min(15)) as u8;
                out.push(token);
                if lit_len >= 15 {
                    push_lz4_len(&mut out, lit_len - 15);
                }
                out.extend_from_slice(&src[anchor..ip]);
                out.extend_from_slice(&(dist as u16).to_le_bytes());
                if ml_code >= 15 {
                    push_lz4_len(&mut out, ml_code - 15);
                }
                ip += mlen;
                anchor = ip;
                continue;
            }
        }
        ip += 1;
    }
    let last_lit = n - anchor;
    let token = ((last_lit.min(15)) as u8) << 4;
    out.push(token);
    if last_lit >= 15 {
        push_lz4_len(&mut out, last_lit - 15);
    }
    out.extend_from_slice(&src[anchor..n]);
    out
}

/// Encode bytes into a canonical `LZ4_LEGACY` stream with 8 MiB chunks.
pub fn lz4_legacy_encode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() / 2 + 1024);
    out.extend_from_slice(&LZ4_LEGACY_MAGIC);
    for chunk in input.chunks(LZ4_LEGACY_CHUNK) {
        let block = encode_lz4_block(chunk);
        out.extend_from_slice(&(block.len() as u32).to_le_bytes());
        out.extend_from_slice(&block);
    }
    out
}

fn parse_hex_u32(slice: &[u8]) -> Result<u32> {
    let s = std::str::from_utf8(slice).map_err(|_| "Invalid CPIO hex field")?;
    u32::from_str_radix(s, 16).map_err(|e| format!("Invalid CPIO hex value {s}: {e}"))
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

/// Replace matching regular-file entries inside an uncompressed `newc` CPIO archive.
pub fn patch_cpio_newc(
    cpio: &[u8],
    updates: &BTreeMap<String, Vec<u8>>,
) -> Result<(Vec<u8>, usize)> {
    let mut pos = 0usize;
    let mut out = Vec::with_capacity(cpio.len());
    let mut replaced = 0usize;

    while pos + 110 <= cpio.len() {
        let magic = &cpio[pos..pos + 6];
        if magic != b"070701" && magic != b"070702" {
            return Err("Initrd không chứa cấu trúc CPIO newc hợp lệ".into());
        }
        let mode = parse_hex_u32(&cpio[pos + 14..pos + 22])?;
        let filesize = parse_hex_u32(&cpio[pos + 54..pos + 62])? as usize;
        let namesize = parse_hex_u32(&cpio[pos + 94..pos + 102])? as usize;
        if namesize == 0 || pos + 110 + namesize > cpio.len() {
            return Err("Mục CPIO trong initrd bị cắt cụt".into());
        }
        let name_bytes = &cpio[pos + 110..pos + 110 + namesize - 1];
        let name = std::str::from_utf8(name_bytes)
            .map_err(|_| "Tên tệp không hợp lệ trong initrd CPIO")?
            .trim_start_matches("./")
            .trim_start_matches('/');
        let data_start = align4(pos + 110 + namesize);
        let next = align4(
            data_start
                .checked_add(filesize)
                .ok_or("Kích thước tệp CPIO tràn số")?,
        );
        if next > cpio.len() {
            return Err("Dữ liệu tệp CPIO trong initrd bị cắt cụt".into());
        }
        if name == "TRAILER!!!" {
            out.extend_from_slice(&cpio[pos..]);
            break;
        }
        let is_regular = (mode & 0o170000) == 0o100000;
        if is_regular {
            if let Some(new_data) = updates.get(name) {
                let mut hdr = cpio[pos..pos + 110].to_vec();
                let size_hex = format!("{:08X}", new_data.len());
                hdr[54..62].copy_from_slice(size_hex.as_bytes());
                out.extend_from_slice(&hdr);
                out.extend_from_slice(&cpio[pos + 110..pos + 110 + namesize]);
                while out.len() % 4 != 0 {
                    out.push(0);
                }
                out.extend_from_slice(new_data);
                while out.len() % 4 != 0 {
                    out.push(0);
                }
                replaced += 1;
                pos = next;
                continue;
            }
        }
        out.extend_from_slice(&cpio[pos..next]);
        pos = next;
    }

    Ok((out, replaced))
}

/// Patch matching files inside an `LZ4_LEGACY` + `newc` CPIO `initrd.img`.
pub fn patch_initrd_lz4_cpio(
    initrd_bytes: &[u8],
    updates: &BTreeMap<String, Vec<u8>>,
) -> Result<Option<Vec<u8>>> {
    if updates.is_empty() {
        return Ok(None);
    }
    let normalized: BTreeMap<String, Vec<u8>> = updates
        .iter()
        .map(|(k, v)| (k.trim_start_matches('/').to_owned(), v.clone()))
        .collect();
    let cpio = lz4_legacy_decode(initrd_bytes)?;
    let (patched_cpio, count) = patch_cpio_newc(&cpio, &normalized)?;
    if count == 0 {
        return Ok(None);
    }
    let encoded = lz4_legacy_encode(&patched_cpio);
    if lz4_legacy_decode(&encoded)? != patched_cpio {
        return Err("Lỗi kiểm tra chéo sau khi nén lại initrd.img (LZ4)".into());
    }
    Ok(Some(encoded))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cpio_entry(name: &str, mode: u32, data: &[u8]) -> Vec<u8> {
        let name_nul = name.len() + 1;
        let hdr = format!(
            "070701{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}{:08X}",
            1, mode, 0, 0, 1, 0, data.len(), 0, 0, 0, 0, name_nul, 0
        );
        let mut out = hdr.into_bytes();
        out.extend_from_slice(name.as_bytes());
        out.push(0);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        out.extend_from_slice(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
        out
    }

    #[test]
    fn lz4_legacy_and_cpio_roundtrip() {
        let mut cpio = Vec::new();
        cpio.extend_from_slice(&make_cpio_entry(
            "usr/share/plymouth/themes/nfslogo/nfslogo/nfslogo_1.png",
            0o100644,
            b"OLD_PNG_DATA_AAAABBBBCCCC",
        ));
        cpio.extend_from_slice(&make_cpio_entry("TRAILER!!!", 0, &[]));
        while cpio.len() % 512 != 0 {
            cpio.push(0);
        }

        let initrd = lz4_legacy_encode(&cpio);
        assert_eq!(lz4_legacy_decode(&initrd).unwrap(), cpio);

        let mut updates = BTreeMap::new();
        updates.insert(
            "usr/share/plymouth/themes/nfslogo/nfslogo/nfslogo_1.png".to_string(),
            b"NEW_ENGLISH_PNG_CONTENT_1234567890_REPEATED_REPEATED".to_vec(),
        );
        let updated = patch_initrd_lz4_cpio(&initrd, &updates)
            .unwrap()
            .expect("should patch initrd");
        let decoded = lz4_legacy_decode(&updated).unwrap();
        assert!(decoded.windows(b"NEW_ENGLISH_PNG_CONTENT".len()).any(|w| w == b"NEW_ENGLISH_PNG_CONTENT"));
        assert!(!decoded.windows(b"OLD_PNG_DATA".len()).any(|w| w == b"OLD_PNG_DATA"));
    }
}
