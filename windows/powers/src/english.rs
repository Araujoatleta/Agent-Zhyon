//! English out of what the user already does: the companies their S&P 500 ETF holds give the day's reading (their
//! latest quarterly results, read in English with the brain), the words worth keeping become cards (spaced
//! repetition), a speaking prompt about investing, training or studying is answered aloud on the phone, and the
//! "English mode" makes Zheus answer in English and correct the user's sentences in the chat.

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::model::{Card, EnglishProfile, PracticeLog};

/// The largest companies of the S&P 500, rotated one a day: (ticker, name, what it does).
pub const COMPANIES: &[(&str, &str, &str)] = &[
    ("NVDA", "Nvidia", "designs the chips that train and run AI models"),
    ("MSFT", "Microsoft", "sells cloud computing (Azure), Office and Windows"),
    ("AAPL", "Apple", "makes the iPhone and earns more and more from services"),
    ("AMZN", "Amazon", "runs the largest online store and the largest cloud (AWS)"),
    ("GOOGL", "Alphabet", "owns Google Search, YouTube and Google Cloud"),
    ("META", "Meta Platforms", "owns Facebook, Instagram and WhatsApp, and earns from ads"),
    ("AVGO", "Broadcom", "makes networking chips and custom AI chips"),
    ("BRK.B", "Berkshire Hathaway", "Warren Buffett's group of insurers, railways and stakes"),
    ("TSLA", "Tesla", "makes electric cars and batteries"),
    ("JPM", "JPMorgan Chase", "the largest bank in the United States"),
    ("LLY", "Eli Lilly", "makes medicines, among them the weight-loss drugs"),
    ("V", "Visa", "runs a card payment network and earns a fee per payment"),
    ("WMT", "Walmart", "the largest retailer in the world"),
    ("XOM", "Exxon Mobil", "produces and refines oil and gas"),
    ("MA", "Mastercard", "runs a card payment network"),
    ("NFLX", "Netflix", "streams series and films for a monthly fee"),
    ("COST", "Costco", "membership warehouse stores"),
    ("ORCL", "Oracle", "databases and cloud infrastructure"),
    ("JNJ", "Johnson & Johnson", "medicines and medical devices"),
    ("PG", "Procter & Gamble", "household brands like Gillette and Pampers"),
    ("HD", "Home Depot", "home-improvement stores"),
    ("KO", "Coca-Cola", "drinks, sold in almost every country"),
    ("BAC", "Bank of America", "one of the largest US banks"),
    ("AMD", "AMD", "processors and graphics chips"),
    ("MCD", "McDonald's", "fast food, mostly through franchisees who pay it rent and fees"),
];

/// Speaking prompts, rotated, by theme.
pub const PROMPTS: &[&str] = &[
    "Explain to a friend why real-estate funds pay dividends every month, and what could make them cut it.",
    "Tell me about your last workout: what you trained, how heavy, and how you felt.",
    "Describe your study routine for the exam and what you would change in it.",
    "What would you do with your first salary as a public servant? Talk for one minute.",
    "Explain the difference between a fixed-income bond and a share, as if to a teenager.",
    "Talk about a decision you made recently, why you made it, and what you expect.",
    "Describe your perfect weekend, from Friday night to Sunday night.",
    "Tell me how the interest rate in Brazil affects your investments.",
    "Explain how you keep motivated when you do not feel like training or studying.",
    "Describe the company of the day's lesson and whether you would invest in it, and why.",
];

/// The day's lesson.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lesson {
    pub date: Date,
    pub level: String,
    pub minutes: u32,
    /// The English cards due today (front → back).
    pub cards: Vec<(String, String)>,
    pub company: (String, String, String),
    pub reading: String,
    pub speaking: String,
    pub writing: String,
}

pub fn level_of(p: &EnglishProfile) -> String {
    let l = p.level.trim().to_uppercase();
    if ["A1", "A2", "B1", "B2", "C1", "C2"].contains(&l.as_str()) {
        l
    } else {
        "B1".into()
    }
}

pub fn lesson(p: &EnglishProfile, cards: &[Card], today: Date) -> Lesson {
    let n = today.days().rem_euclid(1_000_000) as usize;
    let (t, name, what) = COMPANIES[n % COMPANIES.len()];
    let level = level_of(p);
    let minutes = if p.minutes_per_day > 0 { p.minutes_per_day } else { 15 };
    let card_limit = (minutes / 3).clamp(5, 20) as usize;
    let due: Vec<(String, String)> = crate::srs::due(cards, "english", today, card_limit).into_iter().map(|c| (c.front.clone(), c.back.clone())).collect();
    let words = match level.as_str() {
        "A1" | "A2" => 5,
        "B1" => 6,
        _ => 8,
    };
    Lesson {
        date: today,
        level: level.clone(),
        minutes,
        cards: due,
        company: (t.into(), name.into(), what.into()),
        reading: format!(
            "Find {name}'s ({t}) latest quarterly results (its own press release or a reliable news story). Give the user a short passage in English at level {level}, then explain its key terms ({words} words or phrases worth learning, like \"revenue\", \"guidance\", \"margin\") and save them as English cards (life_cards, deck english)."
        ),
        speaking: PROMPTS[n % PROMPTS.len()].into(),
        writing: format!("Write three sentences in English about {name}, then let Zheus correct them."),
    }
}

/// The English mode's note on each of the user's messages.
pub fn mode_note(p: &EnglishProfile) -> String {
    format!(
        "English mode is on: answer in English, at a level the user follows ({}). If their message has mistakes in English, first show the corrected sentence in one line (\"✏️ …\"), then answer. If they write in Portuguese, answer in English and give the English for one or two of their phrases.",
        level_of(p)
    )
}

/// A practice logged: the streak grows on a new day in a row, and starts over after a missed day.
pub fn practice(p: &mut EnglishProfile, logs: &mut Vec<PracticeLog>, kind: &str, minutes: u32, today: Date) {
    logs.push(PracticeLog { date: today, minutes, kind: kind.trim().to_lowercase() });
    match p.last_day {
        Some(d) if d == today => {}
        Some(d) if d.add_days(1) == today => p.streak_days += 1,
        _ => p.streak_days = 1,
    }
    p.last_day = Some(today);
}

pub fn minutes_this_week(logs: &[PracticeLog], today: Date) -> u32 {
    let monday = today.add_days(-(today.weekday() as i64));
    logs.iter().filter(|l| l.date >= monday && l.date <= today).map(|l| l.minutes).sum()
}

pub fn for_model(p: &EnglishProfile, l: &Lesson, logs: &[PracticeLog], today: Date) -> String {
    let mut out = format!(
        "English, level {}, {} minutes a day; English mode {}; streak {} day(s); {} minutes this week.",
        l.level,
        l.minutes,
        if p.mode { "on" } else { "off" },
        if p.last_day.is_some_and(|d| d.add_days(1) >= today) { p.streak_days } else { 0 },
        minutes_this_week(logs, today)
    );
    out.push_str(&format!("\nToday's lesson:\n1. Cards due: {}.", if l.cards.is_empty() { "none".to_string() } else { l.cards.iter().map(|(f, _)| f.clone()).collect::<Vec<_>>().join(", ") }));
    out.push_str(&format!("\n2. Reading — company of the day: {} ({}), which {}. {}", l.company.1, l.company.0, l.company.2, l.reading));
    out.push_str(&format!("\n3. Speaking (the user answers aloud on the phone's microphone): {}", l.speaking));
    out.push_str(&format!("\n4. Writing: {}", l.writing));
    out.push_str("\nAsk the cards one at a time and grade each with life_cards (action grade); log the practice with life_english (action log) at the end.");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::srs::new_card;

    #[test]
    fn a_lesson_a_day_and_the_streak() {
        let today = Date::new(2026, 10, 21).unwrap();
        let p = EnglishProfile { level: "b2".into(), minutes_per_day: 20, mode: true, ..EnglishProfile::default() };
        let cards = vec![new_card("e1".into(), "english", "guidance", "projeção da empresa", vec![], today), new_card("c1".into(), "concurso", "x", "y", vec![], today)];
        let l = lesson(&p, &cards, today);
        assert_eq!(l.level, "B2");
        assert_eq!(l.cards, vec![("guidance".to_string(), "projeção da empresa".to_string())], "only the English deck");
        let tomorrow = lesson(&p, &cards, today.add_days(1));
        assert_ne!(l.company.0, tomorrow.company.0, "a new company every day");
        assert!(l.reading.contains("quarterly results") && l.reading.contains("level B2"));
        assert!(mode_note(&p).contains("corrected sentence"));
        let mut prof = p.clone();
        let mut logs = Vec::new();
        practice(&mut prof, &mut logs, "Speaking", 10, today.add_days(-1));
        practice(&mut prof, &mut logs, "cards", 5, today);
        practice(&mut prof, &mut logs, "reading", 5, today);
        assert_eq!(prof.streak_days, 2);
        practice(&mut prof, &mut logs, "reading", 5, today.add_days(3));
        assert_eq!(prof.streak_days, 1, "a missed day starts over");
        assert_eq!(minutes_this_week(&logs, today), 20);
        assert_eq!(level_of(&EnglishProfile::default()), "B1");
        let text = for_model(&prof, &l, &logs, today);
        assert!(text.contains("English mode on") && text.contains("Speaking"), "{text}");
    }
}
