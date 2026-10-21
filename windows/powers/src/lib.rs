//! Zheus' superpowers, without a window: the user's portfolio and the official data it is read against (B3's daily
//! quotes, the CVM's open data on real-estate funds and listed companies, FundosNet's documents, the Banco Central's
//! rates and the Focus survey), the analyses Zheus gives (a fund's dividends and their sustainability, the events
//! that need the user, a company's statements against the user's own thesis, the portfolio's road to its targets
//! without selling, scenarios, taxes and the yearly declaration, a committee before a decision and the record of
//! decisions), and the user's day: studies for a public exam, English, training and a monthly content kit.
//!
//! Pure Rust and plain data: the app (src-tauri/src/powers) keeps the files, schedules the refreshes and offers all
//! of it to Zheus' brain as tools. Nothing here buys, sells, sends or publishes anything.

pub mod audit;
pub mod company;
pub mod content;
pub mod date;
pub mod day;
pub mod engine;
pub mod english;
pub mod fii;
pub mod gym;
pub mod import;
pub mod journal;
pub mod model;
pub mod portfolio;
pub mod refresh;
pub mod sources;
pub mod srs;
pub mod study;
pub mod table;
pub mod text;
pub mod xlsx;
pub mod xml;
pub mod zip;
