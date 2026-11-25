// Pure-Rust RSA PKCS#1 v1.5 SHA-256 signing, verification, and PEM/DER handling.

type Result<T> = std::result::Result<T, String>;

const SHA256_DIGEST_INFO: [u8; 19] = [
    0x30, 0x31, 0x30, 0x0d, 0x06, 0x09, 0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, 0x05,
    0x00, 0x04, 0x20,
];

const RSA_OID: [u8; 9] = [0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];

pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = Vec::with_capacity(data.len() + 72);
    msg.extend_from_slice(data);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (hv, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *hv = hv.wrapping_add(v);
        }
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BigUint {
    limbs: Vec<u32>, // little-endian base 2^32
}

impl BigUint {
    fn zero() -> Self {
        Self { limbs: Vec::new() }
    }

    fn one() -> Self {
        Self { limbs: vec![1] }
    }

    fn trim(&mut self) {
        while self.limbs.last() == Some(&0) {
            self.limbs.pop();
        }
    }

    fn is_zero(&self) -> bool {
        self.limbs.is_empty()
    }

    fn is_odd(&self) -> bool {
        self.limbs.first().is_some_and(|w| (w & 1) == 1)
    }

    fn from_be_bytes(bytes: &[u8]) -> Self {
        let trimmed = match bytes.iter().position(|&b| b != 0) {
            Some(i) => &bytes[i..],
            None => return Self::zero(),
        };
        let mut limbs = Vec::with_capacity(trimmed.len().div_ceil(4));
        let mut i = trimmed.len();
        while i >= 4 {
            limbs.push(u32::from_be_bytes(trimmed[i - 4..i].try_into().unwrap()));
            i -= 4;
        }
        if i > 0 {
            let mut buf = [0u8; 4];
            buf[4 - i..].copy_from_slice(&trimmed[..i]);
            limbs.push(u32::from_be_bytes(buf));
        }
        let mut res = Self { limbs };
        res.trim();
        res
    }

    fn to_be_bytes_padded(&self, len: usize) -> Result<Vec<u8>> {
        let mut out = vec![0u8; len];
        for (idx, &limb) in self.limbs.iter().enumerate() {
            let be = limb.to_be_bytes();
            for b_i in 0..4 {
                let byte = be[3 - b_i];
                let pos = idx * 4 + b_i;
                if pos >= len {
                    if byte != 0 {
                        return Err("RSA integer overflows target byte length".into());
                    }
                } else {
                    out[len - 1 - pos] = byte;
                }
            }
        }
        Ok(out)
    }

    fn to_der_uint_bytes(&self) -> Vec<u8> {
        if self.is_zero() {
            return vec![0];
        }
        let n = self.limbs.len() * 4;
        let raw = self.to_be_bytes_padded(n).unwrap();
        let first = raw.iter().position(|&b| b != 0).unwrap_or(raw.len() - 1);
        let slice = &raw[first..];
        if (slice[0] & 0x80) != 0 {
            let mut out = Vec::with_capacity(slice.len() + 1);
            out.push(0);
            out.extend_from_slice(slice);
            out
        } else {
            slice.to_vec()
        }
    }

    fn cmp_val(&self, other: &Self) -> std::cmp::Ordering {
        self.limbs.len().cmp(&other.limbs.len()).then_with(|| {
            for (a, b) in self.limbs.iter().rev().zip(other.limbs.iter().rev()) {
                match a.cmp(b) {
                    std::cmp::Ordering::Equal => {}
                    ord => return ord,
                }
            }
            std::cmp::Ordering::Equal
        })
    }

    fn sub_assign(&mut self, other: &Self) {
        let mut borrow: u64 = 0;
        for i in 0..self.limbs.len() {
            let a = self.limbs[i] as u64;
            let b = *other.limbs.get(i).unwrap_or(&0) as u64 + borrow;
            if a >= b {
                self.limbs[i] = (a - b) as u32;
                borrow = 0;
            } else {
                self.limbs[i] = ((1u64 << 32) + a - b) as u32;
                borrow = 1;
            }
        }
        self.trim();
    }

    fn bit_len(&self) -> usize {
        match self.limbs.last() {
            Some(&top) => (self.limbs.len() - 1) * 32 + (32 - top.leading_zeros() as usize),
            None => 0,
        }
    }

    fn bit(&self, idx: usize) -> bool {
        let w = idx / 32;
        let b = idx % 32;
        self.limbs.get(w).is_some_and(|&limb| ((limb >> b) & 1) != 0)
    }

    fn shl1_mod(&mut self, m: &Self) {
        let mut carry = 0u32;
        for limb in &mut self.limbs {
            let next = *limb >> 31;
            *limb = (*limb << 1) | carry;
            carry = next;
        }
        if carry != 0 {
            self.limbs.push(carry);
        }
        self.trim();
        if self.cmp_val(m) != std::cmp::Ordering::Less {
            self.sub_assign(m);
        }
    }

    fn mod_reduce(&self, m: &Self) -> Self {
        let bits = self.bit_len();
        let mut rem = Self::zero();
        for i in (0..bits).rev() {
            let mut carry = u32::from(self.bit(i));
            for limb in &mut rem.limbs {
                let next = *limb >> 31;
                *limb = (*limb << 1) | carry;
                carry = next;
            }
            if carry != 0 {
                rem.limbs.push(carry);
            }
            rem.trim();
            if rem.cmp_val(m) != std::cmp::Ordering::Less {
                rem.sub_assign(m);
            }
        }
        rem
    }

    fn mont_n0_inv(n0: u32) -> u32 {
        let mut inv = 1u32;
        for _ in 0..5 {
            inv = inv.wrapping_mul(2u32.wrapping_sub(n0.wrapping_mul(inv)));
        }
        inv.wrapping_neg()
    }

    fn mont_mul(a: &[u32], b: &[u32], n: &[u32], n0_inv: u32) -> Vec<u32> {
        let k = n.len();
        let mut t = vec![0u32; k + 2];
        for i in 0..k {
            let ai = *a.get(i).unwrap_or(&0) as u64;
            let mut carry = 0u64;
            for j in 0..k {
                let bj = *b.get(j).unwrap_or(&0) as u64;
                let prod = t[j] as u64 + ai * bj + carry;
                t[j] = prod as u32;
                carry = prod >> 32;
            }
            let sum = t[k] as u64 + carry;
            t[k] = sum as u32;
            t[k + 1] = (sum >> 32) as u32;

            let m = (t[0].wrapping_mul(n0_inv)) as u64;
            let mut carry2 = (t[0] as u64 + m * n[0] as u64) >> 32;
            for j in 1..k {
                let prod = t[j] as u64 + m * n[j] as u64 + carry2;
                t[j - 1] = prod as u32;
                carry2 = prod >> 32;
            }
            let sum2 = t[k] as u64 + carry2;
            t[k - 1] = sum2 as u32;
            t[k] = t[k + 1].wrapping_add((sum2 >> 32) as u32);
            t[k + 1] = 0;
        }
        let mut out = t[..k].to_vec();
        let ge_n = t[k] != 0 || {
            let mut ord = std::cmp::Ordering::Equal;
            for idx in (0..k).rev() {
                if out[idx] != n[idx] {
                    ord = out[idx].cmp(&n[idx]);
                    break;
                }
            }
            ord != std::cmp::Ordering::Less
        };
        if ge_n {
            let mut borrow = 0u64;
            for idx in 0..k {
                let av = out[idx] as u64;
                let bv = n[idx] as u64 + borrow;
                if av >= bv {
                    out[idx] = (av - bv) as u32;
                    borrow = 0;
                } else {
                    out[idx] = ((1u64 << 32) + av - bv) as u32;
                    borrow = 1;
                }
            }
        }
        out
    }

    fn mod_pow(&self, exp: &Self, modulus: &Self) -> Result<Self> {
        if modulus.is_zero() || !modulus.is_odd() {
            return Err("RSA modulus must be a positive odd integer".into());
        }
        if modulus.limbs == [1] {
            return Ok(Self::zero());
        }
        let k = modulus.limbs.len();
        let n = &modulus.limbs;
        let n0_inv = Self::mont_n0_inv(n[0]);

        let mut r2 = Self::one();
        for _ in 0..(2 * k * 32) {
            r2.shl1_mod(modulus);
        }
        let mut r2_limbs = r2.limbs;
        r2_limbs.resize(k, 0);

        let base_red = self.mod_reduce(modulus);
        let mut base_limbs = base_red.limbs;
        base_limbs.resize(k, 0);

        let mut one_limbs = vec![0u32; k];
        one_limbs[0] = 1;

        let mut base_m = Self::mont_mul(&base_limbs, &r2_limbs, n, n0_inv);
        let mut acc_m = Self::mont_mul(&one_limbs, &r2_limbs, n, n0_inv);

        let bits = exp.bit_len();
        for i in 0..bits {
            if exp.bit(i) {
                acc_m = Self::mont_mul(&acc_m, &base_m, n, n0_inv);
            }
            if i + 1 < bits {
                base_m = Self::mont_mul(&base_m, &base_m, n, n0_inv);
            }
        }

        let out = Self::mont_mul(&acc_m, &one_limbs, n, n0_inv);
        let mut res = Self { limbs: out };
        res.trim();
        Ok(res)
    }
}

fn base64_decode(input: &str) -> Result<Vec<u8>> {
    fn val(b: u8) -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let clean: Vec<u8> = input
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .collect();
    if clean.len() % 4 != 0 {
        return Err("Invalid base64 length in PEM".into());
    }
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks_exact(4) {
        let a = val(chunk[0]).ok_or("Invalid base64 character")?;
        let b = val(chunk[1]).ok_or("Invalid base64 character")?;
        let c = if chunk[2] == b'=' {
            0
        } else {
            val(chunk[2]).ok_or("Invalid base64 character")?
        };
        let d = if chunk[3] == b'=' {
            0
        } else {
            val(chunk[3]).ok_or("Invalid base64 character")?
        };
        let triple = ((a as u32) << 18) | ((b as u32) << 12) | ((c as u32) << 6) | (d as u32);
        out.push((triple >> 16) as u8);
        if chunk[2] != b'=' {
            out.push((triple >> 8) as u8);
        }
        if chunk[3] != b'=' {
            out.push(triple as u8);
        }
    }
    Ok(out)
}

fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let v = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((v >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((v >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((v >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(v & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn parse_pem(pem: &str, labels: &[&str]) -> Result<(String, Vec<u8>)> {
    for &label in labels {
        let begin = format!("-----BEGIN {label}-----");
        let end = format!("-----END {label}-----");
        if let Some(s) = pem.find(&begin) {
            let after = &pem[s + begin.len()..];
            if let Some(e) = after.find(&end) {
                let der = base64_decode(&after[..e])?;
                return Ok((label.to_string(), der));
            }
        }
    }
    Err(format!(
        "Không tìm thấy khối PEM hợp lệ ({})",
        labels.join(" / ")
    ))
}

fn encode_pem(label: &str, der: &[u8]) -> String {
    let b64 = base64_encode(der);
    let mut out = format!("-----BEGIN {label}-----\n");
    for line in b64.as_bytes().chunks(64) {
        out.push_str(std::str::from_utf8(line).unwrap());
        out.push('\n');
    }
    out.push_str(&format!("-----END {label}-----\n"));
    out
}

fn read_tlv(input: &[u8], expected_tag: u8) -> Result<(&[u8], &[u8])> {
    let &tag = input.first().ok_or("Unexpected end of ASN.1 DER")?;
    if tag != expected_tag {
        return Err(format!(
            "ASN.1 tag mismatch: expected {expected_tag:#x}, got {tag:#x}"
        ));
    }
    let &len_byte = input.get(1).ok_or("Truncated ASN.1 length")?;
    let (len, hdr) = if (len_byte & 0x80) == 0 {
        (len_byte as usize, 2usize)
    } else {
        let octets = (len_byte & 0x7f) as usize;
        if octets == 0 || octets > 4 || input.len() < 2 + octets {
            return Err("Invalid ASN.1 length encoding".into());
        }
        let mut len = 0usize;
        for &b in &input[2..2 + octets] {
            len = (len << 8) | b as usize;
        }
        (len, 2 + octets)
    };
    let end = hdr.checked_add(len).ok_or("ASN.1 length overflow")?;
    if end > input.len() {
        return Err("Truncated ASN.1 value".into());
    }
    Ok((&input[hdr..end], &input[end..]))
}

fn read_uint(input: &[u8]) -> Result<(BigUint, &[u8])> {
    let (val, rest) = read_tlv(input, 0x02)?;
    if val.is_empty() || (val[0] & 0x80) != 0 {
        return Err("Invalid unsigned ASN.1 INTEGER".into());
    }
    Ok((BigUint::from_be_bytes(val), rest))
}

fn der_tlv(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 6);
    out.push(tag);
    let len = body.len();
    if len < 0x80 {
        out.push(len as u8);
    } else if len <= 0xff {
        out.push(0x81);
        out.push(len as u8);
    } else if len <= 0xffff {
        out.push(0x82);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(0x83);
        out.push((len >> 16) as u8);
        out.push((len >> 8) as u8);
        out.push(len as u8);
    }
    out.extend_from_slice(body);
    out
}

#[derive(Clone, Debug)]
pub struct RsaPrivateKey {
    n: BigUint,
    e: BigUint,
    d: BigUint,
    p: BigUint,
    q: BigUint,
    dp: BigUint,
    dq: BigUint,
    qinv: BigUint,
}

#[derive(Clone, Debug)]
pub struct RsaPublicKey {
    n: BigUint,
    e: BigUint,
}

impl RsaPrivateKey {
    pub fn from_pem(pem: &str) -> Result<Self> {
        let (label, der) = parse_pem(pem, &["RSA PRIVATE KEY", "PRIVATE KEY"])?;
        let pkcs1 = if label == "RSA PRIVATE KEY" {
            der
        } else {
            let (seq, rest) = read_tlv(&der, 0x30)?;
            if !rest.is_empty() {
                return Err("Trailing data in PKCS#8 key".into());
            }
            let (_ver, rest) = read_uint(seq)?;
            let (alg, rest) = read_tlv(rest, 0x30)?;
            let (oid, _params) = read_tlv(alg, 0x06)?;
            if oid != RSA_OID {
                return Err("Khóa PKCS#8 không phải thuật toán RSA".into());
            }
            let (octets, rest) = read_tlv(rest, 0x04)?;
            if !rest.is_empty() {
                return Err("Trailing data after PKCS#8 privateKey".into());
            }
            octets.to_vec()
        };

        let (seq, rest) = read_tlv(&pkcs1, 0x30)?;
        if !rest.is_empty() {
            return Err("Trailing data in RSA private key".into());
        }
        let (_ver, rest) = read_uint(seq)?;
        let (n, rest) = read_uint(rest)?;
        let (e, rest) = read_uint(rest)?;
        let (d, rest) = read_uint(rest)?;
        let (p, rest) = read_uint(rest)?;
        let (q, rest) = read_uint(rest)?;
        let (dp, rest) = read_uint(rest)?;
        let (dq, rest) = read_uint(rest)?;
        let (qinv, _) = read_uint(rest)?;

        if n.bit_len() < 1024 || !n.is_odd() || e.is_zero() || d.is_zero() {
            return Err("Khóa RSA không hợp lệ (cần tối thiểu 1024-bit)".into());
        }
        Ok(Self {
            n,
            e,
            d,
            p,
            q,
            dp,
            dq,
            qinv,
        })
    }

    pub fn to_pkcs1_pem(&self) -> String {
        let mut body = Vec::new();
        body.extend_from_slice(&der_tlv(0x02, &[0]));
        for num in [
            &self.n, &self.e, &self.d, &self.p, &self.q, &self.dp, &self.dq, &self.qinv,
        ] {
            body.extend_from_slice(&der_tlv(0x02, &num.to_der_uint_bytes()));
        }
        encode_pem("RSA PRIVATE KEY", &der_tlv(0x30, &body))
    }

    pub fn public_key(&self) -> RsaPublicKey {
        RsaPublicKey {
            n: self.n.clone(),
            e: self.e.clone(),
        }
    }

    pub fn sign_pkcs1_sha256(&self, data: &[u8]) -> Result<Vec<u8>> {
        let k = self.n.bit_len().div_ceil(8);
        let em = pkcs1_v15_sha256_encode(data, k)?;
        let m = BigUint::from_be_bytes(&em);
        let s = if !self.p.is_zero()
            && !self.q.is_zero()
            && !self.dp.is_zero()
            && !self.dq.is_zero()
            && !self.qinv.is_zero()
            && self.p.is_odd()
            && self.q.is_odd()
        {
            // CRT optimization: s1 = m^dP mod p, s2 = m^dQ mod q
            let s1 = m.mod_pow(&self.dp, &self.p)?;
            let s2 = m.mod_pow(&self.dq, &self.q)?;
            let s2_mod_p = s2.mod_reduce(&self.p);
            let diff = if s1.cmp_val(&s2_mod_p) != std::cmp::Ordering::Less {
                let mut d = s1;
                d.sub_assign(&s2_mod_p);
                d
            } else {
                let mut d = self.p.clone();
                let mut neg = s2_mod_p;
                neg.sub_assign(&s1);
                d.sub_assign(&neg);
                d
            };
            let h = mul_mod(&diff, &self.qinv, &self.p)?;
            let hq = mul_raw(&h, &self.q);
            add_raw(&s2, &hq).mod_reduce(&self.n)
        } else {
            m.mod_pow(&self.d, &self.n)?
        };
        let sig = s.to_be_bytes_padded(k)?;
        self.public_key().verify_pkcs1_sha256(data, &sig)?;
        Ok(sig)
    }
}

impl RsaPublicKey {
    pub fn from_pem(pem: &str) -> Result<Self> {
        let (label, der) = parse_pem(pem, &["PUBLIC KEY", "RSA PUBLIC KEY"])?;
        let pkcs1 = if label == "RSA PUBLIC KEY" {
            der
        } else {
            let (seq, rest) = read_tlv(&der, 0x30)?;
            if !rest.is_empty() {
                return Err("Trailing data in SPKI public key".into());
            }
            let (alg, rest) = read_tlv(seq, 0x30)?;
            let (oid, _) = read_tlv(alg, 0x06)?;
            if oid != RSA_OID {
                return Err("Public key is not RSA".into());
            }
            let (bits, rest) = read_tlv(rest, 0x03)?;
            if !rest.is_empty() || bits.is_empty() || bits[0] != 0 {
                return Err("Invalid BIT STRING in SPKI public key".into());
            }
            bits[1..].to_vec()
        };
        let (seq, rest) = read_tlv(&pkcs1, 0x30)?;
        if !rest.is_empty() {
            return Err("Trailing data in RSA public key".into());
        }
        let (n, rest) = read_uint(seq)?;
        let (e, rest) = read_uint(rest)?;
        if !rest.is_empty() || n.bit_len() < 1024 || !n.is_odd() || e.is_zero() {
            return Err("Invalid RSA public key parameters".into());
        }
        Ok(Self { n, e })
    }

    pub fn to_spki_pem(&self) -> String {
        let mut rsa_pub = Vec::new();
        rsa_pub.extend_from_slice(&der_tlv(0x02, &self.n.to_der_uint_bytes()));
        rsa_pub.extend_from_slice(&der_tlv(0x02, &self.e.to_der_uint_bytes()));
        let rsa_pub_seq = der_tlv(0x30, &rsa_pub);

        let mut alg = Vec::new();
        alg.extend_from_slice(&der_tlv(0x06, &RSA_OID));
        alg.extend_from_slice(&der_tlv(0x05, &[]));
        let alg_seq = der_tlv(0x30, &alg);

        let mut bit_string = Vec::with_capacity(rsa_pub_seq.len() + 1);
        bit_string.push(0);
        bit_string.extend_from_slice(&rsa_pub_seq);

        let mut spki = Vec::new();
        spki.extend_from_slice(&alg_seq);
        spki.extend_from_slice(&der_tlv(0x03, &bit_string));
        encode_pem("PUBLIC KEY", &der_tlv(0x30, &spki))
    }

    pub fn verify_pkcs1_sha256(&self, data: &[u8], sig: &[u8]) -> Result<()> {
        let k = self.n.bit_len().div_ceil(8);
        if sig.len() != k {
            return Err(format!(
                "Kích thước chữ ký RSA không khớp ({} != {k})",
                sig.len()
            ));
        }
        let s = BigUint::from_be_bytes(sig);
        if s.cmp_val(&self.n) != std::cmp::Ordering::Less {
            return Err("Chữ ký RSA vượt quá modulus".into());
        }
        let m = s.mod_pow(&self.e, &self.n)?;
        let recovered = m.to_be_bytes_padded(k)?;
        let expected = pkcs1_v15_sha256_encode(data, k)?;
        if recovered != expected {
            return Err("Chữ ký RSA không khớp với nội dung /md5sum.txt".into());
        }
        Ok(())
    }
}

fn mul_raw(a: &BigUint, b: &BigUint) -> BigUint {
    if a.is_zero() || b.is_zero() {
        return BigUint::zero();
    }
    let mut out = vec![0u32; a.limbs.len() + b.limbs.len()];
    for i in 0..a.limbs.len() {
        let mut carry = 0u64;
        for j in 0..b.limbs.len() {
            let v = out[i + j] as u64 + (a.limbs[i] as u64) * (b.limbs[j] as u64) + carry;
            out[i + j] = v as u32;
            carry = v >> 32;
        }
        out[i + b.limbs.len()] = carry as u32;
    }
    let mut res = BigUint { limbs: out };
    res.trim();
    res
}

fn add_raw(a: &BigUint, b: &BigUint) -> BigUint {
    let n = a.limbs.len().max(b.limbs.len());
    let mut out = Vec::with_capacity(n + 1);
    let mut carry = 0u64;
    for i in 0..n {
        let sum = *a.limbs.get(i).unwrap_or(&0) as u64
            + *b.limbs.get(i).unwrap_or(&0) as u64
            + carry;
        out.push(sum as u32);
        carry = sum >> 32;
    }
    if carry != 0 {
        out.push(carry as u32);
    }
    BigUint { limbs: out }
}

fn mul_mod(a: &BigUint, b: &BigUint, m: &BigUint) -> Result<BigUint> {
    if m.is_zero() {
        return Err("Zero modulus".into());
    }
    Ok(mul_raw(a, b).mod_reduce(m))
}

fn pkcs1_v15_sha256_encode(data: &[u8], k: usize) -> Result<Vec<u8>> {
    let digest = sha256(data);
    let t_len = SHA256_DIGEST_INFO.len() + digest.len();
    if k < t_len + 11 {
        return Err("RSA key size too small for SHA-256 PKCS#1 v1.5".into());
    }
    let ps_len = k - t_len - 3;
    let mut em = Vec::with_capacity(k);
    em.push(0x00);
    em.push(0x01);
    em.resize(2 + ps_len, 0xff);
    em.push(0x00);
    em.extend_from_slice(&SHA256_DIGEST_INFO);
    em.extend_from_slice(&digest);
    Ok(em)
}

pub fn sign_with_pem_file(key_path: &std::path::Path, data: &[u8]) -> Result<(Vec<u8>, String)> {
    let pem = std::fs::read_to_string(key_path)
        .map_err(|e| format!("Không đọc được tệp khóa RSA {}: {e}", key_path.display()))?;
    let key = RsaPrivateKey::from_pem(&pem)?;
    let sig = key.sign_pkcs1_sha256(data)?;
    let pub_pem = key.public_key().to_spki_pem();
    Ok((sig, pub_pem))
}

pub fn verify_with_pem_bytes(pub_pem_bytes: &[u8], data: &[u8], sig: &[u8]) -> Result<()> {
    let pem = std::str::from_utf8(pub_pem_bytes)
        .map_err(|_| "Public key PEM không phải UTF-8 hợp lệ".to_string())?;
    let pub_key = RsaPublicKey::from_pem(pem)?;
    pub_key.verify_pkcs1_sha256(data, sig)
}

#[cfg(windows)]
pub fn generate_rsa_2048_pem() -> Result<String> {
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};

    #[link(name = "bcrypt")]
    extern "system" {
        fn BCryptOpenAlgorithmProvider(
            phAlgorithm: *mut *mut c_void,
            pszAlgId: *const u16,
            pszImplementation: *const u16,
            dwFlags: u32,
        ) -> i32;
        fn BCryptCloseAlgorithmProvider(hAlgorithm: *mut c_void, dwFlags: u32) -> i32;
        fn BCryptGenerateKeyPair(
            hAlgorithm: *mut c_void,
            phKey: *mut *mut c_void,
            dwLength: u32,
            dwFlags: u32,
        ) -> i32;
        fn BCryptFinalizeKeyPair(hKey: *mut c_void, dwFlags: u32) -> i32;
        fn BCryptExportKey(
            hKey: *mut c_void,
            hExportKey: *mut c_void,
            pszBlobType: *const u16,
            pbOutput: *mut u8,
            cbOutput: u32,
            pcbResult: *mut u32,
            dwFlags: u32,
        ) -> i32;
        fn BCryptDestroyKey(hKey: *mut c_void) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    let alg_name = wide("RSA");
    let blob_type = wide("RSAFULLPRIVATEBLOB");
    let mut alg: *mut c_void = null_mut();
    let mut key: *mut c_void = null_mut();
    unsafe {
        let st = BCryptOpenAlgorithmProvider(&mut alg, alg_name.as_ptr(), null(), 0);
        if st < 0 {
            return Err(format!("BCryptOpenAlgorithmProvider failed: {st:#x}"));
        }
        let res = (|| -> Result<Vec<u8>> {
            let st = BCryptGenerateKeyPair(alg, &mut key, 2048, 0);
            if st < 0 {
                return Err(format!("BCryptGenerateKeyPair failed: {st:#x}"));
            }
            let st = BCryptFinalizeKeyPair(key, 0);
            if st < 0 {
                return Err(format!("BCryptFinalizeKeyPair failed: {st:#x}"));
            }
            let mut needed = 0u32;
            let st = BCryptExportKey(
                key,
                null_mut(),
                blob_type.as_ptr(),
                null_mut(),
                0,
                &mut needed,
                0,
            );
            if st < 0 || needed < 24 {
                return Err(format!("BCryptExportKey size query failed: {st:#x}"));
            }
            let mut blob = vec![0u8; needed as usize];
            let mut written = 0u32;
            let st = BCryptExportKey(
                key,
                null_mut(),
                blob_type.as_ptr(),
                blob.as_mut_ptr(),
                needed,
                &mut written,
                0,
            );
            if st < 0 {
                return Err(format!("BCryptExportKey failed: {st:#x}"));
            }
            blob.truncate(written as usize);
            Ok(blob)
        })();
        if !key.is_null() {
            BCryptDestroyKey(key);
        }
        BCryptCloseAlgorithmProvider(alg, 0);
        let blob = res?;
        if blob.len() < 24 || &blob[0..4] != b"RSA3" {
            return Err("Invalid BCRYPT_RSAFULLPRIVATE_BLOB header".into());
        }
        let u32le = |p: usize| u32::from_le_bytes(blob[p..p + 4].try_into().unwrap()) as usize;
        let cb_exp = u32le(8);
        let cb_mod = u32le(12);
        let cb_p1 = u32le(16);
        let cb_p2 = u32le(20);
        let expected = 24 + cb_exp + cb_mod * 2 + cb_p1 * 3 + cb_p2 * 2;
        if blob.len() < expected {
            return Err("Truncated BCRYPT_RSAFULLPRIVATE_BLOB".into());
        }
        let mut pos = 24;
        let mut take = |len: usize| {
            let s = &blob[pos..pos + len];
            pos += len;
            BigUint::from_be_bytes(s)
        };
        let e = take(cb_exp);
        let n = take(cb_mod);
        let p = take(cb_p1);
        let q = take(cb_p2);
        let dp = take(cb_p1);
        let dq = take(cb_p2);
        let qinv = take(cb_p1);
        let d = take(cb_mod);

        let rsa = RsaPrivateKey {
            n,
            e,
            d,
            p,
            q,
            dp,
            dq,
            qinv,
        };
        let test_sig = rsa.sign_pkcs1_sha256(b"isolite-self-test")?;
        rsa.public_key()
            .verify_pkcs1_sha256(b"isolite-self-test", &test_sig)?;
        Ok(rsa.to_pkcs1_pem())
    }
}

#[cfg(not(windows))]
pub fn generate_rsa_2048_pem() -> Result<String> {
    Err("Tạo khóa RSA tự động chỉ hỗ trợ trên Windows; trên Linux hãy dùng: openssl genrsa -out key.pem 2048".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_standard_vectors() {
        let to_hex = |b: [u8; 32]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        assert_eq!(
            to_hex(sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            to_hex(sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[cfg(windows)]
    #[test]
    fn rsa_2048_keygen_sign_and_verify_roundtrip() {
        let priv_pem = generate_rsa_2048_pem().unwrap();
        let priv_key = RsaPrivateKey::from_pem(&priv_pem).unwrap();
        let pub_pem = priv_key.public_key().to_spki_pem();
        let data = b"ed37d971653b2a8b973fd5ccdabd15d6  ./boot/grub/efi.img\n";
        let sig = priv_key.sign_pkcs1_sha256(data).unwrap();
        assert_eq!(sig.len(), 256);
        verify_with_pem_bytes(pub_pem.as_bytes(), data, &sig).unwrap();

        let mut tampered = data.to_vec();
        tampered[0] ^= 0x01;
        assert!(verify_with_pem_bytes(pub_pem.as_bytes(), &tampered, &sig).is_err());
    }

    #[test]
    fn verifies_real_nfs_iso_signature_if_present() {
        let iso = std::path::Path::new("NFSDesktop-5.0-G240-251209-amd64.iso");
        if !iso.exists() {
            return;
        }
        let mut r = crate::iso::reader::IsoReader::new(iso).unwrap();
        let files = r.files().unwrap();
        let read_iso = |target: &str| {
            let ext = files.iter().find(|(p, _)| p == target).unwrap().1;
            let mut f = std::fs::File::open(iso).unwrap();
            use std::io::{Read, Seek, SeekFrom};
            let mut b = vec![0u8; ext.size as usize];
            f.seek(SeekFrom::Start(ext.offset)).unwrap();
            f.read_exact(&mut b).unwrap();
            b
        };
        let md5sum = read_iso("/md5sum.txt");
        let sig = read_iso("/.md5sum.txt");
        let pub_pem = read_iso("/.public_key.pem");
        verify_with_pem_bytes(&pub_pem, &md5sum, &sig).unwrap();
    }
}
