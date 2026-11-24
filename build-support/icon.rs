// Native AMD64 COFF resource object, authored with Rust std only.
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
pub fn write(path: &Path, ico: &[u8]) -> std::io::Result<()> {
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
