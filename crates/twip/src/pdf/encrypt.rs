//! Password protection for the PDFs `export` writes: the standard security handler, revision 6 (AES-256, ISO
//! 32000-2 §7.6.4). Every string and stream is encrypted with one random file key; the open password unlocks it
//! through `/U` and `/UE`. The owner password is random and never given out, and every permission is granted.
//!
//! The input is twip-graphics' PDF output, whose shape this relies on: a classic xref table, no object streams, each stream's
//! `/Length` a direct integer, and a trailer with `/Size` and `/ID`.
// shortcut: passwords aren't SASLprep-normalized; ASCII and NFC passwords match what readers compute, compatibility
// characters may not. Upgrade: a SASLprep table.

#[cfg(test)]
use aes::cipher::BlockDecrypt;
use aes::cipher::{BlockEncrypt, KeyInit, consts::U16, generic_array::GenericArray};
use aes::{Aes128, Aes256};
use sha2::{Digest, Sha256, Sha384, Sha512};

use crate::api::error::{Error, ErrorCode};

/// The XMP lines that claim PDF/A conformance, which an encrypted file can't have.
const ARCHIVAL_CLAIM: [(&[u8], &[u8]); 2] = [(b"<pdfaid:part>", b"</pdfaid:part>"), (b"<pdfaid:conformance>", b"</pdfaid:conformance>")];

/// `pdf` encrypted so it opens only with `password`, without its PDF/A conformance claim.
pub(crate) fn encrypt(pdf: &[u8], password: &str) -> Result<Vec<u8>, Error> {
    let parsed = parse(pdf).ok_or_else(malformed)?;
    let key: [u8; 32] = random();
    let mut out = parsed.header.to_vec();
    let mut offsets: Vec<(u32, usize)> = Vec::with_capacity(parsed.objects.len() + 1);
    for object in &parsed.objects {
        offsets.push((object.num, out.len()));
        out.extend_from_slice(format!("{} {} obj\n", object.num, object.generation).as_bytes());
        let stream = object.stream.map(|data| {
            let data = if data.windows(ARCHIVAL_CLAIM[0].0.len()).any(|w| w == ARCHIVAL_CLAIM[0].0) { without_claim(data) } else { data.to_vec() };
            aes_cbc_encrypt(&key, &data)
        });
        emit(&object.body, &mut out, &mut |s| aes_cbc_encrypt(&key, s), stream.as_ref().map(Vec::len));
        if let Some(stream) = stream {
            out.extend_from_slice(b"\nstream\n");
            out.extend_from_slice(&stream);
            out.extend_from_slice(b"\nendstream");
        }
        out.extend_from_slice(b"\nendobj\n");
    }
    let size = trailer_int(&parsed.trailer, b"/Size").ok_or_else(malformed)?.max(parsed.objects.iter().map(|o| o.num + 1).max().unwrap_or(1));
    let encrypt_num = size;
    offsets.push((encrypt_num, out.len()));
    out.extend_from_slice(format!("{encrypt_num} 0 obj\n").as_bytes());
    out.extend_from_slice(&dictionary(&key, password));
    out.extend_from_slice(b"\nendobj\n");

    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", encrypt_num + 1).as_bytes());
    for num in 1..=encrypt_num {
        match offsets.iter().find(|(n, _)| *n == num) {
            Some((_, at)) => out.extend_from_slice(format!("{at:010} 00000 n \n").as_bytes()),
            None => out.extend_from_slice(b"0000000000 65535 f \n"),
        }
    }
    out.extend_from_slice(b"trailer\n");
    let has_id = parsed.trailer.iter().any(|t| matches!(t, Tok::Name(n) if *n == b"/ID"));
    let mut trailer = Vec::new();
    emit(&parsed.trailer, &mut trailer, &mut |s| s.to_vec(), None);
    // The trailer's last token closes its dictionary: the new entries go before it, and `/Size` grows by one.
    let close = trailer.iter().rposition(|&b| b == b'>').and_then(|i| i.checked_sub(1)).ok_or_else(malformed)?;
    let mut extra = format!("/Encrypt {encrypt_num} 0 R").into_bytes();
    if !has_id {
        let id: [u8; 16] = random();
        let hex = hex(&id);
        extra.extend_from_slice(format!("/ID [<{hex}> <{hex}>]").as_bytes());
    }
    trailer.splice(close..close, extra);
    out.extend_from_slice(&replace_int(&trailer, b"/Size", encrypt_num + 1));
    out.extend_from_slice(format!("\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    Ok(out)
}

fn malformed() -> Error {
    Error::new(ErrorCode::ExportFailed, "the PDF couldn't be encrypted")
}

/// `data` with the archival claim's elements and their line breaks removed.
fn without_claim(data: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    for (open, close) in ARCHIVAL_CLAIM {
        if let Some(start) = out.windows(open.len()).position(|w| w == open)
            && let Some(len) = out[start..].windows(close.len()).position(|w| w == close)
        {
            let mut end = start + len + close.len();
            if out.get(end) == Some(&b'\n') {
                end += 1;
            }
            out.drain(start..end);
        }
    }
    out
}

/// The `/Encrypt` dictionary for `key` behind `password`.
fn dictionary(key: &[u8; 32], password: &str) -> Vec<u8> {
    let user = &password.as_bytes()[..password.len().min(127)];
    let (validation, key_salt): ([u8; 8], [u8; 8]) = (random(), random());
    let mut u = hash(user, &validation, &[]).to_vec();
    u.extend_from_slice(&validation);
    u.extend_from_slice(&key_salt);
    let ue = aes_cbc_no_pad(&hash(user, &key_salt, &[]), key);
    // An owner password nobody holds: permissions aren't restricted, so it never needs giving out.
    let owner: [u8; 32] = random();
    let (o_validation, o_key_salt): ([u8; 8], [u8; 8]) = (random(), random());
    let mut o = hash(&owner, &o_validation, &u).to_vec();
    o.extend_from_slice(&o_validation);
    o.extend_from_slice(&o_key_salt);
    let oe = aes_cbc_no_pad(&hash(&owner, &o_key_salt, &u), key);
    let p: i32 = -4;
    let mut perms = [0u8; 16];
    perms[..4].copy_from_slice(&p.to_le_bytes());
    perms[4..8].fill(0xFF);
    perms[8..12].copy_from_slice(b"Tadb");
    perms[12..].copy_from_slice(&random::<4>());
    let cipher = Aes256::new(GenericArray::from_slice(key));
    let mut block = GenericArray::clone_from_slice(&perms);
    cipher.encrypt_block(&mut block);
    format!(
        "<</Filter /Standard /V 5 /R 6 /Length 256 /CF <</StdCF <</AuthEvent /DocOpen /CFM /AESV3 /Length 32>>>> /StmF /StdCF /StrF /StdCF /P {p} /U <{}> /UE <{}> /O <{}> /OE <{}> /Perms <{}>>>",
        hex(&u),
        hex(&ue),
        hex(&o),
        hex(&oe),
        hex(&block)
    )
    .into_bytes()
}

/// ISO 32000-2 hash algorithm 2.B: a password and salt (and for the owner keys, `/U`) stretched into a 32-byte key.
pub(crate) fn hash(password: &[u8], salt: &[u8], udata: &[u8]) -> [u8; 32] {
    let mut k: Vec<u8> = Sha256::new().chain_update(password).chain_update(salt).chain_update(udata).finalize().to_vec();
    let mut round = 0u32;
    loop {
        let mut once = Vec::with_capacity(password.len() + k.len() + udata.len());
        once.extend_from_slice(password);
        once.extend_from_slice(&k);
        once.extend_from_slice(udata);
        let k1 = once.repeat(64);
        let e = aes128_cbc_no_pad(&k[..16], &k[16..32], &k1);
        k = match e[..16].iter().map(|&b| u32::from(b)).sum::<u32>() % 3 {
            0 => Sha256::digest(&e).to_vec(),
            1 => Sha384::digest(&e).to_vec(),
            _ => Sha512::digest(&e).to_vec(),
        };
        round += 1;
        if round >= 64 && u32::from(*e.last().expect("64 copies are never empty")) <= round - 32 {
            break;
        }
    }
    k[..32].try_into().expect("every SHA-2 digest has 32 bytes or more")
}

/// CBC over whole blocks (`data` is a multiple of 16 bytes), chaining from `iv`.
fn cbc<C: BlockEncrypt<BlockSize = U16>>(cipher: &C, iv: [u8; 16], data: &[u8]) -> Vec<u8> {
    let mut prev = iv;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.as_chunks::<16>().0 {
        let mut block = GenericArray::from(*chunk);
        block.iter_mut().zip(prev).for_each(|(b, p)| *b ^= p);
        cipher.encrypt_block(&mut block);
        prev = block.into();
        out.extend_from_slice(&prev);
    }
    out
}

/// The inverse of [`cbc`].
#[cfg(test)]
fn cbc_decrypt<C: BlockDecrypt<BlockSize = U16>>(cipher: &C, iv: [u8; 16], data: &[u8]) -> Vec<u8> {
    let mut prev = iv;
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.as_chunks::<16>().0 {
        let mut block = GenericArray::from(*chunk);
        cipher.decrypt_block(&mut block);
        block.iter_mut().zip(prev).for_each(|(b, p)| *b ^= p);
        prev = *chunk;
        out.extend_from_slice(&block);
    }
    out
}

/// AES-128-CBC over whole blocks.
fn aes128_cbc_no_pad(key: &[u8], iv: &[u8], data: &[u8]) -> Vec<u8> {
    cbc(&Aes128::new(GenericArray::from_slice(key)), iv.try_into().expect("a 16-byte IV"), data)
}

/// AES-256-CBC of 32 bytes with a zero IV and no padding: how `/UE` and `/OE` hold the file key.
fn aes_cbc_no_pad(key: &[u8; 32], data: &[u8; 32]) -> [u8; 32] {
    cbc(&Aes256::new(GenericArray::from_slice(key)), [0; 16], data).try_into().expect("two blocks in, two out")
}

/// A string or stream as AESV3 stores it: a random IV, then AES-256-CBC with PKCS#7 padding.
fn aes_cbc_encrypt(key: &[u8; 32], data: &[u8]) -> Vec<u8> {
    let iv: [u8; 16] = random();
    let pad = 16 - data.len() % 16;
    let mut padded = data.to_vec();
    padded.resize(data.len() + pad, pad as u8);
    [iv.as_slice(), &cbc(&Aes256::new(GenericArray::from_slice(key)), iv, &padded)].concat()
}

/// The inverse of [`aes_cbc_encrypt`]; `None` for data that isn't one.
#[cfg(test)]
pub(crate) fn aes_cbc_decrypt(key: &[u8; 32], data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 32 || !data.len().is_multiple_of(16) {
        return None;
    }
    let mut out = cbc_decrypt(&Aes256::new(GenericArray::from_slice(key)), data[..16].try_into().ok()?, &data[16..]);
    let pad = *out.last()? as usize;
    (1..=16).contains(&pad).then(|| out.truncate(out.len() - pad)).map(|()| out)
}

/// The file key `/UE` holds, unlocked with `password`; `None` when the password doesn't validate against `/U`.
#[cfg(test)]
pub(crate) fn file_key(u: &[u8], ue: &[u8], password: &str) -> Option<[u8; 32]> {
    let user = &password.as_bytes()[..password.len().min(127)];
    if u.len() != 48 || ue.len() != 32 || hash(user, &u[32..40], &[]) != u[..32] {
        return None;
    }
    cbc_decrypt(&Aes256::new(GenericArray::from_slice(&hash(user, &u[40..48], &[]))), [0; 16], ue).try_into().ok()
}

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("the operating system provides random bytes");
    b
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02X}")).collect()
}

// ---------------- reading twip-graphics' PDF ----------------

/// A token of an object or the trailer. Everything but strings is kept as written.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Tok<'a> {
    /// Whitespace, delimiters, numbers, keywords and references, as written.
    Raw(&'a [u8]),
    /// A name, with its slash.
    Name(&'a [u8]),
    /// An integer, as written.
    Int(&'a [u8]),
    /// A literal or hex string, decoded.
    Str(Vec<u8>),
}

pub(crate) struct Object<'a> {
    pub num: u32,
    pub generation: u32,
    pub body: Vec<Tok<'a>>,
    pub stream: Option<&'a [u8]>,
}

pub(crate) struct Parsed<'a> {
    pub header: &'a [u8],
    pub objects: Vec<Object<'a>>,
    pub trailer: Vec<Tok<'a>>,
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\n' | b'\r' | b'\t' | b'\x0C' | 0)
}

fn is_delim(b: u8) -> bool {
    matches!(b, b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%')
}

fn skip_space(d: &[u8], mut i: usize) -> usize {
    while i < d.len() && is_space(d[i]) {
        i += 1;
    }
    i
}

/// The objects and trailer of a PDF with a classic xref table.
pub(crate) fn parse(d: &[u8]) -> Option<Parsed<'_>> {
    // The header is two comment lines: the version, then binary marker bytes.
    let mut i = 0;
    for _ in 0..2 {
        i += d[i..].iter().position(|&b| b == b'\n')? + 1;
    }
    let header = &d[..i];
    let mut objects = Vec::new();
    loop {
        i = skip_space(d, i);
        if d[i..].starts_with(b"xref") {
            break;
        }
        let (num, next) = int(d, i)?;
        let (generation, next) = int(d, skip_space(d, next))?;
        let next = skip_space(d, next);
        if !d[next..].starts_with(b"obj") {
            return None;
        }
        let (body, next) = value(d, next + 3)?;
        let mut j = skip_space(d, next);
        let mut stream = None;
        if d[j..].starts_with(b"stream") {
            j += 6;
            j += if d[j..].starts_with(b"\r\n") { 2 } else { 1 };
            let len = length(&body)?;
            stream = Some(d.get(j..j + len)?);
            j = skip_space(d, j + len);
            if !d[j..].starts_with(b"endstream") {
                return None;
            }
            j = skip_space(d, j + 9);
        }
        if !d[j..].starts_with(b"endobj") {
            return None;
        }
        objects.push(Object { num: num as u32, generation: generation as u32, body, stream });
        i = j + 6;
    }
    let t = i + d[i..].windows(7).position(|w| w == b"trailer")? + 7;
    let (trailer, _) = value(d, t)?;
    Some(Parsed { header, objects, trailer })
}

fn int(d: &[u8], i: usize) -> Option<(u64, usize)> {
    let n = d[i..].iter().take_while(|b| b.is_ascii_digit()).count();
    let v = std::str::from_utf8(&d[i..i + n]).ok()?.parse().ok()?;
    Some((v, i + n))
}

/// The tokens of one value starting at `i` (after leading whitespace), and where it ends.
fn value(d: &[u8], mut i: usize) -> Option<(Vec<Tok<'_>>, usize)> {
    let mut toks = Vec::new();
    let mut depth = 0i32;
    let mut started = false;
    loop {
        let s = i;
        i = skip_space(d, i);
        if i > s {
            toks.push(Tok::Raw(&d[s..i]));
        }
        if started && depth == 0 {
            // Trailing whitespace belongs to what follows.
            if matches!(toks.last(), Some(Tok::Raw(r)) if r.iter().all(|&b| is_space(b))) {
                toks.pop();
                i = s;
            }
            return Some((toks, i));
        }
        let b = *d.get(i)?;
        started = true;
        match b {
            b'<' if d.get(i + 1) == Some(&b'<') => {
                toks.push(Tok::Raw(&d[i..i + 2]));
                depth += 1;
                i += 2;
            }
            b'>' if d.get(i + 1) == Some(&b'>') => {
                toks.push(Tok::Raw(&d[i..i + 2]));
                depth -= 1;
                i += 2;
            }
            b'[' => {
                toks.push(Tok::Raw(&d[i..i + 1]));
                depth += 1;
                i += 1;
            }
            b']' => {
                toks.push(Tok::Raw(&d[i..i + 1]));
                depth -= 1;
                i += 1;
            }
            b'(' => {
                let (s, next) = literal(d, i + 1)?;
                toks.push(Tok::Str(s));
                i = next;
            }
            b'<' => {
                let end = i + d[i..].iter().position(|&c| c == b'>')?;
                let digits: Vec<u8> = d[i + 1..end].iter().copied().filter(u8::is_ascii_hexdigit).collect();
                let nibble = |c: u8| (c as char).to_digit(16).unwrap_or(0) as u8;
                let bytes = digits.chunks(2).map(|p| nibble(p[0]) << 4 | p.get(1).map_or(0, |&c| nibble(c))).collect();
                toks.push(Tok::Str(bytes));
                i = end + 1;
            }
            b'/' => {
                let n = 1 + d[i + 1..].iter().take_while(|&&c| !is_space(c) && !is_delim(c)).count();
                toks.push(Tok::Name(&d[i..i + n]));
                i += n;
            }
            _ => {
                let n = d[i..].iter().take_while(|&&c| !is_space(c) && !is_delim(c)).count().max(1);
                let word = &d[i..i + n];
                toks.push(if word.iter().all(u8::is_ascii_digit) { Tok::Int(word) } else { Tok::Raw(word) });
                i += n;
            }
        }
    }
}

/// A literal string's bytes from just after its `(`, and where it ends.
fn literal(d: &[u8], mut i: usize) -> Option<(Vec<u8>, usize)> {
    let mut out = Vec::new();
    let mut depth = 0;
    loop {
        let b = *d.get(i)?;
        i += 1;
        match b {
            b'\\' => {
                let e = *d.get(i)?;
                i += 1;
                match e {
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'b' => out.push(8),
                    b'f' => out.push(12),
                    b'0'..=b'7' => {
                        let mut v = u32::from(e - b'0');
                        for _ in 0..2 {
                            match d.get(i) {
                                Some(&c @ b'0'..=b'7') => {
                                    v = v * 8 + u32::from(c - b'0');
                                    i += 1;
                                }
                                _ => break,
                            }
                        }
                        out.push(v as u8);
                    }
                    b'\r' => {
                        if d.get(i) == Some(&b'\n') {
                            i += 1;
                        }
                    }
                    b'\n' => {}
                    other => out.push(other),
                }
            }
            b'(' => {
                depth += 1;
                out.push(b);
            }
            b')' if depth == 0 => return Some((out, i)),
            b')' => {
                depth -= 1;
                out.push(b);
            }
            b'\r' => {
                if d.get(i) == Some(&b'\n') {
                    i += 1;
                }
                out.push(b'\n');
            }
            _ => out.push(b),
        }
    }
}

/// A stream dictionary's `/Length`.
fn length(toks: &[Tok]) -> Option<usize> {
    int_after(toks, b"/Length").map(|v| v as usize)
}

fn trailer_int(toks: &[Tok], key: &[u8]) -> Option<u32> {
    int_after(toks, key).map(|v| v as u32)
}

/// The integer that follows `key` at the outermost dictionary level.
fn int_after(toks: &[Tok], key: &[u8]) -> Option<u64> {
    let mut depth = 0;
    let mut want = false;
    for t in toks {
        match t {
            Tok::Raw(b"<<") | Tok::Raw(b"[") => depth += 1,
            Tok::Raw(b">>") | Tok::Raw(b"]") => depth -= 1,
            Tok::Name(n) if depth == 1 => want = *n == key,
            Tok::Int(v) if want && depth == 1 => return std::str::from_utf8(v).ok()?.parse().ok(),
            Tok::Raw(r) if r.iter().all(|&b| is_space(b)) => {}
            _ => want = false,
        }
    }
    None
}

/// Writes `toks`, each string through `f` as a hex string, and the outermost `/Length` as `length` when given.
fn emit(toks: &[Tok], out: &mut Vec<u8>, f: &mut dyn FnMut(&[u8]) -> Vec<u8>, length: Option<usize>) {
    let mut depth = 0;
    let mut after_length = false;
    for t in toks {
        match t {
            Tok::Raw(r) => {
                match *r {
                    b"<<" | b"[" => depth += 1,
                    b">>" | b"]" => depth -= 1,
                    _ => {}
                }
                out.extend_from_slice(r);
            }
            Tok::Name(n) => {
                after_length = depth == 1 && *n == b"/Length";
                out.extend_from_slice(n);
            }
            Tok::Int(v) => {
                match length.filter(|_| after_length) {
                    Some(len) => out.extend_from_slice(len.to_string().as_bytes()),
                    None => out.extend_from_slice(v),
                }
                after_length = false;
            }
            Tok::Str(s) => {
                out.push(b'<');
                out.extend_from_slice(hex(&f(s)).as_bytes());
                out.push(b'>');
            }
        }
    }
}

/// `dict` (written) with the integer after `key` replaced by `v`.
fn replace_int(dict: &[u8], key: &[u8], v: u32) -> Vec<u8> {
    let Some(at) = dict.windows(key.len()).position(|w| w == key) else { return dict.to_vec() };
    let start = skip_space(dict, at + key.len());
    let end = start + dict[start..].iter().take_while(|b| b.is_ascii_digit()).count();
    [&dict[..start], v.to_string().as_bytes(), &dict[end..]].concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::session::PdfOptions;

    /// A fixture's PDF, unprotected.
    fn plain() -> Vec<u8> {
        let (_, bytes) = crate::pdf::tests::fixtures().into_iter().find(|f| f.0 == "text-basics.docx").expect("the fixture");
        crate::pdf::convert(&bytes, &crate::pdf::FontSet::system(), &PdfOptions::default()).expect("converts").bytes
    }

    /// The trailer's `/Encrypt` dictionary's string entries, by key.
    fn encryption(parsed: &Parsed) -> Vec<(Vec<u8>, Vec<u8>)> {
        let num = trailer_int(&parsed.trailer, b"/Encrypt").expect("an /Encrypt entry");
        let object = parsed.objects.iter().find(|o| o.num == num).expect("the /Encrypt object");
        let mut out = Vec::new();
        let mut key = None;
        for t in &object.body {
            match t {
                Tok::Name(n) => key = Some(n.to_vec()),
                Tok::Str(s) => out.push((key.take().unwrap_or_default(), s.clone())),
                _ => {}
            }
        }
        out
    }

    /// Every string and stream of `pdf`'s objects but `skip`, decrypted with `f`.
    fn contents(parsed: &Parsed, skip: Option<u32>, f: &dyn Fn(&[u8]) -> Vec<u8>) -> Vec<(u32, Vec<Vec<u8>>, Option<Vec<u8>>)> {
        parsed
            .objects
            .iter()
            .filter(|o| Some(o.num) != skip)
            .map(|o| {
                let strings = o.body.iter().filter_map(|t| if let Tok::Str(s) = t { Some(f(s)) } else { None }).collect();
                (o.num, strings, o.stream.map(f))
            })
            .collect()
    }

    #[test]
    fn hash_follows_algorithm_2b() {
        // Two runs agree, salts change the key, and the result is 32 bytes.
        let a = hash(b"test", b"12345678", &[]);
        assert_eq!(a, hash(b"test", b"12345678", &[]));
        assert_ne!(a, hash(b"test", b"87654321", &[]));
        assert_ne!(a, hash(b"test", b"12345678", &[0; 48]));
    }

    #[test]
    fn a_protected_pdf_decrypts_to_the_plain_one_with_the_password() {
        let plain = plain();
        for password in ["secret", "s3cret ü"] {
            let locked = encrypt(&plain, password).expect("encrypts");
            assert!(!locked.windows(7).any(|w| w == b"pdfaid:"), "no archival claim, even encrypted");
            let parsed = parse(&locked).expect("the output parses");
            let entries = encryption(&parsed);
            let get = |k: &[u8]| entries.iter().find(|e| e.0 == k).map(|e| e.1.clone()).expect("the entry");
            let (u, ue) = (get(b"/U"), get(b"/UE"));
            assert!(file_key(&u, &ue, "wrong").is_none(), "a wrong password doesn't validate");
            let key = file_key(&u, &ue, password).expect("the password validates");
            let skip = trailer_int(&parsed.trailer, b"/Encrypt");
            let opened = contents(&parsed, skip, &|s| aes_cbc_decrypt(&key, s).expect("decrypts"));
            let original = parse(&plain).expect("parses");
            let expected = contents(&original, None, &|s| if s.windows(13).any(|w| w == b"<pdfaid:part>") { without_claim(s) } else { s.to_vec() });
            assert_eq!(opened, expected, "{password}: every string and stream decrypts to the original");
        }
    }

    #[test]
    fn the_output_is_a_well_formed_file() {
        let locked = encrypt(&plain(), "secret").expect("encrypts");
        assert!(parse(&locked).expect("parses").objects.len() > 3);
        // `startxref` points at the table, and every in-use entry at its object.
        let find = |needle: &[u8]| locked.windows(needle.len()).rposition(|w| w == needle).expect("present");
        let xref = find(
            b"
xref
",
        ) + 1;
        let after = find(
            b"startxref
",
        ) + 10;
        let digits = |at: usize| {
            std::str::from_utf8(&locked[at..at + locked[at..].iter().take_while(|b| b.is_ascii_digit()).count()]).unwrap().parse::<usize>().unwrap()
        };
        assert_eq!(digits(after), xref);
        let table = std::str::from_utf8(&locked[xref..after - 10]).expect("the table and trailer are text");
        for (k, line) in table.lines().skip(3).take_while(|l| l.ends_with(" n ") || l.ends_with(" f ")).enumerate() {
            if line.ends_with(" n ") {
                let at: usize = line[..10].parse().unwrap();
                assert!(locked[at..].starts_with(format!("{} 0 obj", k + 1).as_bytes()), "object {}", k + 1);
            }
        }
    }
}
