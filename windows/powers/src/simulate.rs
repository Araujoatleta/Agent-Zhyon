//! The portfolio's futures: tens of thousands of them, each a month-by-month path of prices and dividends with the
//! user's monthly contribution and every provento bought back in, run on every core of the phone at once. What comes
//! out is not a forecast but a spread: the patrimony and the monthly income at each year in a bad future (one in ten
//! is worse), the middle one and a good one (one in ten is better), the chance the income reaches the user's goal by
//! each year, and three plain shocks on today's portfolio.
//!
//! Everything is in today's money (inflation taken out), under assumptions stated with the answer and no more
//! precise than they are: funds' prices keep their purchasing power on average and swing 14 % a year, shares gain
//! 4 % a year above inflation and swing 26 %, fixed income pays the CDI above the IPCA; dividends a share keep their
//! purchasing power and swing 8 % a year; assets of a class move partly together. What each asset pays today is
//! what it paid in the last twelve months, from the official sources. A contribution is split as the portfolio is.

use serde_json::{json, Value};

use crate::date::Date;
use crate::model::{AssetClass, Market, Store};
use crate::text::{brl, pct};

pub const DEFAULT_PATHS: usize = 20_000;
pub const MOST_PATHS: usize = 200_000;
pub const MOST_YEARS: usize = 40;

/// xoshiro256**: fast, good enough for a simulation, and the same on every device for a seed.
#[derive(Clone)]
struct Rng([u64; 4]);

impl Rng {
    fn new(seed: u64) -> Rng {
        // splitmix64 fills the state.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut next = || {
            z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            x ^ (x >> 31)
        };
        Rng([next(), next(), next(), next()])
    }

    fn next(&mut self) -> u64 {
        let s = &mut self.0;
        let result = s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    /// In (0, 1).
    fn unit(&mut self) -> f64 {
        ((self.next() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }

    /// Standard normal (Box-Muller).
    fn normal(&mut self) -> f64 {
        let (u, v) = (self.unit(), self.unit());
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * v).cos()
    }
}

/// One asset as the simulation sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub ticker: String,
    pub class: AssetClass,
    pub value: f64,
    /// What it pays a month today, R$.
    pub income: f64,
    /// Real drift and volatility of its price, a year.
    pub drift: f64,
    pub vol: f64,
}

/// What a class is assumed to do, above inflation: (drift a year, volatility a year).
fn assumed(class: AssetClass, market: &Market) -> (f64, f64) {
    let real_cdi = match (market.economy.cdi, market.economy.ipca_12m) {
        (Some(cdi), Some(ipca)) => ((cdi - ipca) / 100.0).clamp(0.0, 0.10),
        _ => 0.05,
    };
    match class {
        AssetClass::Fii => (0.0, 0.14),
        AssetClass::Acao | AssetClass::Etf | AssetClass::Bdr => (0.04, 0.26),
        AssetClass::RendaFixa | AssetClass::Tesouro => (real_cdi, 0.01),
        AssetClass::Cripto => (0.0, 0.70),
        AssetClass::Outro => (0.0, 0.10),
    }
}

/// What a share of `ticker` paid a month over the last twelve, R$ (net of the tax on JCP).
pub fn monthly_dividend(market: &Market, ticker: &str, today: Date) -> f64 {
    let since = today.add_months(-12);
    if let Some(fund) = market.funds.get(ticker) {
        let mut pays: Vec<_> = fund.payouts.iter().filter(|p| !p.amortization && p.of_share(ticker) && p.record <= today && p.record > since).collect();
        pays.sort_by_key(|p| p.record);
        pays.dedup_by_key(|p| p.record);
        if pays.is_empty() {
            return 0.0;
        }
        let months = (pays[0].record.until(today) as f64 / 30.44).ceil().clamp(1.0, 12.0);
        return pays.iter().map(|p| p.value).sum::<f64>() / months.max(pays.len() as f64);
    }
    if let Some(c) = market.company_payouts.get(ticker) {
        let year: f64 = c.payouts.iter().filter(|p| p.record <= today && p.record > since).map(|p| if p.tax_free == Some(false) { p.value * 0.85 } else { p.value }).sum();
        return year / 12.0;
    }
    0.0
}

/// The portfolio as the simulation takes it: what is held and priced, with what each pays a month.
pub fn assets(store: &Store, market: &Market, today: Date) -> Vec<Asset> {
    let v = crate::portfolio::value(&store.portfolio, market);
    v.holdings
        .iter()
        .filter(|h| h.value > 0.0)
        .map(|h| {
            let (drift, vol) = assumed(h.class, market);
            let income = if h.class.quoted() { monthly_dividend(market, &h.ticker, today) * h.quantity } else { 0.0 };
            Asset { ticker: h.ticker.clone(), class: h.class, value: h.value, income, drift, vol }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    /// R$ a month, in today's money.
    pub monthly: f64,
    pub years: usize,
    /// The monthly income wanted, R$ of today.
    pub goal: Option<f64>,
    pub paths: usize,
    pub seed: u64,
}

/// One future: the value and the monthly income at the end of each year.
fn path(assets: &[Asset], plan: &Plan, rng: &mut Rng, out: &mut [(f64, f64)]) {
    let n = assets.len();
    let mut value: Vec<f64> = assets.iter().map(|a| a.value).collect();
    let mut income: Vec<f64> = assets.iter().map(|a| a.income).collect();
    let total: f64 = value.iter().sum();
    let weights: Vec<f64> = value.iter().map(|v| if total > 0.0 { v / total } else { 1.0 / n as f64 }).collect();
    // A new real bought with an asset brings that asset's yield on its value of the moment.
    let dividend_vol = 0.08f64 / 12f64.sqrt();
    for year in 0..plan.years {
        for _ in 0..12 {
            // What moves a whole class together, and each asset by itself.
            let (shared_funds, shared_shares) = (rng.normal(), rng.normal());
            let mut paid = 0.0;
            for i in 0..n {
                let a = &assets[i];
                let shared = match a.class {
                    AssetClass::Fii => shared_funds,
                    AssetClass::Acao | AssetClass::Etf | AssetClass::Bdr => shared_shares,
                    _ => 0.0,
                };
                let z = 0.6 * shared + 0.8 * rng.normal();
                let monthly_vol = a.vol / 12f64.sqrt();
                let growth = ((a.drift / 12.0) - 0.5 * monthly_vol * monthly_vol + monthly_vol * z).exp();
                value[i] *= growth;
                income[i] *= (-0.5 * dividend_vol * dividend_vol + dividend_vol * rng.normal()).exp();
                paid += income[i];
            }
            // The proventos and the contribution buy more of everything, as the portfolio is split today.
            let cash = paid + plan.monthly;
            for i in 0..n {
                let bought = cash * weights[i];
                if value[i] > 0.0 {
                    income[i] *= 1.0 + bought / value[i];
                }
                value[i] += bought;
            }
        }
        out[year] = (value.iter().sum(), income.iter().sum());
    }
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let at = (q * (sorted.len() - 1) as f64).round() as usize;
    sorted[at.min(sorted.len() - 1)]
}

/// The futures of `assets` under `plan`, on every core: per year, the value and the income in a bad, the middle and
/// a good future, and the chance of the goal.
pub fn run(assets: &[Asset], plan: &Plan) -> Value {
    let started = std::time::Instant::now();
    let years = plan.years.clamp(1, MOST_YEARS);
    let paths = plan.paths.clamp(100, MOST_PATHS);
    let plan = Plan { years, paths, ..*plan };
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 8);
    let share = paths.div_ceil(cores);
    // Each core's futures, year by year.
    let mut chunks: Vec<Vec<(f64, f64)>> = Vec::new();
    if !assets.is_empty() {
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..cores)
                .map(|core| {
                    let plan = &plan;
                    scope.spawn(move || {
                        let count = share.min(paths.saturating_sub(core * share));
                        let mut all = vec![(0.0, 0.0); count * years];
                        for p in 0..count {
                            // Each future has its own seed: the same futures whatever the number of cores.
                            let mut rng = Rng::new(plan.seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (core * share + p) as u64);
                            path(assets, plan, &mut rng, &mut all[p * years..(p + 1) * years]);
                        }
                        all
                    })
                })
                .collect();
            for h in handles {
                chunks.push(h.join().unwrap_or_default());
            }
        });
    }
    let today_value: f64 = assets.iter().map(|a| a.value).sum();
    let today_income: f64 = assets.iter().map(|a| a.income).sum();
    let mut yearly = Vec::new();
    let mut reached: Option<usize> = None;
    for y in 0..years {
        let mut values: Vec<f64> = Vec::with_capacity(paths);
        let mut incomes: Vec<f64> = Vec::with_capacity(paths);
        for c in &chunks {
            for p in 0..c.len() / years {
                values.push(c[p * years + y].0);
                incomes.push(c[p * years + y].1);
            }
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        incomes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let chance = plan.goal.filter(|g| *g > 0.0).map(|g| incomes.iter().filter(|i| **i >= g).count() as f64 / incomes.len().max(1) as f64 * 100.0);
        if reached.is_none() && plan.goal.is_some_and(|g| g > 0.0 && quantile(&incomes, 0.5) >= g) {
            reached = Some(y + 1);
        }
        yearly.push(json!({
            "year": y + 1,
            "value": { "low": quantile(&values, 0.1), "mid": quantile(&values, 0.5), "high": quantile(&values, 0.9) },
            "income": { "low": quantile(&incomes, 0.1), "mid": quantile(&incomes, 0.5), "high": quantile(&incomes, 0.9) },
            "chance": chance,
        }));
    }
    // Three plain shocks on today's portfolio.
    let biggest = assets.iter().max_by(|a, b| a.income.partial_cmp(&b.income).unwrap_or(std::cmp::Ordering::Equal));
    let mut shocks = vec![
        json!({ "id": "prices", "value": today_value * 0.7, "income": today_income }),
        json!({ "id": "dividends", "value": today_value, "income": today_income * 0.8 }),
    ];
    if let Some(b) = biggest.filter(|b| b.income > 0.0 && assets.len() > 1) {
        shocks.push(json!({ "id": "payer", "ticker": b.ticker, "value": today_value, "income": today_income - b.income }));
    }
    json!({
        "paths": paths, "years": years, "monthly": plan.monthly, "goal": plan.goal, "cores": cores,
        "tookMs": started.elapsed().as_millis() as u64, "steps": paths as u64 * years as u64 * 12 * assets.len() as u64,
        "today": { "value": today_value, "income": today_income },
        "yearly": yearly, "goalYear": reached, "shocks": shocks,
        "assets": assets.iter().map(|a| json!({ "ticker": a.ticker, "value": a.value, "income": a.income, "drift": a.drift * 100.0, "vol": a.vol * 100.0 })).collect::<Vec<_>>(),
    })
}

/// The portfolio's futures, as the app and the brain ask for them.
pub fn simulate(store: &Store, market: &Market, monthly: f64, years: usize, goal: Option<f64>, paths: usize, today: Date) -> Value {
    let plan = Plan { monthly: monthly.max(0.0), years, goal, paths: if paths == 0 { DEFAULT_PATHS } else { paths }, seed: today.y as u64 * 10_000 + today.m as u64 * 100 + today.d as u64 };
    run(&assets(store, market, today), &plan)
}

pub fn for_model(v: &Value) -> String {
    let yearly = v["yearly"].as_array().cloned().unwrap_or_default();
    if yearly.is_empty() || v["today"]["value"].as_f64().unwrap_or(0.0) <= 0.0 {
        return "There is no priced position to simulate yet.".into();
    }
    let f = |x: &Value| brl(x.as_f64().unwrap_or(0.0));
    let mut out = format!(
        "{} futures of the portfolio over {} years, {} a month contributed, every provento reinvested, in today's money. Today: {} worth, {} a month in income.\n",
        v["paths"], v["years"], f(&v["monthly"]), f(&v["today"]["value"]), f(&v["today"]["income"]),
    );
    for y in yearly.iter().filter(|y| {
        let n = y["year"].as_u64().unwrap_or(0);
        n == 1 || n % 5 == 0 || n == v["years"].as_u64().unwrap_or(0)
    }) {
        out.push_str(&format!(
            "Year {}: worth {} (bad future {}, good {}); income a month {} (bad {}, good {}){}\n",
            y["year"],
            f(&y["value"]["mid"]),
            f(&y["value"]["low"]),
            f(&y["value"]["high"]),
            f(&y["income"]["mid"]),
            f(&y["income"]["low"]),
            f(&y["income"]["high"]),
            y["chance"].as_f64().map(|c| format!("; chance of the goal {}", pct(c, 0))).unwrap_or_default(),
        ));
    }
    if let Some(goal) = v["goal"].as_f64() {
        out.push_str(&match v["goalYear"].as_u64() {
            Some(y) => format!("In the middle future the income reaches {} a month in year {y}.\n", brl(goal)),
            None => format!("In the middle future the income does not reach {} a month within {} years.\n", brl(goal), v["years"]),
        });
    }
    out.push_str("Assumptions, not forecasts: funds' prices keep their purchasing power (14 % a year of swing), shares gain 4 % a year above inflation (26 % of swing), dividends keep their purchasing power (8 % of swing); a bad future is one in ten, a good one too.");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(ticker: &str, class: AssetClass, value: f64, income: f64) -> Asset {
        let (drift, vol) = assumed(class, &Market::default());
        Asset { ticker: ticker.into(), class, value, income, drift, vol }
    }

    #[test]
    fn the_same_seed_gives_the_same_futures_and_the_spread_is_ordered() {
        let assets = vec![asset("GGRC11", AssetClass::Fii, 10_000.0, 90.0), asset("ITSA4", AssetClass::Acao, 10_000.0, 60.0)];
        let plan = Plan { monthly: 1_000.0, years: 10, goal: Some(1_000.0), paths: 4_000, seed: 7 };
        let a = run(&assets, &plan);
        let b = run(&assets, &plan);
        assert_eq!(a["yearly"], b["yearly"], "a seed is a result, whatever the number of cores");
        let last = &a["yearly"][9];
        let (low, mid, high) = (last["value"]["low"].as_f64().unwrap(), last["value"]["mid"].as_f64().unwrap(), last["value"]["high"].as_f64().unwrap());
        assert!(low < mid && mid < high, "{last}");
        // 20,000 today and 120,000 contributed, yielding about 9 % a year that comes back in: by hand, near 250,000.
        assert!(mid > 200_000.0 && mid < 320_000.0, "{mid}");
        let income = last["income"]["mid"].as_f64().unwrap();
        assert!(income > 1_200.0 && income < 3_000.0, "{income}");
        let chances: Vec<f64> = a["yearly"].as_array().unwrap().iter().map(|y| y["chance"].as_f64().unwrap()).collect();
        assert!(chances[0] < 1.0 && chances[9] > 30.0 && chances.windows(2).all(|w| w[1] >= w[0] - 1.5), "{chances:?}");
        assert!(a["goalYear"].as_u64().is_some_and(|y| (5..=10).contains(&y)), "{}", a["goalYear"]);
        assert_eq!(a["steps"], 4_000u64 * 10 * 12 * 2);
    }

    #[test]
    fn with_nothing_put_in_the_income_keeps_its_purchasing_power_in_the_middle() {
        let assets = vec![asset("GGRC11", AssetClass::Fii, 100_000.0, 0.0)];
        let v = run(&assets, &Plan { monthly: 0.0, years: 5, goal: None, paths: 6_000, seed: 3 });
        let mid = v["yearly"][4]["value"]["mid"].as_f64().unwrap();
        // No drift above inflation and no income: the middle future is a little under today (the swing costs).
        assert!(mid > 80_000.0 && mid < 101_000.0, "{mid}");
        assert!(v["yearly"][4]["chance"].is_null() && v["goalYear"].is_null());
        let shocks = v["shocks"].as_array().unwrap();
        assert_eq!(shocks[0]["value"], 70_000.0);
        assert_eq!(shocks.len(), 2, "one asset alone has no 'biggest payer stops'");
    }

    /// How fast it is: `cargo test -p coucou-powers --lib --release speed -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn speed() {
        let assets: Vec<Asset> = (0..8).map(|i| asset(&format!("FUND{i}"), if i % 2 == 0 { AssetClass::Fii } else { AssetClass::Acao }, 5_000.0, 40.0)).collect();
        let v = run(&assets, &Plan { monthly: 1_000.0, years: 20, goal: Some(5_000.0), paths: 100_000, seed: 1 });
        println!("{} futures, {} steps, {} cores: {} ms", v["paths"], v["steps"], v["cores"], v["tookMs"]);
    }

    #[test]
    fn an_empty_portfolio_says_so() {
        let v = run(&[], &Plan { monthly: 500.0, years: 3, goal: None, paths: 500, seed: 1 });
        assert!(for_model(&v).contains("no priced position"));
    }
}
