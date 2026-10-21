//! The user's day in one list: the portfolio's events (a record date, a payment, an assembly's deadline, a DARF), the
//! decisions to look at again, the study blocks and reviews, the training (or rest) and the English lesson — and the
//! morning briefing Zheus says from it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::date::{last_business_day, Date};
use crate::fii::{FundEvent, Importance};
use crate::text::brl;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// "money", "decision", "study", "training", "english".
    pub area: String,
    pub title: String,
    #[serde(default)]
    pub detail: String,
    /// Something the user must not miss today.
    #[serde(default)]
    pub urgent: bool,
    /// What it is, for the island to say in the user's language: a fund event's kind ("record", "payment",
    /// "amortization", "assembly", "offering", "material_fact", "notice", "report"), "darf", "decision", "study",
    /// "study_review", "training", "training_light", "rest" or "english"; its values in `vars`.
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub vars: BTreeMap<String, String>,
}

fn vars(pairs: &[(&str, String)]) -> BTreeMap<String, String> {
    pairs.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| (k.to_string(), v.clone())).collect()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayPlan {
    pub date: Date,
    pub items: Vec<Item>,
}

/// What goes into a day.
pub struct Inputs<'a> {
    pub today: Date,
    pub events: &'a [FundEvent],
    /// Decisions with a review due (id, "buy GARE11").
    pub reviews: Vec<(String, String)>,
    /// A DARF to pay this month (amount), if any.
    pub darf: Option<f64>,
    /// Today's study blocks ("Economia (50 min)") and the subjects to review.
    pub study: Vec<String>,
    pub study_reviews: Vec<String>,
    /// The next training session and the day's readiness ("normal", "light", "rest").
    pub training: Option<(String, String)>,
    /// The company of the English lesson, and cards due.
    pub english: Option<(String, usize)>,
}

pub fn plan(i: Inputs) -> DayPlan {
    let mut items = Vec::new();
    for e in i.events {
        let today = e.date == i.today || e.deadline == Some(i.today);
        let tomorrow = e.date == i.today.add_days(1) || e.deadline == Some(i.today.add_days(1));
        let soon_deadline = e.deadline.is_some_and(|d| d >= i.today && i.today.until(d) <= 3);
        if !(today || tomorrow || soon_deadline || (e.importance == Importance::High && i.today.until(e.date) >= -2 && e.date <= i.today)) {
            continue;
        }
        let when = if today { "today" } else if tomorrow { "tomorrow" } else { "soon" };
        items.push(Item {
            area: "money".into(),
            title: format!("{} — {} ({when})", e.ticker, e.title),
            detail: e.action.clone(),
            urgent: e.importance == Importance::High || (e.kind == "record" && today),
            code: e.kind.clone(),
            vars: vars(&[("ticker", e.ticker.clone()), ("when", when.into()), ("amount", e.amount.map(|a| format!("{a:.2}")).unwrap_or_default())]),
        });
    }
    if let Some(amount) = i.darf.filter(|a| *a > 0.0) {
        let due = last_business_day(i.today.y, i.today.m);
        let left = i.today.until(due);
        if (0..=5).contains(&left) {
            items.push(Item {
                area: "money".into(),
                title: format!("DARF 6015 of {} due {}", brl(amount), due.br()),
                detail: "The month's gains in the stock market: pay it in the bank's app or the Receita's site.".into(),
                urgent: left <= 1,
                code: "darf".into(),
                vars: vars(&[("amount", format!("{amount:.2}")), ("due", due.to_string())]),
            });
        }
    }
    for (id, what) in &i.reviews {
        items.push(Item {
            area: "decision".into(),
            title: format!("Look again at your decision: {what}"),
            detail: format!("Journal entry {id}: right, wrong or mixed?"),
            urgent: false,
            code: "decision".into(),
            vars: vars(&[("id", id.clone()), ("what", what.clone())]),
        });
    }
    if !i.study.is_empty() {
        items.push(Item {
            area: "study".into(),
            title: format!("Study: {}", i.study.join(", ")),
            detail: String::new(),
            urgent: false,
            code: "study".into(),
            vars: vars(&[("subjects", i.study.join(", "))]),
        });
    }
    if !i.study_reviews.is_empty() {
        items.push(Item {
            area: "study".into(),
            title: format!("Review: {}", i.study_reviews.join(", ")),
            detail: "Spaced at 1, 7 and 30 days.".into(),
            urgent: false,
            code: "study_review".into(),
            vars: vars(&[("subjects", i.study_reviews.join(", "))]),
        });
    }
    if let Some((session, readiness)) = &i.training {
        let (title, code) = match readiness.as_str() {
            "rest" => ("Training: rest or a walk today".to_string(), "rest"),
            "light" => (format!("Training: {session}, light"), "training_light"),
            _ => (format!("Training: {session}"), "training"),
        };
        items.push(Item { area: "training".into(), title, detail: String::new(), urgent: false, code: code.into(), vars: vars(&[("session", session.clone())]) });
    }
    if let Some((company, cards)) = &i.english {
        items.push(Item {
            area: "english".into(),
            title: format!("English: {company}'s results{}", if *cards > 0 { format!(" and {cards} cards") } else { String::new() }),
            detail: String::new(),
            urgent: false,
            code: "english".into(),
            vars: vars(&[("company", company.clone()), ("cards", cards.to_string())]),
        });
    }
    items.sort_by_key(|x| !x.urgent);
    DayPlan { date: i.today, items }
}

pub fn for_model(p: &DayPlan) -> String {
    if p.items.is_empty() {
        return format!("Nothing planned for {} in the portfolio, studies, training or English.", p.date.br());
    }
    let mut out = format!("The user's day, {}:", p.date.br());
    for x in &p.items {
        out.push_str(&format!("\n- [{}{}] {}{}", x.area, if x.urgent { ", don't miss" } else { "" }, x.title, if x.detail.is_empty() { String::new() } else { format!(" — {}", x.detail) }));
    }
    out.push_str("\nGive it as a short morning briefing, the things not to miss first.");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_days_items_with_what_not_to_miss_first() {
        let today = Date::new(2026, 10, 28).unwrap();
        let ev = |kind: &str, date: Date, deadline: Option<Date>, imp: Importance| FundEvent {
            ticker: "GGRC11".into(),
            date,
            kind: kind.into(),
            title: format!("{kind} title"),
            action: "do it".into(),
            deadline,
            url: None,
            importance: imp,
            amount: None,
        };
        let events = vec![
            ev("record", today, Some(today), Importance::Normal),
            ev("payment", today.add_days(20), None, Importance::Low),
            ev("assembly", today.add_days(-10), Some(today.add_days(2)), Importance::High),
        ];
        let p = plan(Inputs {
            today,
            events: &events,
            reviews: vec![("d-1".into(), "buy GARE11".into())],
            darf: Some(199.68),
            study: vec!["Economia (50 min)".into()],
            study_reviews: vec!["Português".into()],
            training: Some(("Upper B".into(), "light".into())),
            english: Some(("Nvidia".into(), 4)),
        });
        let titles: Vec<&str> = p.items.iter().map(|i| i.title.as_str()).collect();
        assert!(p.items[0].urgent);
        assert!(titles.iter().any(|t| t.contains("record title (today)")));
        assert!(titles.iter().any(|t| t.contains("assembly title (soon)")));
        assert!(!titles.iter().any(|t| t.contains("payment title")), "twenty days away");
        assert!(titles.iter().any(|t| t.starts_with("DARF 6015")), "due the 30th");
        assert!(titles.iter().any(|t| t == &"Training: Upper B, light"));
        assert!(titles.iter().any(|t| t.contains("Nvidia") && t.contains("4 cards")));
        let record = p.items.iter().find(|i| i.code == "record").unwrap();
        assert_eq!((record.vars["ticker"].as_str(), record.vars["when"].as_str()), ("GGRC11", "today"));
        let darf = p.items.iter().find(|i| i.code == "darf").unwrap();
        assert_eq!((darf.vars["amount"].as_str(), darf.vars["due"].as_str()), ("199.68", "2026-10-30"));
        assert!(p.items.iter().any(|i| i.code == "training_light" && i.vars["session"] == "Upper B"));
        assert!(p.items.iter().any(|i| i.code == "english" && i.vars["cards"] == "4"));
        assert!(for_model(&p).contains("don't miss"));
        let empty = plan(Inputs { today, events: &[], reviews: vec![], darf: None, study: vec![], study_reviews: vec![], training: None, english: None });
        assert!(for_model(&empty).contains("Nothing planned"));
    }
}
