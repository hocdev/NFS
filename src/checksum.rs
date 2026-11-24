use std::fs::File;
use std::io::Read;
use std::path::Path;
pub fn md5_file(path: &Path) -> Result<String, String> {
    let mut input = File::open(path).map_err(|e| e.to_string())?;
    let mut state = [0x67452301u32, 0xefcdab89, 0x98badcfe, 0x10325476];
    let mut length = 0u64;
    let mut pending = Vec::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        length += n as u64;
        pending.extend_from_slice(&buffer[..n]);
        let whole = pending.len() / 64 * 64;
        for block in pending[..whole].chunks_exact(64) {
            transform(&mut state, block);
        }
        pending.drain(..whole);
    }
    pending.push(0x80);
    while pending.len() % 64 != 56 {
        pending.push(0);
    }
    pending.extend_from_slice(&(length.wrapping_mul(8)).to_le_bytes());
    for block in pending.chunks_exact(64) {
        transform(&mut state, block);
    }
    Ok(state
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .map(|b| format!("{b:02x}"))
        .collect())
}
fn transform(state: &mut [u32; 4], block: &[u8]) {
    let shifts = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];
    static CONSTANTS: std::sync::OnceLock<[u32; 64]> = std::sync::OnceLock::new();
    let constants = CONSTANTS.get_or_init(|| {
        std::array::from_fn(|i| (((i + 1) as f64).sin().abs() * 4294967296.0) as u32)
    });
    let [mut a, mut b, mut c, mut d] = *state;
    for i in 0..64 {
        let (f, g) = match i {
            0..16 => ((b & c) | (!b & d), i),
            16..32 => ((d & b) | (!d & c), (5 * i + 1) % 16),
            32..48 => (b ^ c ^ d, (3 * i + 5) % 16),
            _ => (c ^ (b | !d), (7 * i) % 16),
        };
        let k = constants[i];
        let word = u32::from_le_bytes(block[g * 4..g * 4 + 4].try_into().unwrap());
        let next = b.wrapping_add(
            a.wrapping_add(f)
                .wrapping_add(k)
                .wrapping_add(word)
                .rotate_left(shifts[(i / 16) * 4 + i % 4]),
        );
        a = d;
        d = c;
        c = b;
        b = next;
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d]) {
        *s = s.wrapping_add(v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn standard_vectors() {
        let file = std::env::temp_dir().join(format!("isolite-md5-{}.tmp", std::process::id()));
        for (input, expected) in [
            ("", "d41d8cd98f00b204e9800998ecf8427e"),
            ("abc", "900150983cd24fb0d6963f7d28e17f72"),
            (
                "The quick brown fox jumps over the lazy dog",
                "9e107d9d372bb6826bd81d3542a419d6",
            ),
        ] {
            std::fs::write(&file, input).unwrap();
            assert_eq!(md5_file(&file).unwrap(), expected);
        }
        std::fs::remove_file(file).unwrap();
    }
}
