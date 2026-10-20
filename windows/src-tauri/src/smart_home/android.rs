// Android TVs, projectors and TV boxes over ADB, Android's debugging bridge, on TCP port 5555 (AOSP
// packages/modules/adb: protocol.txt, auth.cpp, android_pubkey.cpp). Coucou uses it as it uses the other TVs: a fixed
// list of commands (keys, the apps of the device's own list, reading its state), never a command typed by anyone,
// the model included. The device asks once on its screen whether to allow this computer ("Allow USB debugging?");
// the key it asks for, an RSA-2048 one as adb makes it, is made here and kept in the OS keystore, never in a file.
//
// Only the classic network debugging (port 5555, no TLS) is spoken: Android 11's wireless debugging, with its pairing
// code and TLS, is said to be unsupported rather than tried.

use std::io;
use std::time::Duration;

use num_bigint::BigUint;
use regex::Regex;
use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

use super::config;
use super::philips::TvError;
use super::samsung::App;
use crate::i18n::t;
use crate::{net, secrets};

pub const PORT: u16 = 5555;

const CNXN: u32 = u32::from_le_bytes(*b"CNXN");
const AUTH: u32 = u32::from_le_bytes(*b"AUTH");
const OPEN: u32 = u32::from_le_bytes(*b"OPEN");
const OKAY: u32 = u32::from_le_bytes(*b"OKAY");
const CLSE: u32 = u32::from_le_bytes(*b"CLSE");
const WRTE: u32 = u32::from_le_bytes(*b"WRTE");
const STLS: u32 = u32::from_le_bytes(*b"STLS");

/// The protocol's version that made checksums optional; they are still sent, for the devices from before it.
const VERSION: u32 = 0x0100_0001;
/// The largest payload this side announces.
const MAX_PAYLOAD: u32 = 256 * 1024;
/// A payload larger than this is not ADB (newer devices send up to 1 MiB).
const MAX_ACCEPTED: usize = 1024 * 1024;
const TOKEN: u32 = 1;
const SIGNATURE: u32 = 2;
const PUBLIC_KEY: u32 = 3;
/// A command's output is kept up to this much (dumpsys on a busy device); the rest is read and dropped.
const MAX_OUTPUT: usize = 4 * 1024 * 1024;

const CONNECT_WAIT: Duration = Duration::from_secs(3);
const ANSWER_WAIT: Duration = Duration::from_secs(8);
/// How long a command may run (dumpsys takes a moment on a small device).
const COMMAND_WAIT: Duration = Duration::from_secs(15);
/// How long the device's question ("Allow USB debugging?") is waited for, once this computer's key was offered.
pub const ALLOW_WAIT: Duration = Duration::from_secs(60);

// ── Messages ──────────────────────────────────────────────────────────────────

/// One message: a command, two arguments and a payload (protocol.txt).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub command: u32,
    pub arg0: u32,
    pub arg1: u32,
    pub data: Vec<u8>,
}

impl Message {
    pub fn new(command: u32, arg0: u32, arg1: u32, data: impl Into<Vec<u8>>) -> Message {
        Message { command, arg0, arg1, data: data.into() }
    }

    /// The 24-byte header (little-endian: command, arguments, length, the sum of the payload's bytes, the command's
    /// complement), then the payload.
    pub fn encode(&self) -> Vec<u8> {
        let check = self.data.iter().fold(0u32, |sum, b| sum.wrapping_add(u32::from(*b)));
        let mut out = Vec::with_capacity(24 + self.data.len());
        for word in [self.command, self.arg0, self.arg1, self.data.len() as u32, check, !self.command] {
            out.extend_from_slice(&word.to_le_bytes());
        }
        out.extend_from_slice(&self.data);
        out
    }

    pub async fn read(stream: &mut (impl AsyncRead + Unpin)) -> io::Result<Message> {
        let mut header = [0u8; 24];
        stream.read_exact(&mut header).await?;
        let word = |i: usize| u32::from_le_bytes([header[i * 4], header[i * 4 + 1], header[i * 4 + 2], header[i * 4 + 3]]);
        let (command, length) = (word(0), word(3) as usize);
        if word(5) != !command || length > MAX_ACCEPTED {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "not an ADB message"));
        }
        let mut data = vec![0u8; length];
        stream.read_exact(&mut data).await?;
        Ok(Message { command, arg0: word(1), arg1: word(2), data })
    }
}

// ── This computer's key ───────────────────────────────────────────────────────

const E: u32 = 65_537;
/// The size of the key adb makes, the one Android accepts.
const BITS: u64 = 2048;
/// SHA-1's DigestInfo: adbd checks the token's signature as RSA_sign(NID_sha1) of the 20-byte token.
const SHA1_INFO: [u8; 15] = [0x30, 0x21, 0x30, 0x09, 0x06, 0x05, 0x2b, 0x0e, 0x03, 0x02, 0x1a, 0x05, 0x00, 0x04, 0x14];

/// An RSA key with e = 65537: its primes (what is kept) and what is computed from them.
pub struct Key {
    p: BigUint,
    q: BigUint,
    n: BigUint,
    d: BigUint,
}

fn random_bytes(len: usize) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0u8; len];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes).map_err(|_| "no randomness".to_string())?;
    Ok(bytes)
}

/// The odd primes below 2000, to set most candidates aside before Miller–Rabin.
fn small_primes() -> Vec<u32> {
    let mut sieve = vec![true; 2000];
    let mut primes = Vec::new();
    for i in 2..sieve.len() {
        if sieve[i] {
            if i > 2 {
                primes.push(i as u32);
            }
            for j in (i * i..sieve.len()).step_by(i) {
                sieve[j] = false;
            }
        }
    }
    primes
}

/// `n` mod `m`, for a small `m`.
fn remainder(digits: &[u32], m: u32) -> u32 {
    digits.iter().rev().fold(0u64, |r, d| ((r << 32) | u64::from(*d)) % u64::from(m)) as u32
}

/// Miller–Rabin with `rounds` random bases: false is certain, true is wrong with a chance below 4^-rounds.
fn probably_prime(n: &BigUint, rounds: usize) -> Result<bool, String> {
    let one = BigUint::from(1u32);
    let two = BigUint::from(2u32);
    let n1 = n - &one;
    let s = n1.trailing_zeros().unwrap_or(0);
    let d = &n1 >> s;
    let span = n - 3u32;
    'rounds: for _ in 0..rounds {
        let a = BigUint::from_bytes_be(&random_bytes(n.bits() as usize / 8 + 8)?) % &span + &two;
        let mut x = a.modpow(&d, n);
        if x == one || x == n1 {
            continue;
        }
        for _ in 1..s {
            x = &x * &x % n;
            if x == n1 {
                continue 'rounds;
            }
        }
        return Ok(false);
    }
    Ok(true)
}

/// A random prime of exactly `bits` bits, its two top bits set (two of them make a modulus of twice the bits), and
/// p − 1 prime to e.
fn random_prime(bits: u64) -> Result<BigUint, String> {
    let small = small_primes();
    loop {
        let mut bytes = random_bytes(bits.div_ceil(8) as usize)?;
        bytes[0] |= 0xc0;
        if let Some(last) = bytes.last_mut() {
            *last |= 1;
        }
        let mut candidate = BigUint::from_bytes_be(&bytes);
        // The odd numbers from there, a few thousand at most, then another start.
        for _ in 0..4096 {
            if candidate.bits() != bits {
                break;
            }
            let digits = candidate.to_u32_digits();
            if small.iter().all(|&p| remainder(&digits, p) != 0) && remainder(&digits, E) != 1 && probably_prime(&candidate, 40)? {
                return Ok(candidate);
            }
            candidate += 2u32;
        }
    }
}

fn hex(n: &BigUint) -> String {
    n.to_str_radix(16)
}

impl Key {
    /// A new key, as adb makes one: RSA-2048, e = 65537. Takes a moment: run it off the async threads.
    pub fn generate() -> Result<Key, String> {
        Key::generate_bits(BITS)
    }

    fn generate_bits(bits: u64) -> Result<Key, String> {
        loop {
            let (p, q) = (random_prime(bits / 2)?, random_prime(bits / 2)?);
            if let Some(key) = Key::from_primes(p, q, bits) {
                return Ok(key);
            }
        }
    }

    fn from_primes(p: BigUint, q: BigUint, bits: u64) -> Option<Key> {
        let one = BigUint::from(1u32);
        if p == q || p <= one || q <= one {
            return None;
        }
        let n = &p * &q;
        if n.bits() != bits {
            return None;
        }
        let phi = (&p - &one) * (&q - &one);
        let d = BigUint::from(E).modinv(&phi)?;
        Some(Key { p, q, n, d })
    }

    /// The modulus' size in bytes.
    fn size(&self) -> usize {
        self.n.bits().div_ceil(8) as usize
    }

    /// The token's signature as adbd checks it: PKCS#1 v1.5 over SHA-1's DigestInfo, the token standing for the
    /// digest. None for a token too long to sign (Android's is 20 bytes).
    pub fn sign(&self, token: &[u8]) -> Option<Vec<u8>> {
        let size = self.size();
        let tail = SHA1_INFO.len() + token.len();
        if tail + 11 > size {
            return None;
        }
        let mut block = vec![0xffu8; size];
        block[0] = 0;
        block[1] = 1;
        block[size - tail - 1] = 0;
        block[size - tail..size - token.len()].copy_from_slice(&SHA1_INFO);
        block[size - token.len()..].copy_from_slice(token);
        let signature = BigUint::from_bytes_be(&block).modpow(&self.d, &self.n).to_bytes_be();
        let mut out = vec![0u8; size - signature.len()];
        out.extend_from_slice(&signature);
        Some(out)
    }

    /// The public key as Android keeps it (RSAPublicKey, android_pubkey.cpp), little-endian: the modulus' size in
    /// 32-bit words, −1/n mod 2³², the modulus, R² mod n (R = 2^bits), the exponent.
    pub fn android_public(&self) -> Vec<u8> {
        let words = self.size() / 4;
        let n0 = self.n.iter_u32_digits().next().unwrap_or(1);
        // The inverse of the odd n0 mod 2³²: right to 3 bits from the start, Newton's step doubles that.
        let mut inverse = n0;
        for _ in 0..5 {
            inverse = inverse.wrapping_mul(2u32.wrapping_sub(n0.wrapping_mul(inverse)));
        }
        let rr = (BigUint::from(1u32) << (2 * self.n.bits())) % &self.n;
        let mut out = Vec::with_capacity(12 + 8 * words);
        out.extend_from_slice(&(words as u32).to_le_bytes());
        out.extend_from_slice(&inverse.wrapping_neg().to_le_bytes());
        for number in [&self.n, &rr] {
            let digits = number.to_u32_digits();
            for i in 0..words {
                out.extend_from_slice(&digits.get(i).copied().unwrap_or(0).to_le_bytes());
            }
        }
        out.extend_from_slice(&E.to_le_bytes());
        out
    }

    /// What is sent for the device to ask its user: the public key in base64 and a name, as adb sends it.
    pub fn public_line(&self) -> Vec<u8> {
        let mut line = crate::claude::base64_for(&self.android_public()).into_bytes();
        line.extend_from_slice(b" coucou@coucou\0");
        line
    }

    fn to_secret(&self) -> String {
        json!({ "p": hex(&self.p), "q": hex(&self.q) }).to_string()
    }

    fn from_secret(text: &str) -> Option<Key> {
        let v: Value = serde_json::from_str(text).ok()?;
        let prime = |k: &str| BigUint::parse_bytes(v.get(k)?.as_str()?.as_bytes(), 16);
        Key::from_primes(prime("p")?, prime("q")?, BITS)
    }
}

/// One reading or making of the key at a time: two pairings at once must not each make a key, the device allowing
/// one and the keystore keeping the other.
static KEY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// This computer's key: the one in the keystore, or (`make`) a new one, kept there first. Blocking.
pub fn load_key(make: bool) -> Result<Option<Key>, String> {
    let _one = KEY_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(key) = secrets::get_internal(config::ANDROID_KEY).as_deref().and_then(Key::from_secret) {
        return Ok(Some(key));
    }
    if !make {
        return Ok(None);
    }
    let key = Key::generate()?;
    secrets::set_internal(config::ANDROID_KEY, &key.to_secret())?;
    crate::log::line("home: this computer's ADB key made, kept in the keystore");
    Ok(Some(key))
}

// ── A connection ──────────────────────────────────────────────────────────────

/// A device that let this computer in, ready for commands.
pub struct Adb {
    stream: TcpStream,
    next_id: u32,
    /// What it said it is: "device::ro.product.model=HY300-T;…".
    pub banner: String,
    /// It asked for a key before letting this computer in (false: it needs none).
    pub signed_in: bool,
}

fn lost(e: impl ToString) -> TvError {
    TvError::Unreachable(e.to_string())
}

fn wireless_only() -> TvError {
    TvError::Other(t("This device only offers Android's wireless debugging, with a pairing code, which Coucou does not speak. Turn on network debugging on port 5555 in its Developer options instead, then add it again."))
}

/// Connects to the device at `host`:`port` and goes through its authentication with `key`: the token signed, then,
/// when the device does not know the key and `ask` allows it, the key offered (the device asks its user, for up to
/// ALLOW_WAIT). Err(Auth): the device wants a key it does not have, or its user said no.
pub async fn connect(host: &str, port: u16, key: Option<&Key>, ask: bool) -> Result<Adb, TvError> {
    if !net::resolves_locally(host, port).await {
        return Err(TvError::Other(t("Only devices on your local network can be used.")));
    }
    let stream = timeout(CONNECT_WAIT, TcpStream::connect((host, port))).await.map_err(|_| lost("no answer"))?.map_err(lost)?;
    let _ = stream.set_nodelay(true);
    let mut adb = Adb { stream, next_id: 1, banner: String::new(), signed_in: false };
    adb.send(Message::new(CNXN, VERSION, MAX_PAYLOAD, &b"host::\0"[..])).await?;
    let (mut signed, mut offered) = (false, false);
    loop {
        let wait = if offered { ALLOW_WAIT } else { ANSWER_WAIT };
        let message = match timeout(wait, Message::read(&mut adb.stream)).await {
            Ok(Ok(m)) => m,
            // The device closes the connection when its user says no, or lets the question go.
            Ok(Err(_)) | Err(_) if offered => return Err(TvError::Auth),
            Ok(Err(e)) => return Err(lost(e)),
            Err(_) => return Err(lost("no answer")),
        };
        match message.command {
            CNXN => {
                adb.banner = String::from_utf8_lossy(&message.data).trim_end_matches('\0').to_string();
                adb.signed_in = signed;
                return Ok(adb);
            }
            AUTH if message.arg0 == TOKEN => match key {
                Some(key) if !signed => {
                    signed = true;
                    let signature = key.sign(&message.data).ok_or(TvError::Auth)?;
                    adb.send(Message::new(AUTH, SIGNATURE, 0, signature)).await?;
                }
                Some(key) if ask && !offered => {
                    offered = true;
                    adb.send(Message::new(AUTH, PUBLIC_KEY, 0, key.public_line())).await?;
                }
                _ => return Err(TvError::Auth),
            },
            STLS => return Err(wireless_only()),
            _ => {}
        }
    }
}

impl Adb {
    async fn send(&mut self, message: Message) -> Result<(), TvError> {
        timeout(ANSWER_WAIT, self.stream.write_all(&message.encode())).await.map_err(|_| lost("no answer"))?.map_err(lost)
    }

    /// Runs `command` in the device's shell and returns what it printed. Only the fixed commands of this file are
    /// ever given (with a package or a class checked by `valid_package` / `valid_class`).
    pub async fn shell(&mut self, command: &str) -> Result<String, TvError> {
        let local = self.next_id;
        self.next_id += 1;
        self.send(Message::new(OPEN, local, 0, format!("shell:{command}\0"))).await?;
        let mut remote = None;
        let mut out = Vec::new();
        loop {
            let message = match timeout(COMMAND_WAIT, Message::read(&mut self.stream)).await {
                Ok(Ok(m)) => m,
                Ok(Err(e)) => return Err(lost(e)),
                Err(_) => return Err(TvError::Other(t("The device took too long to answer."))),
            };
            if message.arg1 != local {
                continue;
            }
            match message.command {
                OKAY => {
                    remote.get_or_insert(message.arg0);
                }
                WRTE => {
                    let from = *remote.get_or_insert(message.arg0);
                    let room = MAX_OUTPUT.saturating_sub(out.len());
                    out.extend_from_slice(&message.data[..message.data.len().min(room)]);
                    self.send(Message::new(OKAY, local, from, Vec::new())).await?;
                }
                CLSE => {
                    // Closed before it was ever open: the device refused the service.
                    let Some(from) = remote else { return Err(TvError::NotSupported) };
                    let _ = self.send(Message::new(CLSE, local, from, Vec::new())).await;
                    return Ok(String::from_utf8_lossy(&out).into_owned());
                }
                _ => {}
            }
        }
    }

    pub async fn close(mut self) {
        let _ = self.stream.shutdown().await;
    }
}

/// What the device at `host` does when Coucou knocks on its port 5555.
pub enum Knock {
    /// It let this computer in (it needs no key, or it allowed this one before).
    Open(Adb),
    /// It answers, and wants this computer allowed on its screen first: Pair.
    Locked,
    /// It answers in a way Coucou cannot use (its words).
    Refused(String),
}

/// Knocks without offering the key (the device asks nothing). None: nothing answers ADB there.
pub async fn knock(host: &str) -> Option<Knock> {
    match open(host, false).await {
        Ok(adb) => Some(Knock::Open(adb)),
        Err(TvError::Auth) => Some(Knock::Locked),
        Err(TvError::Unreachable(_)) => None,
        Err(e) => Some(Knock::Refused(e.message())),
    }
}

/// Opens the device at `host` with this computer's key (made first when `ask`, then offered if the device does not
/// know it: the device asks its user).
pub async fn open(host: &str, ask: bool) -> Result<Adb, TvError> {
    let key = tokio::task::spawn_blocking(move || load_key(ask)).await.map_err(|e| TvError::Other(e.to_string()))?.map_err(TvError::Other)?;
    connect(host, PORT, key.as_ref(), ask).await
}

// ── The commands ──────────────────────────────────────────────────────────────

/// Android's key codes (KeyEvent).
pub const KEY_POWER: u16 = 26;
pub const KEY_VOLUME_UP: u16 = 24;
pub const KEY_VOLUME_DOWN: u16 = 25;
pub const KEY_MUTE: u16 = 164;
/// Sleep and wake-up exist from Android 4.4W (API 20); before, the power key toggles.
pub const KEY_SLEEP: u16 = 223;
pub const KEY_WAKEUP: u16 = 224;

pub fn wake_key(sdk: u8) -> u16 {
    if sdk >= 20 { KEY_WAKEUP } else { KEY_POWER }
}

pub fn sleep_key(sdk: u8) -> u16 {
    if sdk >= 20 { KEY_SLEEP } else { KEY_POWER }
}

/// Presses these keys, in order.
pub fn keys_command(keys: &[u16]) -> String {
    let codes: Vec<String> = keys.iter().map(u16::to_string).collect();
    format!("input keyevent {}", codes.join(" "))
}

/// What the device is (its model, Android's version, API level, the name the user gave it), one line each.
pub const ABOUT_COMMAND: &str = "getprop ro.product.model; getprop ro.build.version.release; getprop ro.build.version.sdk; settings get global device_name";

/// Whether it is awake, its media volume and the app on screen, in one go: filtered on the device from Android 6
/// (toybox's grep), whole before.
pub fn state_command(sdk: u8) -> &'static str {
    if sdk >= 23 {
        "echo @@power; dumpsys power | grep -E 'mWakefulness=|Display Power: state='; echo @@audio; dumpsys audio | grep -E -A 12 '^- STREAM_MUSIC:'; echo @@top; dumpsys activity activities | grep -E 'mResumedActivity|topResumedActivity'"
    } else {
        "echo @@power; dumpsys power; echo @@audio; dumpsys audio; echo @@top; dumpsys activity activities"
    }
}

/// The apps the device's launcher shows: through the package manager's query from Android 7, else its third-party
/// packages (opened by their package).
pub fn apps_command(sdk: u8) -> &'static str {
    if sdk >= 24 {
        "cmd package query-activities --brief -a android.intent.action.MAIN -c android.intent.category.LEANBACK_LAUNCHER; cmd package query-activities --brief -a android.intent.action.MAIN -c android.intent.category.LAUNCHER"
    } else {
        "pm list packages -3"
    }
}

/// A Java package name: letters, digits and underscores, in at least two parts.
pub fn valid_package(package: &str) -> bool {
    let parts: Vec<&str> = package.split('.').collect();
    package.len() <= 200
        && parts.len() >= 2
        && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
        && parts[0].starts_with(|c: char| c.is_ascii_alphabetic())
}

/// An activity's class as a component names it: ".Main", "com.x.Main", "com.x.Outer$Inner".
pub fn valid_class(class: &str) -> bool {
    !class.is_empty() && class.len() <= 300 && class.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '$'))
}

/// An app's stored id, "package/activity" or "package", in its two parts.
fn component(id: &str) -> (&str, Option<&str>) {
    match id.split_once('/') {
        Some((package, class)) => (package, Some(class)),
        None => (id, None),
    }
}

/// The command that opens the app `id`: its activity when known (in quotes: a class may hold `$`), else what its
/// package offers the launcher. None for anything that is not a package name.
pub fn launch_command(id: &str) -> Option<String> {
    let (package, class) = component(id);
    if !valid_package(package) {
        return None;
    }
    match class {
        Some(class) if valid_class(class) => Some(format!("am start -n '{package}/{class}'")),
        Some(_) => None,
        None => Some(format!("monkey -p {package} -c android.intent.category.LEANBACK_LAUNCHER -c android.intent.category.LAUNCHER 1")),
    }
}

/// True when the device says an app did not open.
pub fn launch_failed(out: &str) -> bool {
    let out = out.to_ascii_lowercase();
    out.contains("error") || out.contains("no activities found") || out.contains("aborted") || out.contains("exception")
}

// ── What the device says ──────────────────────────────────────────────────────

/// One value of the banner the device sends when it lets this computer in.
pub fn banner_value(banner: &str, key: &str) -> Option<String> {
    let props = banner.split_once("::")?.1;
    props.split(';').find_map(|kv| kv.strip_prefix(key)?.strip_prefix('=')).map(str::to_string).filter(|v| !v.is_empty())
}

/// What a device that let Coucou in says it is.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct About {
    pub model: String,
    pub release: String,
    pub sdk: u8,
    /// The name the user gave it on the device, "" when none.
    pub name: String,
}

pub fn parse_about(out: &str) -> About {
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    let line = |i: usize| lines.get(i).copied().unwrap_or("").to_string();
    let name = line(3);
    About {
        model: line(0),
        release: line(1),
        sdk: line(2).parse().unwrap_or(0),
        name: if name == "null" { String::new() } else { name.chars().take(60).collect() },
    }
}

/// What the device reports now. None: it does not say.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Reading {
    pub awake: Option<bool>,
    /// The media volume's step and its highest.
    pub volume: Option<(u32, u32)>,
    pub muted: Option<bool>,
    /// The package of the app on screen.
    pub package: Option<String>,
}

impl Reading {
    pub fn percent(&self) -> Option<u8> {
        self.volume.filter(|(_, max)| *max > 0).map(|(v, max)| ((v.min(max) * 100 + max / 2) / max) as u8)
    }
}

/// The lines of `out` after the marker `@@name`, up to the next marker.
fn section(out: &str, name: &str) -> String {
    let marker = format!("@@{name}");
    out.lines().skip_while(|l| l.trim() != marker).skip(1).take_while(|l| !l.trim().starts_with("@@")).collect::<Vec<_>>().join("\n")
}

fn number_after(text: &str, label: &str) -> Option<u32> {
    let at = text.find(label)? + label.len();
    let digits: String = text[at..].trim_start().chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The media stream's block of `dumpsys audio`: muted, highest step, and the step of the device it plays on.
fn parse_music(audio: &str) -> (Option<(u32, u32)>, Option<bool>) {
    let Some(start) = audio.find("- STREAM_MUSIC:") else { return (None, None) };
    let block: Vec<&str> = audio[start..].lines().skip(1).take_while(|l| !l.trim_start().starts_with("- ")).collect();
    let block = block.join("\n");
    let muted = if let Some(at) = block.find("Muted:") {
        Some(block[at + 6..].trim_start().starts_with("true"))
    } else {
        number_after(&block, "Mute count:").map(|n| n > 0)
    };
    let max = number_after(&block, "Max:");
    let current = number_after(&block, "streamVolume:").or_else(|| {
        // "Current: 2 (speaker): 8, 400 (hdmi): 15" with "Devices: speaker": the step of the device in use.
        let devices = block.lines().find_map(|l| l.trim().strip_prefix("Devices:"))?.trim().split([' ', ',']).next()?.to_string();
        let current = block.lines().find_map(|l| l.trim().strip_prefix("Current:"))?;
        number_after(current, &format!("({devices}):"))
    });
    (current.zip(max), muted)
}

pub fn parse_state(out: &str) -> Reading {
    let power = section(out, "power");
    let awake = match power.find("mWakefulness=") {
        Some(at) => {
            let word: String = power[at + 13..].chars().take_while(char::is_ascii_alphabetic).collect();
            match word.as_str() {
                "Awake" | "Dreaming" => Some(true),
                "Asleep" | "Dozing" => Some(false),
                _ => None,
            }
        }
        None => power.find("Display Power: state=").map(|at| power[at + 21..].starts_with("ON")),
    };
    let (volume, muted) = parse_music(&section(out, "audio"));
    let top = section(out, "top");
    let package = Regex::new(r"ActivityRecord\{[^}]*?\s([A-Za-z][A-Za-z0-9_]*(?:\.[A-Za-z0-9_]+)+)/")
        .ok()
        .and_then(|re| re.captures(&top).map(|c| c[1].to_string()));
    Reading { awake, volume, muted, package }
}

/// Apps whose names are known by their package; any other app is named after its package.
const KNOWN_APPS: &[(&str, &str)] = &[
    ("com.google.android.youtube.tv", "YouTube"),
    ("com.google.android.youtube", "YouTube"),
    ("com.google.android.youtube.tvkids", "YouTube Kids"),
    ("com.google.android.youtube.tvmusic", "YouTube Music"),
    ("com.google.android.apps.youtube.music", "YouTube Music"),
    ("com.teamsmart.videomanager.tv", "SmartTube"),
    ("com.liskovsoft.smarttubetv.beta", "SmartTube"),
    ("com.netflix.ninja", "Netflix"),
    ("com.netflix.mediaclient", "Netflix"),
    ("com.amazon.amazonvideo.livingroom", "Prime Video"),
    ("com.amazon.avod.thirdpartyclient", "Prime Video"),
    ("com.disney.disneyplus", "Disney+"),
    ("com.globo.globotv", "Globoplay"),
    ("com.wbd.stream", "Max"),
    ("com.hbo.hbonow", "Max"),
    ("com.spotify.tv.android", "Spotify"),
    ("com.spotify.music", "Spotify"),
    ("tv.twitch.android.app", "Twitch"),
    ("tv.pluto.android", "Pluto TV"),
    ("com.crunchyroll.crunchyroid", "Crunchyroll"),
    ("com.plexapp.android", "Plex"),
    ("org.xbmc.kodi", "Kodi"),
    ("org.videolan.vlc", "VLC"),
    ("com.mxtech.videoplayer.ad", "MX Player"),
    ("com.android.chrome", "Chrome"),
    ("com.google.android.videos", "Google TV"),
    ("deezer.android.tv", "Deezer"),
    ("deezer.android.app", "Deezer"),
];

/// Parts of a package that say nothing of the app.
const PLAIN: &[&str] = &[
    "com", "org", "net", "br", "tv", "android", "androidtv", "atv", "app", "apps", "mobile", "client", "ui", "leanback",
    "launcher", "main", "activity", "free", "google",
];

/// An app's name: the known one, else its package's first telling part ("com.waxrain.airplaydmr" → "Waxrain").
pub fn label_of(package: &str) -> String {
    if let Some((_, name)) = KNOWN_APPS.iter().find(|(p, _)| *p == package) {
        return name.to_string();
    }
    let word = package.split('.').find(|w| w.len() > 1 && !PLAIN.contains(&w.to_ascii_lowercase().as_str())).unwrap_or(package);
    let mut chars = word.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// The apps in what `apps_command` printed: "package/activity" lines (the launcher's), or "package:…" ones (third-party
/// packages), each package once, named; two apps of one name told apart by their package.
pub fn parse_apps(out: &str) -> Vec<App> {
    let mut found: Vec<(String, Option<String>)> = Vec::new();
    for line in out.lines().map(str::trim) {
        let (package, class) = match line.strip_prefix("package:") {
            Some(package) => (package.trim(), None),
            None => match line.split_once('/') {
                Some((package, class)) if !line.contains(' ') && valid_class(class) => (package, Some(class)),
                _ => continue,
            },
        };
        if valid_package(package) && !found.iter().any(|(p, _)| p == package) {
            found.push((package.to_string(), class.map(str::to_string)));
        }
    }
    let mut apps: Vec<App> = Vec::new();
    for (package, class) in found {
        let mut name = label_of(&package);
        if apps.iter().any(|a| a.name == name) {
            name = format!("{name} ({package})");
        }
        let id = match class {
            Some(class) => format!("{package}/{class}"),
            None => package,
        };
        apps.push(App { id, name, kind: 0 });
    }
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

/// The name, in `apps`, of the app whose package is on screen.
pub fn app_on_screen(apps: &[App], package: &str) -> Option<String> {
    apps.iter().find(|a| component(&a.id).0 == package).map(|a| a.name.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }

    /// A small key, quick to make, for the tests of the protocol: the code is the same at any size.
    fn test_key() -> Key {
        Key::generate_bits(512).unwrap()
    }

    /// What Android does with the key it was given: the modulus read back from the RSAPublicKey blob, and the
    /// signature checked as RSA_verify(NID_sha1) does.
    fn android_checks(blob: &[u8], token: &[u8], signature: &[u8]) -> bool {
        let word = |i: usize| u32::from_le_bytes(blob[i * 4..i * 4 + 4].try_into().unwrap());
        let words = word(0) as usize;
        let n = BigUint::new((0..words).map(|i| word(2 + i)).collect());
        let e = word(2 + 2 * words);
        let block = BigUint::from_bytes_be(signature).modpow(&BigUint::from(e), &n).to_bytes_be();
        // The leading 0 is dropped by the big number: 01 FF… 00 DigestInfo token.
        let size = words * 4;
        block.len() == size - 1
            && block[0] == 1
            && block.ends_with(token)
            && block[block.len() - token.len() - SHA1_INFO.len()..block.len() - token.len()] == SHA1_INFO
            && block[1..size - 1 - token.len() - SHA1_INFO.len() - 1].iter().all(|b| *b == 0xff)
    }

    #[test]
    fn a_message_is_framed_as_adb_frames_it_and_a_stranger_is_refused() {
        let m = Message::new(CNXN, VERSION, MAX_PAYLOAD, &b"host::\0"[..]);
        let bytes = m.encode();
        assert_eq!(&bytes[..4], b"CNXN");
        assert_eq!(u32::from_le_bytes(bytes[12..16].try_into().unwrap()), 7);
        assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), b"host::\0".iter().map(|b| u32::from(*b)).sum::<u32>());
        assert_eq!(u32::from_le_bytes(bytes[20..24].try_into().unwrap()), !CNXN);
        assert_eq!(block_on(Message::read(&mut &bytes[..])).unwrap(), m);
        let mut bad = bytes.clone();
        bad[20] ^= 1;
        assert!(block_on(Message::read(&mut &bad[..])).is_err(), "the magic must be the command's complement");
    }

    #[test]
    fn the_key_signs_the_token_as_android_checks_it_and_is_kept_as_its_primes() {
        let key = test_key();
        let blob = key.android_public();
        assert_eq!(blob.len(), 12 + 8 * (key.size() / 4));
        let token = [7u8; 20];
        let signature = key.sign(&token).unwrap();
        assert_eq!(signature.len(), key.size());
        assert!(android_checks(&blob, &token, &signature));
        assert!(!android_checks(&blob, &[8u8; 20], &signature), "another token does not pass");
        // n0inv and R² as Android computes them for its Montgomery maths.
        let word = |i: usize| u32::from_le_bytes(blob[i * 4..i * 4 + 4].try_into().unwrap());
        let n0 = key.n.iter_u32_digits().next().unwrap();
        assert_eq!(n0.wrapping_mul(word(1)), u32::MAX, "n0 × n0inv ≡ −1 mod 2³²");
        let words = key.size() / 4;
        let rr = BigUint::new((0..words).map(|i| word(2 + words + i)).collect());
        assert_eq!(rr, (BigUint::from(1u32) << (2 * key.n.bits())) % &key.n);
        let line = String::from_utf8(key.public_line()).unwrap();
        assert!(line.ends_with(" coucou@coucou\0") && !line.contains('\n'));
        // The keystore holds the primes; the rest is computed again.
        let secret = key.to_secret();
        let back = Key::from_primes(BigUint::parse_bytes(serde_json::from_str::<Value>(&secret).unwrap()["p"].as_str().unwrap().as_bytes(), 16).unwrap(), key.q.clone(), key.n.bits()).unwrap();
        assert_eq!(back.d, key.d);
        assert!(Key::from_secret(&secret).is_none(), "only a 2048-bit key is taken from the keystore");
        assert!(key.sign(&[0u8; 200]).is_none(), "a token too long for the key is not signed");
    }

    #[test]
    fn a_real_key_is_rsa_2048_with_the_exponent_android_wants() {
        let key = Key::generate().unwrap();
        assert_eq!(key.n.bits(), 2048);
        let blob = key.android_public();
        assert_eq!(blob.len(), 524, "RSAPublicKey: 4 + 4 + 256 + 256 + 4");
        assert_eq!(u32::from_le_bytes(blob[0..4].try_into().unwrap()), 64);
        assert_eq!(u32::from_le_bytes(blob[520..524].try_into().unwrap()), 65_537);
        assert!(android_checks(&blob, &[3u8; 20], &key.sign(&[3u8; 20]).unwrap()));
        assert!(Key::from_secret(&key.to_secret()).is_some_and(|back| back.n == key.n));
        assert!(probably_prime(&BigUint::from(65_537u32), 20).unwrap() && !probably_prime(&BigUint::from(65_535u32), 20).unwrap());
    }

    /// How the fake device answers.
    #[derive(Clone, Copy, PartialEq)]
    enum Device {
        /// No authentication (ro.adb.secure=0).
        Open,
        /// Wants a key; allows the one offered (its user says yes).
        Allows,
        /// Wants a key; its user says no to the one offered.
        Denies,
        /// Wireless debugging: TLS first.
        Tls,
    }

    /// A device on 127.0.0.1 that speaks ADB as `kind` does and answers `shell:` with `output`, in two writes. It
    /// returns what it was sent, for the test to check.
    async fn fake_device(kind: Device, output: &'static str) -> (u16, tokio::task::JoinHandle<Vec<Message>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut seen = Vec::new();
            let mut allowed: Option<Vec<u8>> = None;
            let token = [9u8; 20];
            let send = |m: Message| m.encode();
            loop {
                let Ok(m) = Message::read(&mut s).await else { break };
                seen.push(m.clone());
                let reply = match (m.command, m.arg0) {
                    (CNXN, _) => match kind {
                        Device::Open => vec![send(Message::new(CNXN, VERSION, MAX_PAYLOAD, &b"device::ro.product.model=HY300-T;features=cmd\0"[..]))],
                        Device::Tls => vec![send(Message::new(STLS, 0x0100_0000, 0, Vec::new()))],
                        _ => vec![send(Message::new(AUTH, TOKEN, 0, token.to_vec()))],
                    },
                    (AUTH, SIGNATURE) => match &allowed {
                        Some(blob) if android_checks(blob, &token, &m.data) => vec![send(Message::new(CNXN, VERSION, MAX_PAYLOAD, &b"device::ro.product.model=HY300-T;\0"[..]))],
                        _ => vec![send(Message::new(AUTH, TOKEN, 0, token.to_vec()))],
                    },
                    (AUTH, PUBLIC_KEY) => {
                        if kind == Device::Denies {
                            break;
                        }
                        let text = String::from_utf8(m.data.clone()).unwrap();
                        let b64 = text.split(' ').next().unwrap();
                        allowed = Some(decode_base64(b64));
                        vec![send(Message::new(CNXN, VERSION, MAX_PAYLOAD, &b"device::ro.product.model=HY300-T;\0"[..]))]
                    }
                    (OPEN, local) => {
                        let half = output.len() / 2;
                        vec![
                            send(Message::new(OKAY, 42, local, Vec::new())),
                            send(Message::new(WRTE, 42, local, &output.as_bytes()[..half])),
                            send(Message::new(WRTE, 42, local, &output.as_bytes()[half..])),
                            send(Message::new(CLSE, 42, local, Vec::new())),
                        ]
                    }
                    _ => Vec::new(),
                };
                for bytes in reply {
                    s.write_all(&bytes).await.unwrap();
                }
            }
            seen
        });
        (port, handle)
    }

    fn decode_base64(text: &str) -> Vec<u8> {
        const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let values: Vec<u32> = text.bytes().filter(|b| *b != b'=').map(|b| TABLE.iter().position(|t| *t == b).unwrap() as u32).collect();
        let mut out = Vec::new();
        for chunk in values.chunks(4) {
            let n = chunk.iter().enumerate().fold(0u32, |n, (i, v)| n | (v << (18 - 6 * i)));
            out.extend_from_slice(&n.to_be_bytes()[1..chunk.len()]);
        }
        out
    }

    #[test]
    fn an_open_device_runs_a_command_and_its_output_comes_back_whole() {
        block_on(async {
            let (port, device) = fake_device(Device::Open, "mWakefulness=Awake\n").await;
            let mut adb = connect("127.0.0.1", port, None, false).await.unwrap();
            assert!(!adb.signed_in, "it asked for no key");
            assert_eq!(banner_value(&adb.banner, "ro.product.model").as_deref(), Some("HY300-T"));
            assert_eq!(adb.shell("echo @@power; dumpsys power").await.unwrap(), "mWakefulness=Awake\n");
            adb.close().await;
            let seen = device.await.unwrap();
            assert_eq!(seen[1].command, OPEN);
            assert_eq!(seen[1].data, b"shell:echo @@power; dumpsys power\0");
            // Each write acknowledged, then the stream closed from this side too.
            assert_eq!(seen.iter().filter(|m| m.command == OKAY && m.arg1 == 42).count(), 2);
            assert!(seen.iter().any(|m| m.command == CLSE && m.arg1 == 42));
        });
    }

    #[test]
    fn a_locked_device_asks_its_user_only_when_pairing() {
        block_on(async {
            let key = test_key();
            // Knocking: the token is signed, the device does not know the key, and nothing is offered.
            let (port, device) = fake_device(Device::Allows, "").await;
            assert!(matches!(connect("127.0.0.1", port, Some(&key), false).await, Err(TvError::Auth)));
            let seen = device.await.unwrap();
            assert!(seen.iter().any(|m| m.command == AUTH && m.arg0 == SIGNATURE));
            assert!(!seen.iter().any(|m| m.command == AUTH && m.arg0 == PUBLIC_KEY), "no question on its screen");
            // Pairing: the key is offered, its user allows it.
            let (port, device) = fake_device(Device::Allows, "").await;
            let adb = connect("127.0.0.1", port, Some(&key), true).await.unwrap();
            assert!(adb.signed_in);
            adb.close().await;
            assert!(device.await.unwrap().iter().any(|m| m.command == AUTH && m.arg0 == PUBLIC_KEY));
            // Its user says no: the device hangs up.
            let (port, _device) = fake_device(Device::Denies, "").await;
            assert!(matches!(connect("127.0.0.1", port, Some(&key), true).await, Err(TvError::Auth)));
            // Without a key, a locked device is only Locked.
            let (port, _device) = fake_device(Device::Allows, "").await;
            assert!(matches!(connect("127.0.0.1", port, None, true).await, Err(TvError::Auth)));
            // Wireless debugging (TLS) is said to be unsupported.
            let (port, _device) = fake_device(Device::Tls, "").await;
            assert!(matches!(connect("127.0.0.1", port, Some(&key), false).await, Err(TvError::Other(_))));
        });
    }

    #[test]
    fn a_device_that_allowed_the_key_lets_it_in_with_its_signature_alone() {
        block_on(async {
            let key = test_key();
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let blob = key.android_public();
            let device = tokio::spawn(async move {
                let (mut s, _) = listener.accept().await.unwrap();
                let token = [5u8; 20];
                let mut offered = false;
                while let Ok(m) = Message::read(&mut s).await {
                    let reply = match (m.command, m.arg0) {
                        (CNXN, _) => Message::new(AUTH, TOKEN, 0, token.to_vec()),
                        (AUTH, SIGNATURE) if android_checks(&blob, &token, &m.data) => Message::new(CNXN, VERSION, MAX_PAYLOAD, &b"device::\0"[..]),
                        (AUTH, PUBLIC_KEY) => {
                            offered = true;
                            break;
                        }
                        _ => Message::new(AUTH, TOKEN, 0, token.to_vec()),
                    };
                    s.write_all(&reply.encode()).await.unwrap();
                }
                offered
            });
            let adb = connect("127.0.0.1", port, Some(&key), true).await.unwrap();
            adb.close().await;
            assert!(!device.await.unwrap(), "a key already allowed is never offered again");
        });
    }

    #[test]
    fn only_local_devices_are_knocked_on() {
        block_on(async {
            assert!(matches!(connect("8.8.8.8", PORT, None, false).await, Err(TvError::Other(_))));
        });
    }

    #[test]
    fn the_commands_are_fixed_and_a_package_or_class_is_checked_before_it_is_used() {
        assert_eq!(keys_command(&[KEY_VOLUME_UP, KEY_VOLUME_UP]), "input keyevent 24 24");
        assert_eq!((wake_key(33), sleep_key(33), wake_key(19), sleep_key(19)), (224, 223, 26, 26));
        assert_eq!(launch_command("com.netflix.ninja/.MainActivity").as_deref(), Some("am start -n 'com.netflix.ninja/.MainActivity'"));
        assert_eq!(launch_command("com.x.y/com.x.y.Outer$Inner").as_deref(), Some("am start -n 'com.x.y/com.x.y.Outer$Inner'"));
        assert_eq!(
            launch_command("org.xbmc.kodi").as_deref(),
            Some("monkey -p org.xbmc.kodi -c android.intent.category.LEANBACK_LAUNCHER -c android.intent.category.LAUNCHER 1")
        );
        for bad in ["com.x;reboot", "com.x/.Main';reboot;'", "com.x/.Main x", "x", "1x.y", "com.x/", "com..x", "com.x$(id)"] {
            assert_eq!(launch_command(bad), None, "{bad:?}");
        }
        assert!(launch_failed("Error: Activity class {com.x/.Main} does not exist.") && launch_failed("** No activities found to run, monkey aborted."));
        assert!(!launch_failed("Starting: Intent { cmp=com.netflix.ninja/.MainActivity }") && !launch_failed("Events injected: 1"));
    }

    #[test]
    fn android_13_says_awake_its_volume_and_the_app_on_screen() {
        let out = "@@power\n  mWakefulness=Awake\n  Display Power: state=ON\n@@audio\n- STREAM_MUSIC:\n   Muted: false\n   Muted Internally: false\n   Min: 0\n   Max: 15\n   streamVolume:8\n   Current: 2 (speaker): 8, 40000000 (default): 9\n   Devices: speaker\n- STREAM_ALARM:\n   Muted: true\n@@top\n  topResumedActivity=ActivityRecord{2f1a u0 com.google.android.youtube.tv/com.google.android.apps.youtube.tv.activity.ShellActivity t12}\n  mResumedActivity: ActivityRecord{2f1a u0 com.google.android.youtube.tv/com.google.android.apps.youtube.tv.activity.ShellActivity t12}\n";
        let r = parse_state(out);
        assert_eq!(r, Reading { awake: Some(true), volume: Some((8, 15)), muted: Some(false), package: Some("com.google.android.youtube.tv".into()) });
        assert_eq!(r.percent(), Some(53));
        let asleep = parse_state("@@power\nmWakefulness=Asleep\n@@audio\n@@top\n");
        assert_eq!((asleep.awake, asleep.volume, asleep.package), (Some(false), None, None));
    }

    #[test]
    fn older_androids_say_it_their_own_way() {
        // Android 7–9: no streamVolume line; the step of the device in use, from Current.
        let nougat = "@@power\nmWakefulness=Dreaming\n@@audio\nStream volumes (device: index)\n- STREAM_MUSIC:\n   Muted: true\n   Min: 0\n   Max: 15\n   Current: 2 (speaker): 4, 400 (hdmi): 15, 40000000 (default): 9\n   Devices: hdmi\n- STREAM_RING:\n@@top\n  mResumedActivity: ActivityRecord{1b2 u0 com.netflix.ninja/.MainActivity t3}\n";
        let r = parse_state(nougat);
        assert_eq!((r.awake, r.volume, r.muted, r.package.as_deref()), (Some(true), Some((15, 15)), Some(true), Some("com.netflix.ninja")));
        // Android 4.4: the mute count, the display's power.
        let kitkat = "@@power\n  Display Power: state=OFF\n@@audio\n- STREAM_MUSIC:\n   Mute count: 1\n   Max: 15\n   Current: 2 (speaker): 6, 400 (hdmi): 15\n   Devices: speaker\n@@top\n";
        let r = parse_state(kitkat);
        assert_eq!((r.awake, r.volume, r.muted), (Some(false), Some((6, 15)), Some(true)));
        assert_eq!(parse_state(""), Reading::default(), "nothing said, nothing assumed");
    }

    #[test]
    fn the_apps_are_the_launchers_named_once_each() {
        let out = "2 activities found:\n  Activity #0:\n    priority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=false\n    com.netflix.ninja/.MainActivity\n  Activity #1:\n    priority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=false\n    com.google.android.youtube.tv/com.google.android.apps.youtube.tv.activity.ShellActivity\n1 activities found:\n  Activity #0:\n    priority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=false\n    com.netflix.ninja/.SplashActivity\n    com.waxrain.airplaydmr/.WaxPlayer\n";
        let apps = parse_apps(out);
        let named: Vec<(&str, &str)> = apps.iter().map(|a| (a.name.as_str(), a.id.as_str())).collect();
        assert_eq!(
            named,
            [
                ("Netflix", "com.netflix.ninja/.MainActivity"),
                ("Waxrain", "com.waxrain.airplaydmr/.WaxPlayer"),
                ("YouTube", "com.google.android.youtube.tv/com.google.android.apps.youtube.tv.activity.ShellActivity"),
            ]
        );
        assert_eq!(app_on_screen(&apps, "com.netflix.ninja").as_deref(), Some("Netflix"));
        // Before Android 7: third-party packages, opened by their package.
        let old = parse_apps("package:com.netflix.mediaclient\npackage:org.xbmc.kodi\npackage:bad name\n");
        assert_eq!(old.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["org.xbmc.kodi", "com.netflix.mediaclient"]);
        assert_eq!(label_of("com.example.tv.app"), "Example");
        let twins = parse_apps("package:com.acme.player\npackage:com.acme.radio\n");
        assert_eq!(twins.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(), ["Acme", "Acme (com.acme.radio)"]);
    }

    #[test]
    fn what_the_device_says_it_is() {
        let about = parse_about("HY300-T\n13\n33\nATV_99\n");
        assert_eq!(about, About { model: "HY300-T".into(), release: "13".into(), sdk: 33, name: "ATV_99".into() });
        assert_eq!(parse_about("X\n4.4.2\n19\nnull\n").name, "", "no name given");
        assert_eq!(banner_value("device::ro.product.name=hy;ro.product.model=HY300-T;features=cmd", "ro.product.model").as_deref(), Some("HY300-T"));
        assert_eq!(banner_value("device::", "ro.product.model"), None);
    }
}
