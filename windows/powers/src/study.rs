//! Studying for a public exam ("concurso") the way a good coach would plan it: the week's hours shared among the
//! subjects by how much each counts and how weak the user is in it, fifty-minute blocks spread over the days, the
//! reviews spaced at one, seven and thirty days, the subjects whose questions go badly, and the money: what passing is
//! worth (the post's salary over a career, in today's reais, times the user's own chance) and so each hour of study,
//! and how much sooner the portfolio reaches its milestones with that salary.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::model::{StudyLog, StudyProfile, Subject};
use crate::text::{brl, grouped, pct};

/// One block of study.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    /// 0 = Monday … 6 = Sunday.
    pub weekday: u32,
    pub subject: String,
    pub minutes: u32,
    /// "new", "review" or "questions".
    pub kind: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub hours_per_week: f64,
    /// Hours a week each subject gets.
    pub hours: Vec<(String, f64)>,
    pub week: Vec<Block>,
    pub weeks_left: Option<i64>,
    /// Subjects to review today (studied one, seven or thirty days ago).
    pub reviews_today: Vec<String>,
    /// Subjects whose questions went under 60 % in the last 30 days.
    pub weak: Vec<(String, f64)>,
}

const BLOCK: u32 = 50;

fn priority(s: &Subject) -> f64 {
    s.weight.max(0.1) * (6.0 - s.level.min(5) as f64) / 5.0
}

pub fn plan(profile: &StudyProfile, logs: &[StudyLog], today: Date) -> Plan {
    let hours_per_week = if profile.hours_per_week > 0.0 { profile.hours_per_week } else { 10.0 };
    let subjects: Vec<&Subject> = profile.subjects.iter().collect();
    let total: f64 = subjects.iter().map(|s| priority(s)).sum();
    // Ten percent of the week goes to reviews and questions on Sunday; the rest by priority, at least half an hour.
    let study_hours = hours_per_week * 0.9;
    let mut hours: Vec<(String, f64)> = subjects
        .iter()
        .map(|s| (s.name.clone(), if total > 0.0 { (study_hours * priority(s) / total).max(0.5) } else { 0.0 }))
        .collect();
    hours.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    // Blocks: each subject's hours as 50-minute blocks, dealt round-robin over Monday to Saturday so no day repeats a
    // subject while another waits.
    let mut queue: Vec<(String, u32)> = hours.iter().map(|(n, h)| (n.clone(), ((h * 60.0) / BLOCK as f64).round().max(1.0) as u32)).collect();
    let mut week = Vec::new();
    let mut day = 0u32;
    while queue.iter().any(|(_, n)| *n > 0) {
        for (name, n) in queue.iter_mut() {
            if *n == 0 {
                continue;
            }
            week.push(Block { weekday: day % 6, subject: name.clone(), minutes: BLOCK, kind: "new".into() });
            *n -= 1;
            day += 1;
        }
    }
    let sunday = ((hours_per_week * 0.1 * 60.0) / BLOCK as f64).round().max(1.0) as u32;
    for i in 0..sunday {
        week.push(Block {
            weekday: 6,
            subject: if i == 0 { "revisão da semana".into() } else { "questões".into() },
            minutes: BLOCK,
            kind: if i == 0 { "review".into() } else { "questions".into() },
        });
    }
    week.sort_by_key(|b| b.weekday);
    let studied_on = |d: Date| -> Vec<String> {
        let mut v: Vec<String> = logs.iter().filter(|l| l.date == d).map(|l| l.subject.clone()).collect();
        v.sort();
        v.dedup();
        v
    };
    let mut reviews_today: Vec<String> = [1i64, 7, 30].iter().flat_map(|n| studied_on(today.add_days(-*n))).collect();
    reviews_today.sort();
    reviews_today.dedup();
    let month_ago = today.add_days(-30);
    let mut per: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    for l in logs.iter().filter(|l| l.date > month_ago && l.questions > 0) {
        let e = per.entry(l.subject.clone()).or_default();
        e.0 += l.questions;
        e.1 += l.correct;
    }
    let weak = per.into_iter().filter(|(_, (q, _))| *q >= 10).map(|(s, (q, c))| (s, c as f64 / q as f64 * 100.0)).filter(|(_, p)| *p < 60.0).collect();
    Plan {
        hours_per_week,
        hours,
        week,
        weeks_left: profile.date.map(|d| (today.until(d) / 7).max(0)),
        reviews_today,
        weak,
    }
}

/// The blocks of one day.
pub fn today_blocks(plan: &Plan, today: Date) -> Vec<&Block> {
    plan.week.iter().filter(|b| b.weekday == today.weekday()).collect()
}

/// The money of passing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Value {
    /// What passing adds a month, net (about 75 % of the gross difference: income tax and pension).
    pub monthly_gain: f64,
    /// That gain over 30 years, in today's reais (5 % a year real).
    pub career_value: f64,
    pub hours_left: f64,
    /// Value of an hour of study at the user's chance (when they gave one) and at 10, 25 and 50 %.
    pub per_hour: Vec<(f64, f64)>,
}

pub fn value(profile: &StudyProfile, today: Date) -> Option<Value> {
    let salary = profile.salary.filter(|s| *s > 0.0)?;
    let gross_gain = salary - profile.current_income.unwrap_or(0.0);
    if gross_gain <= 0.0 {
        return None;
    }
    let monthly_gain = gross_gain * 0.75;
    let yearly = monthly_gain * 13.33;
    let r: f64 = 0.05;
    let career_value = yearly * (1.0 - (1.0 + r).powi(-30)) / r;
    let hours_per_week = if profile.hours_per_week > 0.0 { profile.hours_per_week } else { 10.0 };
    let weeks = profile.date.map(|d| (today.until(d) as f64 / 7.0).max(1.0)).unwrap_or(52.0);
    let hours_left = hours_per_week * weeks;
    let mut chances: Vec<f64> = vec![10.0, 25.0, 50.0];
    if let Some(c) = profile.chance.filter(|c| *c > 0.0 && *c <= 100.0) {
        chances.insert(0, c);
        chances.dedup();
    }
    let per_hour = chances.into_iter().map(|c| (c, career_value * c / 100.0 / hours_left)).collect();
    Some(Value { monthly_gain, career_value, hours_left, per_hour })
}

/// Months until `value` grows to `target` with `monthly` added each month at `yearly_rate` percent (real). None
/// when it never does within 100 years.
pub fn months_to(value: f64, monthly: f64, target: f64, yearly_rate: f64) -> Option<u32> {
    if value >= target {
        return Some(0);
    }
    let r = (1.0 + yearly_rate / 100.0).powf(1.0 / 12.0) - 1.0;
    let mut v = value;
    for n in 1..=1200 {
        v = v * (1.0 + r) + monthly;
        if v >= target {
            return Some(n);
        }
    }
    None
}

/// The portfolio's milestones with today's contributions and with the post's salary (30 % of the gain invested).
pub fn projection(portfolio_value: f64, monthly: f64, gain: Option<f64>, targets: &[f64]) -> Vec<(f64, Option<u32>, Option<u32>)> {
    targets
        .iter()
        .map(|t| {
            let now = months_to(portfolio_value, monthly, *t, 5.0);
            let with = gain.map(|g| monthly + g * 0.3).and_then(|m| months_to(portfolio_value, m, *t, 5.0));
            (*t, now, with)
        })
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub hours_this_week: f64,
    pub hours_planned: f64,
    /// Days in a row with study, up to today.
    pub streak: u32,
    pub questions_30d: u32,
    pub accuracy_30d: Option<f64>,
    pub hours_by_subject_30d: Vec<(String, f64)>,
}

pub fn progress(profile: &StudyProfile, logs: &[StudyLog], today: Date) -> Progress {
    let monday = today.add_days(-(today.weekday() as i64));
    let hours_this_week = logs.iter().filter(|l| l.date >= monday && l.date <= today).map(|l| l.minutes as f64 / 60.0).sum();
    let mut streak = 0;
    let mut d = today;
    while logs.iter().any(|l| l.date == d && l.minutes > 0) {
        streak += 1;
        d = d.add_days(-1);
    }
    if streak == 0 && logs.iter().any(|l| l.date == today.add_days(-1)) {
        // Not studied yet today: the streak still counts up to yesterday.
        let mut d = today.add_days(-1);
        while logs.iter().any(|l| l.date == d && l.minutes > 0) {
            streak += 1;
            d = d.add_days(-1);
        }
    }
    let recent: Vec<&StudyLog> = logs.iter().filter(|l| l.date > today.add_days(-30)).collect();
    let q: u32 = recent.iter().map(|l| l.questions).sum();
    let c: u32 = recent.iter().map(|l| l.correct).sum();
    let mut by: BTreeMap<String, f64> = BTreeMap::new();
    for l in &recent {
        *by.entry(l.subject.clone()).or_default() += l.minutes as f64 / 60.0;
    }
    let mut hours_by_subject_30d: Vec<(String, f64)> = by.into_iter().collect();
    hours_by_subject_30d.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    Progress {
        hours_this_week,
        hours_planned: if profile.hours_per_week > 0.0 { profile.hours_per_week } else { 10.0 },
        streak,
        questions_30d: q,
        accuracy_30d: (q > 0).then(|| c as f64 / q as f64 * 100.0),
        hours_by_subject_30d,
    }
}

const DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

pub fn for_model(profile: &StudyProfile, plan: &Plan, progress: &Progress, value: Option<&Value>, projection: &[(f64, Option<u32>, Option<u32>)], today: Date) -> String {
    if profile.exam.trim().is_empty() && profile.subjects.is_empty() {
        return "No exam is set yet. Ask the user which exam (post and board), its date, the salary, the hours a week they can study and the subjects with their weight (from the edital), then save it with life_study (action set).".into();
    }
    let mut out = format!(
        "Exam: {}{}{}.",
        profile.exam,
        if profile.board.is_empty() { String::new() } else { format!(" ({})", profile.board) },
        match (profile.date, plan.weeks_left) {
            (Some(d), Some(w)) => format!(", on {} — {} weeks left", d.br(), w),
            _ => String::new(),
        }
    );
    out.push_str(&format!("\nWeek of {} hours:", grouped(plan.hours_per_week, 1)));
    for (s, h) in &plan.hours {
        out.push_str(&format!(" {s} {}h;", grouped(*h, 1)));
    }
    let blocks = today_blocks(plan, today);
    if !blocks.is_empty() {
        out.push_str(&format!("\nToday ({}): {}.", DAYS[today.weekday() as usize], blocks.iter().map(|b| format!("{} ({} min)", b.subject, b.minutes)).collect::<Vec<_>>().join(", ")));
    }
    if !plan.reviews_today.is_empty() {
        out.push_str(&format!("\nTo review today (spaced 1/7/30 days): {}.", plan.reviews_today.join(", ")));
    }
    for (s, p) in &plan.weak {
        out.push_str(&format!("\nWeak spot: {s}, {} right in the last 30 days.", pct(*p, 0)));
    }
    out.push_str(&format!(
        "\nThis week {}h of {}h planned; {} day(s) in a row.",
        grouped(progress.hours_this_week, 1),
        grouped(progress.hours_planned, 0),
        progress.streak
    ));
    if let Some(a) = progress.accuracy_30d {
        out.push_str(&format!(" Questions in 30 days: {} at {} right.", progress.questions_30d, pct(a, 0)));
    }
    if let Some(v) = value {
        out.push_str(&format!(
            "\nPassing adds about {} a month net; over 30 years that is about {} in today's reais.",
            brl(v.monthly_gain),
            brl(v.career_value)
        ));
        let hours: Vec<String> = v.per_hour.iter().map(|(c, h)| format!("{} chance → {} an hour", pct(*c, 0), brl(*h))).collect();
        out.push_str(&format!(" Each hour of study ({} hours to go) is worth: {}.", grouped(v.hours_left, 0), hours.join("; ")));
    }
    for (target, now, with) in projection {
        let years = |m: Option<u32>| m.map(|m| format!("{} years", grouped(m as f64 / 12.0, 1))).unwrap_or_else(|| "never at this pace".into());
        out.push_str(&format!("\nPortfolio at {}: {} as today; {} with the post's salary (30 % of the gain invested).", brl(*target), years(*now), years(*with)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Date {
        Date::parse(s).unwrap()
    }

    fn profile() -> StudyProfile {
        StudyProfile {
            exam: "Analista do Banco Central".into(),
            board: "Cebraspe".into(),
            date: Some(d("2027-04-18")),
            salary: Some(20_924.80),
            current_income: Some(4_000.0),
            hours_per_week: 15.0,
            subjects: vec![
                Subject { name: "Economia".into(), weight: 30.0, level: 2, topics: vec![] },
                Subject { name: "Português".into(), weight: 15.0, level: 4, topics: vec![] },
                Subject { name: "Direito Administrativo".into(), weight: 10.0, level: 1, topics: vec![] },
            ],
            chance: Some(20.0),
        }
    }

    #[test]
    fn the_week_follows_weights_and_weak_spots() {
        let today = d("2026-10-21");
        let logs = vec![
            StudyLog { date: d("2026-10-20"), subject: "Economia".into(), minutes: 100, questions: 20, correct: 9 },
            StudyLog { date: d("2026-10-14"), subject: "Português".into(), minutes: 50, questions: 0, correct: 0 },
            StudyLog { date: d("2026-10-19"), subject: "Português".into(), minutes: 50, questions: 0, correct: 0 },
        ];
        let p = plan(&profile(), &logs, today);
        assert_eq!(p.hours[0].0, "Economia", "heaviest and weak");
        let total: f64 = p.hours.iter().map(|h| h.1).sum();
        assert!((total - 13.5).abs() < 1e-9, "90 % of the week by priority");
        assert!(p.week.iter().any(|b| b.weekday == 6 && b.kind == "review"));
        assert_eq!(p.week.iter().filter(|b| b.subject == "Economia").count(), 10, "8.1 hours in 50-minute blocks");
        assert!((0..6).all(|d| p.week.iter().any(|b| b.weekday == d)), "spread over Monday to Saturday");
        assert_eq!(p.reviews_today, vec!["Economia", "Português"], "studied yesterday and a week ago");
        assert_eq!(p.weak, vec![("Economia".to_string(), 45.0)]);
        assert_eq!(p.weeks_left, Some(25));
        let pr = progress(&profile(), &logs, today);
        assert!((pr.hours_this_week - 100.0 / 60.0 - 50.0 / 60.0).abs() < 1e-9, "Monday and Tuesday of this week");
        assert_eq!(pr.streak, 2, "Monday and Tuesday, not yet today");
        assert_eq!(pr.accuracy_30d, Some(45.0));
    }

    #[test]
    fn the_value_of_passing_and_of_an_hour() {
        let today = d("2026-10-21");
        let v = value(&profile(), today).unwrap();
        assert!((v.monthly_gain - (20_924.80 - 4_000.0) * 0.75).abs() < 1e-9);
        assert!(v.career_value > 2_000_000.0 && v.career_value < 4_000_000.0, "{}", v.career_value);
        assert_eq!(v.per_hour[0].0, 20.0, "the user's own chance first");
        assert!((v.per_hour[0].1 - v.career_value * 0.2 / v.hours_left).abs() < 1e-6);
        assert!(value(&StudyProfile::default(), today).is_none());
        assert_eq!(months_to(1000.0, 0.0, 500.0, 5.0), Some(0));
        let m = months_to(7_900.0, 500.0, 100_000.0, 5.0).unwrap();
        assert!(m > 100 && m < 160, "{m}");
        assert_eq!(months_to(0.0, 0.0, 1.0, 0.0), None);
        let proj = projection(7_900.0, 500.0, Some(v.monthly_gain), &[100_000.0, 1_000_000.0]);
        assert!(proj[0].2.unwrap() < proj[0].1.unwrap(), "the salary gets there sooner");
        let text = for_model(&profile(), &plan(&profile(), &[], today), &progress(&profile(), &[], today), Some(&v), &proj, today);
        assert!(text.contains("Each hour of study") && text.contains("Cebraspe"), "{text}");
        assert!(for_model(&StudyProfile::default(), &Plan::default(), &Progress::default(), None, &[], today).contains("No exam"));
    }
}
