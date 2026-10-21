//! Words and numbers as Brazilian files and people write them: "R$ 1.234,56", "9.20", "12,5%", headers with accents
//! and odd spacing, files in Latin-1, and tickers ("GGRC11", "TIMS3F").

/// Lower case, accents off, anything but letters and digits as one underscore: "Preço Médio" → "preco_medio",
/// "CNPJ_Fundo_Classe" → "cnpj_fundo_classe". For matching headers and words.
pub fn norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut gap = false;
    for c in s.chars() {
        let c = fold(c);
        if c.is_ascii_alphanumeric() {
            if gap && !out.is_empty() {
                out.push('_');
            }
            gap = false;
            out.push(c.to_ascii_lowercase());
        } else {
            gap = true;
        }
    }
    out
}

/// One letter without its accent (the Portuguese and Spanish ones, and the usual others).
pub fn fold(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'å' => 'a',
        'Á' | 'À' | 'Â' | 'Ã' | 'Ä' | 'Å' => 'A',
        'é' | 'è' | 'ê' | 'ë' => 'e',
        'É' | 'È' | 'Ê' | 'Ë' => 'E',
        'í' | 'ì' | 'î' | 'ï' => 'i',
        'Í' | 'Ì' | 'Î' | 'Ï' => 'I',
        'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
        'Ó' | 'Ò' | 'Ô' | 'Õ' | 'Ö' => 'O',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
        'ç' => 'c',
        'Ç' => 'C',
        'ñ' => 'n',
        'Ñ' => 'N',
        other => other,
    }
}

/// A sentence without accents, in lower case, its words separated by one space: for looking for words in it.
pub fn plain(s: &str) -> String {
    norm(s).replace('_', " ")
}

/// A number as written in Brazil or in a data file: "1.234,56", "R$ 9,20", "9.20", "-12,5%", "1 234,5", "(3,20)"
/// (negative). A comma makes the comma the decimal mark (and dots thousands); with no comma the dot is the decimal
/// mark. None for "", "-", "—" and words.
pub fn num(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let negative_paren = t.starts_with('(') && t.ends_with(')');
    let mut digits = String::new();
    let mut seen_digit = false;
    for c in t.chars() {
        match c {
            '0'..='9' => {
                seen_digit = true;
                digits.push(c);
            }
            ',' | '.' => digits.push(c),
            '-' | '−' if !seen_digit => digits.push('-'),
            'e' | 'E' if seen_digit && !digits.contains('e') => digits.push('e'),
            '+' if digits.ends_with('e') => digits.push('+'),
            '-' if digits.ends_with('e') => digits.push('-'),
            ' ' | '\u{a0}' | 'R' | '$' | '%' | '(' | ')' | '+' => {}
            _ => return None,
        }
    }
    if !seen_digit {
        return None;
    }
    let cleaned = if digits.contains(',') {
        digits.replace('.', "").replace(',', ".")
    } else if digits.matches('.').count() > 1 {
        // "1.234.567": thousands only.
        digits.replace('.', "")
    } else {
        digits
    };
    let value: f64 = cleaned.parse().ok()?;
    Some(if negative_paren { -value.abs() } else { value })
}

/// Bytes of a file, as text: UTF-8 when they are, else Latin-1 (the CVM's and B3's files), a byte a letter.
pub fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

/// "GGRC11", "TIMS3", "SPXB11", "BBAS3F" (the odd-lot market: read as BBAS3), "TAEE11". Upper case, no spaces.
pub fn ticker(s: &str) -> Option<String> {
    let t: String = s.trim().chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase();
    let t = t.strip_suffix(".SA").unwrap_or(&t).to_string();
    let bytes = t.as_bytes();
    if bytes.len() < 5 || bytes.len() > 7 || !bytes[..4].iter().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    let rest = &t[4..];
    let rest = rest.strip_suffix('F').filter(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit())).unwrap_or(rest);
    let (num, letter) = match rest.find(|c: char| !c.is_ascii_digit()) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if num.is_empty() || num.len() > 2 || letter.len() > 1 || !letter.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    Some(format!("{}{}{}", &t[..4], num, letter))
}

/// The four letters of a ticker: "GGRC11" → "GGRC". The ISIN of its shares carries them ("BRGGRCCTF006").
pub fn ticker_root(t: &str) -> &str {
    t.get(..4).unwrap_or(t)
}

/// Every ticker in a text, once each, in order.
pub fn tickers_in(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for word in text.split(|c: char| !c.is_ascii_alphanumeric()) {
        if let Some(t) = ticker(word) {
            if word.chars().take(4).all(|c| c.is_ascii_uppercase()) && !out.contains(&t) {
                out.push(t);
            }
        }
    }
    out
}

/// Reais as a Brazilian reads them: "R$ 1.234,56", "-R$ 3,20".
pub fn brl(v: f64) -> String {
    let sign = if v < 0.0 { "-" } else { "" };
    format!("{sign}R$ {}", grouped(v.abs(), 2))
}

/// A number with Brazilian marks and `decimals` places: "1.234,5".
pub fn grouped(v: f64, decimals: usize) -> String {
    let s = format!("{:.*}", decimals, v.abs());
    let (int, frac) = match s.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (s.clone(), None),
    };
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    if let Some(f) = frac {
        out.push(',');
        out.push_str(&f);
    }
    if v < 0.0 {
        format!("-{out}")
    } else {
        out
    }
}

/// "12,3%".
pub fn pct(v: f64, decimals: usize) -> String {
    format!("{}%", grouped(v, decimals))
}

/// A short text cut at `max` characters, on a word, with an ellipsis.
pub fn clip(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max).collect();
    let cut = match cut.rfind(' ') {
        Some(i) if i > max / 2 => cut[..i].to_string(),
        _ => cut,
    };
    format!("{}…", cut.trim_end_matches([',', ';', '.', ' ']))
}

/// A CNPJ as its 14 characters, without dots, slash and dash: digits, and since July 2026 letters too in the first
/// twelve (the alphanumeric CNPJ).
pub fn cnpj_chars(s: &str) -> String {
    s.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_uppercase()).collect()
}

/// A CNPJ the user or the brain typed, checked by its two check digits (a character counts its ASCII code minus 48,
/// as the Receita defines it for the alphanumeric CNPJ; for digits, the old rule).
pub fn cnpj(s: &str) -> Result<String, String> {
    let c = cnpj_chars(s);
    let wrong = || format!("\"{}\" is not a valid CNPJ (14 characters, like 00.000.000/0000-00, with their check digits).", s.trim());
    if c.len() != 14 || !c[12..].bytes().all(|b| b.is_ascii_digit()) || c.bytes().all(|b| b == c.as_bytes()[0]) {
        return Err(wrong());
    }
    let check = |part: &[u8]| -> u8 {
        let sum: u32 = part.iter().rev().enumerate().map(|(i, b)| (*b as u32 - 48) * (i as u32 % 8 + 2)).sum();
        let r = sum % 11;
        if r < 2 { b'0' } else { b'0' + (11 - r) as u8 }
    };
    let b = c.as_bytes();
    if check(&b[..12]) == b[12] && check(&b[..13]) == b[13] {
        Ok(c)
    } else {
        Err(wrong())
    }
}

/// "09552812000114" as "09.552.812/0001-14".
pub fn cnpj_format(c: &str) -> String {
    if c.len() != 14 || !c.is_ascii() {
        return c.to_string();
    }
    format!("{}.{}.{}/{}-{}", &c[..2], &c[2..5], &c[5..8], &c[8..12], &c[12..])
}

/// `data` in base64 (standard alphabet, padded): what some sites take in their addresses.
pub fn base64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
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
    fn headers_and_words_match_without_accents() {
        assert_eq!(norm("Preço Médio"), "preco_medio");
        assert_eq!(norm("  CNPJ_Fundo_Classe "), "cnpj_fundo_classe");
        assert_eq!(norm("Código ISIN / Distribuição"), "codigo_isin_distribuicao");
        assert_eq!(norm("Entrada/Saída"), "entrada_saida");
        assert_eq!(plain("Saindo da ACADEMIA!"), "saindo da academia");
    }

    #[test]
    fn numbers_the_brazilian_way_and_the_data_way() {
        assert_eq!(num("R$ 1.234,56"), Some(1234.56));
        assert_eq!(num("9,20"), Some(9.20));
        assert_eq!(num("9.20"), Some(9.20));
        assert_eq!(num("-12,5%"), Some(-12.5));
        assert_eq!(num("1.234.567"), Some(1234567.0));
        assert_eq!(num("150"), Some(150.0));
        assert_eq!(num("(3,20)"), Some(-3.2));
        assert_eq!(num("1 234,5"), Some(1234.5));
        assert_eq!(num("1.5E-3"), Some(0.0015));
        assert_eq!(num("-"), None);
        assert_eq!(num(""), None);
        assert_eq!(num("Compra"), None);
        assert_eq!(brl(1234.5), "R$ 1.234,50");
        assert_eq!(brl(-3.2), "-R$ 3,20");
        assert_eq!(grouped(6489.6, 2), "6.489,60");
        assert_eq!(grouped(1000000.0, 0), "1.000.000");
        assert_eq!(pct(79.8, 1), "79,8%");
    }

    #[test]
    fn latin1_files_read_as_text() {
        assert_eq!(decode(&[0x50, 0x52, 0x45, 0xC7, 0x4F]), "PREÇO");
        assert_eq!(decode("Preço".as_bytes()), "Preço");
        assert_eq!(decode(&[0xEF, 0xBB, 0xBF, b'a']), "a");
    }

    #[test]
    fn base64_as_the_sites_read_it() {
        assert_eq!(base64(b"{\"typeFund\":7}"), "eyJ0eXBlRnVuZCI6N30=");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b""), "");
    }

    #[test]
    fn a_cnpj_is_checked_by_its_digits() {
        assert_eq!(cnpj("26.614.291/0001-00").as_deref(), Ok("26614291000100"));
        assert_eq!(cnpj(" 26614291000100 ").as_deref(), Ok("26614291000100"));
        assert!(cnpj("26.614.291/0001-01").is_err(), "a check digit off");
        assert!(cnpj("2661429100010").is_err(), "13 digits");
        assert!(cnpj("00.000.000/0000-00").is_err(), "all the same");
        // The Receita's own example of an alphanumeric CNPJ (from July 2026).
        assert_eq!(cnpj("12.abc.345/01de-35").as_deref(), Ok("12ABC34501DE35"));
        assert!(cnpj("12.ABC.345/01DE-36").is_err());
        assert_eq!(cnpj_format("26614291000100"), "26.614.291/0001-00");
        assert_eq!(cnpj_chars("12.ABC.345/01DE-35"), "12ABC34501DE35");
    }

    #[test]
    fn tickers_are_recognised() {
        assert_eq!(ticker("ggrc11").as_deref(), Some("GGRC11"));
        assert_eq!(ticker("TIMS3").as_deref(), Some("TIMS3"));
        assert_eq!(ticker("BBAS3F").as_deref(), Some("BBAS3"));
        assert_eq!(ticker("PETR4.SA").as_deref(), Some("PETR4"));
        assert_eq!(ticker("KNCR11B").as_deref(), Some("KNCR11B"));
        assert_eq!(ticker("COMPRA"), None);
        assert_eq!(ticker("R$"), None);
        assert_eq!(ticker("2026"), None);
        assert_eq!(ticker_root("GGRC11"), "GGRC");
        assert_eq!(tickers_in("comprar GARE11 e TIMS3, depois GARE11 de novo; SELIC não"), vec!["GARE11", "TIMS3"]);
        assert_eq!(clip("um texto bem comprido para cortar", 15), "um texto bem…");
    }
}
