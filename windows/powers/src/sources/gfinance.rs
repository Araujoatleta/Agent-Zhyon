//! Google Finance's quote page: the price of a B3 asset during the session, with no key and no account. The page is
//! drawn by script from data blocks it carries (`AF_initDataCallback`), where each asset is a list:
//!
//! ```text
//! ["/g/11c807gj6w",["GGRC11","BVMF"],"GGR Covepi Renda FII - RL",0,"BRL",[8.69,-0.16,-1.81,2,2,2],null,8.85,
//!  null,"BR","/g/11fzc7250v",[1793125002],"America/Sao_Paulo",…]
//! ```
//!
//! its price, change and change in percent, then the previous close, then when the price was (unix seconds). Google
//! gives B3's prices some 15 minutes late. B3's daily file stays the record of each day's close; this is the price
//! during the session.

use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// "https://www.google.com/finance/quote/GGRC11:BVMF?hl=en".
pub fn url(ticker: &str) -> String {
    format!("https://www.google.com/finance/quote/{}:BVMF?hl=en", ticker.trim().to_uppercase())
}

/// A price read from Google Finance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Live {
    pub price: f64,
    /// When the price was, unix seconds.
    pub at: i64,
    /// The previous session's close, when the page gives it.
    #[serde(default)]
    pub previous_close: Option<f64>,
}

const NUMBER: &str = r"-?[0-9]+(?:\.[0-9]+)?(?:[eE][-+]?[0-9]+)?";

fn entry() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // [ticker, exchange], name, kind, currency, [price, change, percent…], null, previous close, …, [time].
        Regex::new(&format!(
            r#"\["([A-Z0-9]{{4,8}})","BVMF"\],"(?:[^"\\]|\\.)*",[0-9]+,"[A-Z]{{3}}",\[({NUMBER}),[^\]]*\],null,({NUMBER}|null),[^\[\]]*?\[([0-9]{{9,11}})\]"#
        ))
        .expect("the entry pattern")
    })
}

/// The price of `ticker` on its quote page: the newest of the page's entries for it (its blocks may differ by a few
/// seconds). None when the page has none (an unknown ticker: the page only asks for it).
pub fn parse(page: &str, ticker: &str) -> Option<Live> {
    let ticker = ticker.trim().to_uppercase();
    entry()
        .captures_iter(page)
        .filter(|c| c[1] == ticker)
        .filter_map(|c| {
            let price: f64 = c[2].parse().ok().filter(|p: &f64| p.is_finite() && *p > 0.0 && *p < 1e7)?;
            let at: i64 = c[4].parse().ok()?;
            let previous_close = c[3].parse::<f64>().ok().filter(|p| p.is_finite() && *p > 0.0);
            Some(Live { price, at, previous_close })
        })
        .max_by_key(|l| l.at)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pieces of the quote pages as Google Finance sent them on 27 October 2026 (powers-live, live_raw_gfinance).
    const GGRC11: &str = r#"AF_initDataCallback({key: 'ds:2', hash: '9', data:[[[["/g/11c807gj6w",["GGRC11","BVMF"],"GGR Covepi Renda FII - RL",0,"BRL",[8.69,-0.1600008,-1.8079185,2,2,2],null,8.85,null,"BR","/g/11fzc7250v",[1793125002],"America/Sao_Paulo",-10800,"/g/11c807gj6w",null,null,[1793125002],null,[[1,[2026,10,27,10,null,null,null,[-10800]],[2026,10,27,16,56,null,null,[-10800]]]],null,"GGRC11:BVMF",0,null,null,null,1]]]], sideChannel: {}});</script><script class="ds:8">AF_initDataCallback({key: 'ds:8', hash: '10', data:[[[[null,["GGRC11","BVMF"]],null,8.87,"/g/11c807gj6w",8.68,8.91,8.69,2,-0.1600008,2,-1.8079185,2,"BRL","GGRC11:BVMF","GGR Covepi Renda FII - RL",8.85,0,2140119,"Real estate investment trust",["/g/11c807gj6w",["GGRC11","BVMF"],"GGR Covepi Renda FII - RL",0,"BRL",[8.69,-0.1600008,-1.8079185,2,2,2],null,8.85,null,"BR","/g/11fzc7250v",[1793125002],"America/Sao_Paulo",-10800]]]]});"#;
    const TIMS3: &str = r##"data:[[[["/g/11dx9hs84p",["TIMS3","BVMF"],"Tim SA",0,"BRL",[18.78,0.6800003,3.7569077,2,2,2],null,18.1,"#063987","BR","/m/0bbzkq0",[1793124996],"America/Sao_Paulo",-10800]]]]"##;
    const BOVA11: &str = r#"data:[[[["/g/12hdlpl2f",["BOVA11","BVMF"],"Ishares Ibovespa Fundo de Indice",5,"BRL",[172.95,0.5299988,0.30738822,2,2,2],null,172.42,null,"BR","/g/11byd1bjwj",[1793125045],"America/Sao_Paulo",-10800]]]]
data:[[[[null,["BOVA11","BVMF"]],null,174.5,"/g/12hdlpl2f",171.85,175.66,172.94,2,0.5200043,2,0.30159163,2,"BRL","BOVA11:BVMF","Ishares Ibovespa Fundo de Indice",172.42,0,7886171,"Capital market",["/g/12hdlpl2f",["BOVA11","BVMF"],"Ishares Ibovespa Fundo de Indice",5,"BRL",[172.94,0.5200043,0.30159163,2,2,2],null,172.42,null,"BR","/g/11byd1bjwj",[1793125039],"America/Sao_Paulo",-10800]]]]"#;
    /// A ticker Google does not know: the page only asks for it.
    const UNKNOWN: &str = r#"'ds:2' : {id:'gCvqoe',request:[[[null,["ZZZZ11","BVMF"]]],1]},'ds:4' : {id:'gXxkFd',request:[["ZZZZ11","BVMF"]]}"#;
    /// The indexes every page shows on top: not the asset asked for.
    const INDEXES: &str = r#"[null,[["/m/0cqyw",[".DJI","INDEXDJX"],"Dow Jones Industrial Average",1,null,[52373.72,12.140625,0.023186132,2,2,3],null,52361.58,null,null,null,[1793125913],"America/New_York",-14400]]"#;

    #[test]
    fn the_price_its_time_and_the_previous_close_come_from_the_assets_own_entry() {
        assert_eq!(parse(GGRC11, "ggrc11"), Some(Live { price: 8.69, at: 1_793_125_002, previous_close: Some(8.85) }));
        assert_eq!(parse(TIMS3, "TIMS3"), Some(Live { price: 18.78, at: 1_793_124_996, previous_close: Some(18.1) }), "a colour between the close and the time");
        assert_eq!(parse(BOVA11, "BOVA11").map(|l| (l.price, l.at)), Some((172.95, 1_793_125_045)), "the newest of its entries");
        assert_eq!(url(" ggrc11 "), "https://www.google.com/finance/quote/GGRC11:BVMF?hl=en");
    }

    #[test]
    fn another_asset_or_none_is_no_price() {
        assert_eq!(parse(UNKNOWN, "ZZZZ11"), None);
        assert_eq!(parse(&format!("{INDEXES}{TIMS3}"), "GGRC11"), None, "TIMS3's entry is not GGRC11's");
        assert_eq!(parse(INDEXES, "DJI"), None);
        assert_eq!(parse(r#"["GGRC11","BVMF"],"X",0,"BRL",[0,0,0],null,8.85,null,[1793125002]"#, "GGRC11"), None, "no price is not a price of zero");
        assert_eq!(parse("", "GGRC11"), None);
    }
}
