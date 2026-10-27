//! Google Finance's quote page: the price of a B3 asset during the session, with no key and no account. The page
//! carries it in its own attributes (`data-last-price`, and when it was, `data-last-normal-market-timestamp`), and
//! shows it as its big price too. B3's daily file stays the record of each day's close; this is the price now.

use serde::{Deserialize, Serialize};

/// "https://www.google.com/finance/quote/GGRC11:BVMF?hl=en": English, so the figures read the same everywhere.
pub fn url(ticker: &str) -> String {
    format!("https://www.google.com/finance/quote/{}:BVMF?hl=en", ticker.trim().to_uppercase())
}

/// A price read from Google Finance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Live {
    pub price: f64,
    /// When the price was, unix seconds: the page's own time, else when it was read.
    pub at: i64,
    /// The previous close, when the page gives it.
    #[serde(default)]
    pub previous_close: Option<f64>,
}

/// The value of the attribute `name` in the tag that holds byte `at` of `html`.
fn attribute_in_tag(html: &str, at: usize, name: &str) -> Option<String> {
    let start = html[..at].rfind('<')?;
    let end = at + html[at..].find('>')?;
    let tag = &html[start..end];
    let key = format!(" {name}=\"");
    let from = tag.find(&key)? + key.len();
    let to = from + tag[from..].find('"')?;
    Some(tag[from..to].to_string())
}

/// "R$8.69", "R$1,234.56", "8.69" (the English page) → 8.69.
fn money(text: &str) -> Option<f64> {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    digits.parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0 && *v < 1e7)
}

/// The text of the first element after `marker` (up to its closing tag).
fn text_after(html: &str, marker: &str) -> Option<String> {
    let from = html.find(marker)? + marker.len();
    let rest = &html[from..];
    let rest = &rest[rest.find('>')? + 1..];
    Some(rest[..rest.find('<')?].trim().to_string())
}

/// The price on a quote page. The page's own asset comes first, before any related one further down; None when it
/// has no price (an unknown ticker gets Google's search instead).
pub fn parse(html: &str, read_at: i64) -> Option<Live> {
    let key = "data-last-price=\"";
    let by_attribute = html.find(key).and_then(|at| {
        let from = at + key.len();
        let value = &html[from..from + html[from..].find('"')?];
        let price = value.trim().parse::<f64>().ok().filter(|v| v.is_finite() && *v > 0.0 && *v < 1e7)?;
        let when = attribute_in_tag(html, at, "data-last-normal-market-timestamp").and_then(|t| t.trim().parse::<i64>().ok()).filter(|t| *t > 1_000_000_000);
        Some((price, when))
    });
    // The big price, as shown: what the page says when the attribute is gone.
    let shown = text_after(html, "class=\"YMlKec fxKbKc\"").and_then(|t| money(&t));
    let (price, when) = match (by_attribute, shown) {
        (Some(found), _) => found,
        (None, Some(price)) => (price, None),
        (None, None) => return None,
    };
    let previous_close = html.find(">Previous close<").and_then(|at| text_after(&html[at..], "class=\"P6K39c\"")).and_then(|t| money(&t));
    Some(Live { price, at: when.unwrap_or(read_at), previous_close })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<html><head><title>GGR Covepi Renda FII - RL (GGRC11) Price & News - Google Finance</title></head><body>
<div jscontroller="NdbN0c" jsname="AS5Pxb" data-mid="/g/11c1q2n5lk" data-entity-type="0" data-exchange-code="BVMF" data-currency-code="BRL" data-last-price="8.69" data-last-normal-market-timestamp="1793127370" data-tz-offset="-10800000">
<div class="AHmHk"><span><div jsname="ip75Cb" class="kf1m0"><div class="YMlKec fxKbKc">R$8.69</div></div></span></div></div>
<div class="gyFHrc"><span class="mfs7Fc">Previous close</span><div class="P6K39c">R$8.85</div></div>
<div class="related"><div data-last-price="12.40" data-last-normal-market-timestamp="1793127000"></div><div class="YMlKec">R$12.40</div></div>
</body></html>"#;

    #[test]
    fn the_price_and_its_time_come_from_the_pages_own_asset() {
        let live = parse(PAGE, 5).unwrap();
        assert_eq!(live, Live { price: 8.69, at: 1_793_127_370, previous_close: Some(8.85) });
        assert_eq!(url(" ggrc11 "), "https://www.google.com/finance/quote/GGRC11:BVMF?hl=en");
    }

    #[test]
    fn the_shown_price_stands_in_for_the_attribute_and_nothing_is_none() {
        let shown = r#"<div class="YMlKec fxKbKc">R$1,234.56</div>"#;
        assert_eq!(parse(shown, 77), Some(Live { price: 1234.56, at: 77, previous_close: None }));
        assert_eq!(parse("<html><title>Google Finance - Search</title></html>", 1), None);
        assert_eq!(parse(r#"<div data-last-price="0"></div>"#, 1), None, "no price is not a price of zero");
        assert_eq!(parse(r#"<div data-last-price="abc"></div>"#, 1), None);
    }
}
