//! ZIP archives as the official sites publish them (B3's daily quotes, the CVM's open data) and as XLSX files are
//! made: the central directory read from the end of the file, then only the entries wanted, stored or deflated. The
//! directory and the entries can come from byte ranges, so a large archive on a server is never downloaded whole when
//! only one of its files is needed.

use std::io::Read;

use flate2::read::DeflateDecoder;

/// One file in an archive, as its central directory describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    /// 0 stored, 8 deflated.
    pub method: u16,
    pub compressed: u64,
    pub size: u64,
    /// Where its local header starts in the archive.
    pub offset: u64,
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], i: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(i..i + 8)?.try_into().ok()?))
}

/// How many bytes from the end of an archive hold its directory's end record (and a comment of any length).
pub const TAIL: u64 = 65_536 + 22;

/// Where the central directory is (its offset and size in the archive), read from the archive's last bytes. `tail`
/// holds the last bytes of an archive of `total` bytes.
pub fn directory_location(tail: &[u8], total: u64) -> Result<(u64, u64), String> {
    let at = tail.windows(4).rposition(|w| w == [0x50, 0x4b, 0x05, 0x06]).ok_or("not a ZIP archive (no end record)")?;
    let mut size = u32_at(tail, at + 12).ok_or("short end record")? as u64;
    let mut offset = u32_at(tail, at + 16).ok_or("short end record")? as u64;
    // ZIP64: the real values are in the ZIP64 end record, found through its locator just before.
    if (offset == 0xFFFF_FFFF || size == 0xFFFF_FFFF) && at >= 20 && u32_at(tail, at - 20) == Some(0x0706_4b50) {
        let record = u64_at(tail, at - 20 + 8).ok_or("short ZIP64 locator")?;
        let tail_start = total.saturating_sub(tail.len() as u64);
        if record >= tail_start {
            let r = (record - tail_start) as usize;
            if u32_at(tail, r) == Some(0x0606_4b50) {
                size = u64_at(tail, r + 40).ok_or("short ZIP64 record")?;
                offset = u64_at(tail, r + 48).ok_or("short ZIP64 record")?;
            }
        }
    }
    Ok((offset, size))
}

/// The entries of a central directory.
pub fn entries(directory: &[u8]) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 46 <= directory.len() {
        if u32_at(directory, i) != Some(0x0201_4b50) {
            break;
        }
        let method = u16_at(directory, i + 10).unwrap_or(0);
        let mut compressed = u32_at(directory, i + 20).unwrap_or(0) as u64;
        let mut size = u32_at(directory, i + 24).unwrap_or(0) as u64;
        let name_len = u16_at(directory, i + 28).unwrap_or(0) as usize;
        let extra_len = u16_at(directory, i + 30).unwrap_or(0) as usize;
        let comment_len = u16_at(directory, i + 32).unwrap_or(0) as usize;
        let mut offset = u32_at(directory, i + 42).unwrap_or(0) as u64;
        let name_bytes = directory.get(i + 46..i + 46 + name_len).ok_or("short directory entry")?;
        let name = crate::text::decode(name_bytes);
        // ZIP64 extra field: the 64-bit values, in order, for those that were 0xFFFFFFFF.
        let extra = directory.get(i + 46 + name_len..i + 46 + name_len + extra_len).unwrap_or(&[]);
        let mut j = 0;
        while j + 4 <= extra.len() {
            let id = u16_at(extra, j).unwrap_or(0);
            let len = u16_at(extra, j + 2).unwrap_or(0) as usize;
            if id == 0x0001 {
                let mut k = j + 4;
                if size == 0xFFFF_FFFF {
                    size = u64_at(extra, k).unwrap_or(size);
                    k += 8;
                }
                if compressed == 0xFFFF_FFFF {
                    compressed = u64_at(extra, k).unwrap_or(compressed);
                    k += 8;
                }
                if offset == 0xFFFF_FFFF {
                    offset = u64_at(extra, k).unwrap_or(offset);
                }
            }
            j += 4 + len;
        }
        out.push(Entry { name, method, compressed, size, offset });
        i += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

/// How many bytes to read at an entry's offset to be sure to hold its local header and its data.
pub fn span(entry: &Entry) -> u64 {
    30 + 1024 + entry.name.len() as u64 + entry.compressed
}

/// An entry's bytes, from `local` (the archive's bytes starting at the entry's local header).
pub fn extract(entry: &Entry, local: &[u8]) -> Result<Vec<u8>, String> {
    if u32_at(local, 0) != Some(0x0403_4b50) {
        return Err(format!("{}: no local header where the directory says", entry.name));
    }
    let name_len = u16_at(local, 26).unwrap_or(0) as usize;
    let extra_len = u16_at(local, 28).unwrap_or(0) as usize;
    let start = 30 + name_len + extra_len;
    let end = start + entry.compressed as usize;
    let data = local.get(start..end).ok_or_else(|| format!("{}: the archive is cut short", entry.name))?;
    match entry.method {
        0 => Ok(data.to_vec()),
        8 => {
            let mut out = Vec::with_capacity(entry.size.min(512 * 1024 * 1024) as usize);
            DeflateDecoder::new(data).read_to_end(&mut out).map_err(|e| format!("{}: {e}", entry.name))?;
            Ok(out)
        }
        m => Err(format!("{}: compression method {m} is not supported", entry.name)),
    }
}

/// A whole archive in memory.
pub struct Archive<'a> {
    bytes: &'a [u8],
    pub entries: Vec<Entry>,
}

impl<'a> Archive<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Archive<'a>, String> {
        let total = bytes.len() as u64;
        let tail_start = total.saturating_sub(TAIL) as usize;
        let (offset, size) = directory_location(&bytes[tail_start..], total)?;
        let directory = bytes.get(offset as usize..(offset + size) as usize).ok_or("the directory is past the end of the archive")?;
        Ok(Archive { bytes, entries: entries(directory)? })
    }

    /// The first entry whose name (any folder) ends with `suffix`, case-insensitive.
    pub fn find(&self, suffix: &str) -> Option<&Entry> {
        let suffix = suffix.to_ascii_lowercase();
        self.entries.iter().find(|e| e.name.to_ascii_lowercase().ends_with(&suffix))
    }

    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>, String> {
        extract(entry, self.bytes.get(entry.offset as usize..).ok_or("entry past the end")?)
    }

    pub fn read_named(&self, suffix: &str) -> Result<Vec<u8>, String> {
        let entry = self.find(suffix).ok_or_else(|| format!("no {suffix} in the archive"))?;
        self.read(entry)
    }
}

/// A small archive, for the tests and the fixtures: each file stored or deflated (`deflate`).
pub fn build(files: &[(&str, &[u8])], deflate: bool) -> Vec<u8> {
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use std::io::Write;
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in files {
        let payload = if deflate {
            let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
            enc.write_all(data).expect("in memory");
            enc.finish().expect("in memory")
        } else {
            data.to_vec()
        };
        let method: u16 = if deflate { 8 } else { 0 };
        let offset = out.len() as u32;
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&[0; 8]); // time, date, crc (not checked)
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&payload);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&[0; 8]);
        central.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 12]); // extra, comment, disk, internal and external attributes
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let dir_offset = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&dir_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_and_deflated_entries_come_back_whole() {
        let big = "CNPJ_Fundo;Valor\n".repeat(2000);
        for deflate in [false, true] {
            let zip = build(&[("a/inf_geral_2026.csv", big.as_bytes()), ("b.txt", b"ok")], deflate);
            let archive = Archive::new(&zip).unwrap();
            assert_eq!(archive.entries.len(), 2);
            assert_eq!(archive.read_named("INF_GERAL_2026.CSV").unwrap(), big.as_bytes());
            assert_eq!(archive.read_named("b.txt").unwrap(), b"ok");
            assert!(archive.read_named("nada.csv").is_err());
        }
    }

    #[test]
    fn an_entry_reads_from_its_own_byte_range() {
        let zip = build(&[("x.csv", b"1;2;3\n"), ("y.csv", &b"y".repeat(5000))], true);
        let total = zip.len() as u64;
        let tail = &zip[total.saturating_sub(TAIL) as usize..];
        let (offset, size) = directory_location(tail, total).unwrap();
        let list = entries(&zip[offset as usize..(offset + size) as usize]).unwrap();
        let y = list.iter().find(|e| e.name == "y.csv").unwrap();
        let end = (y.offset + span(y)).min(total);
        assert_eq!(extract(y, &zip[y.offset as usize..end as usize]).unwrap(), b"y".repeat(5000));
        assert!(Archive::new(b"not a zip at all").is_err());
        assert!(extract(y, b"garbage").is_err());
    }
}
