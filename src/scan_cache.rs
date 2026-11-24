// UI inspection cache only. Builds always re-read and validate the original ISO.
use crate::types::{Image, Package};
use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};
fn location(path: &Path) -> Option<PathBuf> {
    let canonical = path.canonicalize().ok()?;
    let meta = fs::metadata(path).ok()?;
    let time = meta
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let mut f = File::open(path).ok()?;
    let mut sample = vec![0; meta.len().min(65536) as usize];
    f.read_exact(&mut sample).ok()?;
    if meta.len() > 65536 {
        f.seek(SeekFrom::End(-65536)).ok()?;
        let n = sample.len();
        sample.resize(n + 65536, 0);
        f.read_exact(&mut sample[n..]).ok()?;
    }
    let key = format!(
        "5|{}|{}|{}|{}",
        canonical.display(),
        meta.len(),
        time,
        crate::xz::crc32(&sample)
    );
    let mut hash = 0xcbf29ce484222325u64;
    for b in key.bytes() {
        hash = (hash ^ b as u64).wrapping_mul(0x100000001b3);
    }
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("ISOLite/scan-v5");
    Some(root.join(format!("{hash:016x}.bin")))
}
fn string(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}
fn encode(image: &Image) -> Vec<u8> {
    let mut out = Vec::new();
    string(&mut out, &image.release);
    string(&mut out, &image.distro_name);
    string(&mut out, &image.squash_iso_path);
    out.extend_from_slice(&image.squash_offset.to_le_bytes());
    out.extend_from_slice(&image.flatpak_bytes.to_le_bytes());
    out.extend_from_slice(&(image.supported_locales.len() as u32).to_le_bytes());
    for s in &image.supported_locales {
        string(&mut out, s);
    }
    out.extend_from_slice(&(image.packages.len() as u32).to_le_bytes());
    for p in &image.packages {
        string(&mut out, &p.name);
        string(&mut out, &p.version);
        string(&mut out, &p.group);
        out.push(u8::from(p.protected_package));
        out.extend_from_slice(&p.installed_kib.unwrap_or(u64::MAX).to_le_bytes());
    }
    let mut all = b"ISC5".to_vec();
    all.extend_from_slice(&crate::xz::crc32(&out).to_le_bytes());
    all.extend(out);
    all
}
struct Decoder<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Decoder<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let b = self.bytes.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(b)
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn string(&mut self) -> Option<String> {
        let n = self.u32()? as usize;
        if n > 32768 {
            return None;
        }
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
}
fn decode(path: &Path, bytes: &[u8]) -> Option<Image> {
    if bytes.len() < 8
        || &bytes[..4] != b"ISC5"
        || crate::xz::crc32(&bytes[8..]) != u32::from_le_bytes(bytes[4..8].try_into().ok()?)
    {
        return None;
    }
    let mut d = Decoder {
        bytes: &bytes[8..],
        pos: 0,
    };
    let release = d.string()?;
    let distro_name = d.string()?;
    let squash_iso_path = d.string()?;
    let squash_offset = d.u64()?;
    let flatpak_bytes = d.u64()?;
    let n = d.u32()?;
    if n > 10000 {
        return None;
    }
    let mut supported_locales = Vec::new();
    for _ in 0..n {
        supported_locales.push(d.string()?);
    }
    let n = d.u32()?;
    if n > 100000 {
        return None;
    }
    let mut packages = Vec::new();
    for _ in 0..n {
        let name = d.string()?;
        let version = d.string()?;
        let group = d.string()?;
        let protected_package = match d.take(1)?[0] {
            0 => false,
            1 => true,
            _ => return None,
        };
        let size = d.u64()?;
        packages.push(Package {
            name,
            version,
            group,
            protected_package,
            installed_kib: if size == u64::MAX { None } else { Some(size) },
        });
    }
    if d.pos != d.bytes.len() {
        return None;
    }
    Some(Image {
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
pub fn load(path: &Path) -> Option<Image> {
    let file = location(path)?;
    if fs::metadata(&file).ok()?.len() > 32 * 1024 * 1024 {
        return None;
    }
    decode(path, &fs::read(file).ok()?)
}
pub fn save(image: &Image) {
    if let Some(file) = location(&image.path) {
        if let Some(parent) = file.parent() {
            if fs::create_dir_all(parent).is_ok() {
                let tmp = file.with_extension(format!("{}.tmp", std::process::id()));
                if fs::write(&tmp, encode(image)).is_ok() {
                    let _ = fs::rename(tmp, file);
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn corrupt_cache_is_never_used() {
        let image = Image {
            path: "sample.iso".into(),
            release: "Sample".into(),
            distro_name: "test".into(),
            supported_locales: vec!["vi_VN.UTF-8".into()],
            packages: Vec::new(),
            has_flatpak: false,
            flatpak_bytes: 0,
            squash_offset: 123,
            squash_iso_path: "/live/root.sfs".into(),
        };
        let mut b = encode(&image);
        assert_eq!(
            decode(Path::new("sample.iso"), &b).unwrap().release,
            "Sample"
        );
        b[10] ^= 1;
        assert!(decode(Path::new("sample.iso"), &b).is_none());
    }
}
