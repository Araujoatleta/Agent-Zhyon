// HTTP Digest authentication (RFC 7616, and RFC 2617 before it), which Philips TVs ask for once paired, with the MD5
// it needs. ring, already here, has SHA-256 and HMAC but no MD5, and a whole crate for one legacy hash is not worth
// it: MD5 here only answers the TV's digest challenge, it protects nothing else. Checked below against the RFC test
// vectors. Also the standard base64 the pairing signature is written in.

use ring::digest;

const S: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
    4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

/// floor(|sin(i + 1)| × 2³²), RFC 1321.
const K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501, 0x698098d8, 0x8b44f7af,
    0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa,
    0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8,
    0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244, 0x432aff97,
    0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1, 0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1,
    0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

/// MD5 of `data` (RFC 1321).
pub fn md5(data: &[u8]) -> [u8; 16] {
    let mut state: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    let mut message = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_le_bytes());
    for block in message.chunks_exact(64) {
        let mut m = [0u32; 16];
        for (i, word) in block.chunks_exact(4).enumerate() {
            m[i] = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
        }
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(K[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        for (s, v) in state.iter_mut().zip([a, b, c, d]) {
            *s = s.wrapping_add(v);
        }
    }
    let mut out = [0u8; 16];
    for (i, s) in state.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&s.to_le_bytes());
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The hash a challenge names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Md5,
    Sha256,
}

impl Algorithm {
    fn hash(self, text: &str) -> String {
        match self {
            Algorithm::Md5 => hex(&md5(text.as_bytes())),
            Algorithm::Sha256 => hex(digest::digest(&digest::SHA256, text.as_bytes()).as_ref()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Algorithm::Md5 => "MD5",
            Algorithm::Sha256 => "SHA-256",
        }
    }
}

/// What the server asked for in `WWW-Authenticate: Digest …`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub realm: String,
    pub nonce: String,
    pub opaque: Option<String>,
    /// The server offered `qop=auth` (the only protection level used here).
    pub qop_auth: bool,
    pub algorithm: Algorithm,
    /// The challenge named an algorithm, so the answer names it too.
    named: bool,
}

/// `key="value", key=value…`, commas inside quotes kept.
fn params(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let Some(eq) = rest.find('=') else { break };
        let key = rest[..eq].trim().trim_start_matches(',').trim().to_ascii_lowercase();
        rest = rest[eq + 1..].trim_start();
        let value;
        if let Some(stripped) = rest.strip_prefix('"') {
            let mut v = String::new();
            let mut chars = stripped.char_indices();
            let mut end = stripped.len();
            while let Some((i, c)) = chars.next() {
                match c {
                    '\\' => {
                        if let Some((_, next)) = chars.next() {
                            v.push(next);
                        }
                    }
                    '"' => {
                        end = i + 1;
                        break;
                    }
                    c => v.push(c),
                }
            }
            value = v;
            rest = stripped[end.min(stripped.len())..].trim_start();
        } else {
            let end = rest.find(',').unwrap_or(rest.len());
            value = rest[..end].trim().to_string();
            rest = &rest[end..];
        }
        rest = rest.trim_start_matches(|c: char| c == ',' || c.is_whitespace());
        out.push((key, value));
    }
    out
}

impl Challenge {
    /// The Digest challenge in a `WWW-Authenticate` header, if it is one this module can answer.
    pub fn parse(header: &str) -> Option<Challenge> {
        let header = header.trim();
        let rest = header.get(..7).filter(|s| s.eq_ignore_ascii_case("digest "))?;
        let fields = params(&header[rest.len()..]);
        let get = |k: &str| fields.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone());
        let (algorithm, named) = match get("algorithm").map(|a| a.to_ascii_uppercase()) {
            None => (Algorithm::Md5, false),
            Some(a) if a == "MD5" => (Algorithm::Md5, true),
            Some(a) if a == "SHA-256" => (Algorithm::Sha256, true),
            // -sess variants and anything else: not answered (the request fails with the TV's 401).
            Some(_) => return None,
        };
        let qop_auth = get("qop").is_some_and(|q| q.split(',').any(|p| p.trim().eq_ignore_ascii_case("auth")));
        if get("qop").is_some() && !qop_auth {
            return None;
        }
        Some(Challenge { realm: get("realm").unwrap_or_default(), nonce: get("nonce")?, opaque: get("opaque"), qop_auth, algorithm, named })
    }

    /// The `Authorization` header answering this challenge for one request. `uri` is the request target (the path).
    pub fn answer(&self, user: &str, password: &str, method: &str, uri: &str, cnonce: &str, nc: u32) -> String {
        let a = self.algorithm;
        let ha1 = a.hash(&format!("{user}:{}:{password}", self.realm));
        let ha2 = a.hash(&format!("{method}:{uri}"));
        let nc = format!("{nc:08x}");
        let response = if self.qop_auth {
            a.hash(&format!("{ha1}:{}:{nc}:{cnonce}:auth:{ha2}", self.nonce))
        } else {
            a.hash(&format!("{ha1}:{}:{ha2}", self.nonce))
        };
        let quote = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let mut header = format!(
            "Digest username=\"{}\", realm=\"{}\", nonce=\"{}\", uri=\"{}\", response=\"{response}\"",
            quote(user),
            quote(&self.realm),
            quote(&self.nonce),
            quote(uri)
        );
        if self.named {
            header.push_str(&format!(", algorithm={}", a.name()));
        }
        if let Some(opaque) = &self.opaque {
            header.push_str(&format!(", opaque=\"{}\"", quote(opaque)));
        }
        if self.qop_auth {
            header.push_str(&format!(", qop=auth, nc={nc}, cnonce=\"{}\"", quote(cnonce)));
        }
        header
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_matches_the_rfc_1321_test_suite() {
        for (input, expected) in [
            ("", "d41d8cd98f00b204e9800998ecf8427e"),
            ("a", "0cc175b9c0f1b6a831c399e269772661"),
            ("abc", "900150983cd24fb0d6963f7d28e17f72"),
            ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
            ("abcdefghijklmnopqrstuvwxyz", "c3fcd3d76192e4007dfb496cca67e13b"),
            ("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789", "d174ab98d277d9f5a5611c2c9f419d9f"),
            ("12345678901234567890123456789012345678901234567890123456789012345678901234567890", "57edf4a22be3c955ac49da2e2107b67a"),
        ] {
            assert_eq!(hex(&md5(input.as_bytes())), expected, "{input:?}");
        }
    }

    #[test]
    fn the_rfc_2617_example_is_answered_as_written() {
        let challenge = Challenge::parse(
            r#"Digest realm="testrealm@host.com", qop="auth,auth-int", nonce="dcd98b7102dd2f0e8b11d0f600bfb0c093", opaque="5ccc069c403ebaf9f0171e9517f40e41""#,
        )
        .unwrap();
        assert_eq!(challenge.realm, "testrealm@host.com");
        assert!(challenge.qop_auth);
        let header = challenge.answer("Mufasa", "Circle Of Life", "GET", "/dir/index.html", "0a4f113b", 1);
        assert!(header.contains(r#"response="6629fae49393a05397450978507c4ef1""#), "{header}");
        assert!(header.contains("nc=00000001") && header.contains(r#"cnonce="0a4f113b""#) && header.contains("qop=auth"));
        assert!(header.contains(r#"opaque="5ccc069c403ebaf9f0171e9517f40e41""#));
        assert!(!header.contains("algorithm"), "not named by the server, not named back");
    }

    #[test]
    fn the_rfc_7616_sha_256_example_is_answered_as_written() {
        let challenge = Challenge::parse(
            r#"Digest realm="http-auth@example.org", qop="auth, auth-int", algorithm=SHA-256, nonce="7ypf/xlj9XXwfDPEoM4URrv/xwf94BcCAzFZH4GiTo0v", opaque="FQhe/qaU925kfnzjCev0ciny7QMkPqMAFRtzCUYo5tdS""#,
        )
        .unwrap();
        assert_eq!(challenge.algorithm, Algorithm::Sha256);
        let header = challenge.answer("Mufasa", "Circle of Life", "GET", "/dir/index.html", "f2/wE4q74E6zIJEtWaHKaf5wv/H5QzzpXusqGemxURZJ", 1);
        assert!(header.contains(r#"response="753927fa0e85d155564e2e272a28d1802ca10daf4496794697cf8db5856cb6c1""#), "{header}");
        assert!(header.contains("algorithm=SHA-256"));
    }

    #[test]
    fn challenges_this_module_cannot_answer_are_refused() {
        assert!(Challenge::parse(r#"Basic realm="x""#).is_none());
        assert!(Challenge::parse(r#"Digest realm="x", nonce="n", algorithm=MD5-sess"#).is_none());
        assert!(Challenge::parse(r#"Digest realm="x", nonce="n", qop="auth-int""#).is_none());
        assert!(Challenge::parse(r#"Digest realm="x""#).is_none(), "no nonce");
        // No qop at all: the RFC 2069 form, still answered.
        let old = Challenge::parse(r#"digest realm="x", nonce="n""#).unwrap();
        assert!(!old.qop_auth);
        assert!(!old.answer("u", "p", "POST", "/6/input/key", "c", 1).contains("qop"));
    }

    #[test]
    fn base64_is_the_standard_alphabet_with_padding() {
        for (input, expected) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")] {
            assert_eq!(base64(input.as_bytes()), expected);
        }
        assert_eq!(base64(&[0xfb, 0xff]), "+/8=");
    }
}
