//! The month as content: square charts ready to post (the portfolio's growth, the passive income, the allocation, the
//! hours of study, the training), drawn from the user's own records as SVG, and the brief a post is written from. In
//! private mode (the default) no amount in reais appears: growth as an index, the allocation in percent. Nothing is
//! ever posted: the files are the user's to use.

use std::collections::BTreeMap;

use crate::date::Date;
use crate::model::{AssetClass, Snapshot, Store};
use crate::text::{brl, grouped, pct};

const W: f64 = 1080.0;
const H: f64 = 1080.0;
const COLORS: [&str; 8] = ["#818cf8", "#22d3ee", "#f472b6", "#fbbf24", "#34d399", "#a78bfa", "#fb7185", "#94a3b8"];

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn frame(title: &str, subtitle: &str, body: &str) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">
<defs><radialGradient id="g1" cx="0.1" cy="0" r="0.9"><stop offset="0" stop-color="#6366f1" stop-opacity="0.45"/><stop offset="1" stop-color="#6366f1" stop-opacity="0"/></radialGradient>
<radialGradient id="g2" cx="1" cy="1" r="0.8"><stop offset="0" stop-color="#ec4899" stop-opacity="0.28"/><stop offset="1" stop-color="#ec4899" stop-opacity="0"/></radialGradient></defs>
<rect width="{W}" height="{H}" fill="#0d0f17"/><rect width="{W}" height="{H}" fill="url(#g1)"/><rect width="{W}" height="{H}" fill="url(#g2)"/>
<rect x="60" y="230" width="960" height="720" rx="40" fill="#ffffff" fill-opacity="0.06" stroke="#ffffff" stroke-opacity="0.14"/>
<text x="80" y="130" font-family="Segoe UI, system-ui, sans-serif" font-size="58" font-weight="700" fill="#f5f6f8">{}</text>
<text x="80" y="190" font-family="Segoe UI, system-ui, sans-serif" font-size="32" fill="#aab0bb">{}</text>
{body}
<text x="80" y="1030" font-family="Segoe UI, system-ui, sans-serif" font-size="24" fill="#6b7079">✦ Zheus</text>
</svg>
"##,
        esc(title),
        esc(subtitle)
    )
}

/// A line (with its area) of `points`, labelled under each point.
pub fn line_chart(title: &str, subtitle: &str, points: &[(String, f64)], show: impl Fn(f64) -> String) -> String {
    if points.is_empty() {
        return frame(title, subtitle, "");
    }
    let (x0, x1, y0, y1) = (120.0, 960.0, 330.0, 820.0);
    let max = points.iter().map(|p| p.1).fold(f64::MIN, f64::max);
    let min = points.iter().map(|p| p.1).fold(f64::MAX, f64::min);
    let span = if (max - min).abs() < 1e-9 { max.abs().max(1.0) * 0.1 } else { max - min };
    let (lo, hi) = (min - span * 0.15, max + span * 0.15);
    let x = |i: usize| if points.len() == 1 { (x0 + x1) / 2.0 } else { x0 + (x1 - x0) * i as f64 / (points.len() - 1) as f64 };
    let y = |v: f64| y1 - (v - lo) / (hi - lo) * (y1 - y0);
    let path: Vec<String> = points.iter().enumerate().map(|(i, p)| format!("{:.1},{:.1}", x(i), y(p.1))).collect();
    let mut body = format!(
        r##"<polygon points="{x0:.1},{y1:.1} {} {:.1},{y1:.1}" fill="#818cf8" fill-opacity="0.18"/><polyline points="{}" fill="none" stroke="#818cf8" stroke-width="6" stroke-linejoin="round" stroke-linecap="round"/>"##,
        path.join(" "),
        x(points.len() - 1),
        path.join(" ")
    );
    for (i, (label, v)) in points.iter().enumerate() {
        body.push_str(&format!(r##"<circle cx="{:.1}" cy="{:.1}" r="9" fill="#ffffff"/>"##, x(i), y(*v)));
        if points.len() <= 12 {
            body.push_str(&format!(
                r##"<text x="{:.1}" y="880" text-anchor="middle" font-family="Segoe UI, system-ui, sans-serif" font-size="24" fill="#aab0bb">{}</text>"##,
                x(i),
                esc(label)
            ));
        }
    }
    let last = points.last().map(|p| show(p.1)).unwrap_or_default();
    body.push_str(&format!(
        r##"<text x="{:.1}" y="{:.1}" text-anchor="end" font-family="Segoe UI, system-ui, sans-serif" font-size="40" font-weight="700" fill="#f5f6f8">{}</text>"##,
        x(points.len() - 1),
        (y(points.last().map(|p| p.1).unwrap_or(0.0)) - 26.0).max(300.0),
        esc(&last)
    ));
    frame(title, subtitle, &body)
}

/// Bars of `values`, with their labels and numbers.
pub fn bar_chart(title: &str, subtitle: &str, values: &[(String, f64)], show: impl Fn(f64) -> String) -> String {
    let (x0, x1, y0, y1) = (110.0, 970.0, 320.0, 830.0);
    let max = values.iter().map(|v| v.1).fold(0.0, f64::max).max(1e-9);
    let n = values.len().max(1) as f64;
    let slot = (x1 - x0) / n;
    let mut body = String::new();
    for (i, (label, v)) in values.iter().enumerate() {
        let h = (v / max) * (y1 - y0);
        let x = x0 + slot * i as f64 + slot * 0.18;
        let w = slot * 0.64;
        body.push_str(&format!(
            r##"<rect x="{x:.1}" y="{:.1}" width="{w:.1}" height="{h:.1}" rx="14" fill="{}"/><text x="{:.1}" y="{:.1}" text-anchor="middle" font-family="Segoe UI, system-ui, sans-serif" font-size="26" font-weight="600" fill="#f5f6f8">{}</text><text x="{:.1}" y="880" text-anchor="middle" font-family="Segoe UI, system-ui, sans-serif" font-size="24" fill="#aab0bb">{}</text>"##,
            y1 - h,
            COLORS[i % COLORS.len()],
            x + w / 2.0,
            y1 - h - 14.0,
            esc(&show(*v)),
            x + w / 2.0,
            esc(label)
        ));
    }
    frame(title, subtitle, &body)
}

/// A donut of `slices` (label, share), with its legend.
pub fn donut(title: &str, subtitle: &str, slices: &[(String, f64)]) -> String {
    let total: f64 = slices.iter().map(|s| s.1).sum();
    let (cx, cy, r) = (380.0, 590.0, 230.0);
    let mut body = String::new();
    let mut angle = -std::f64::consts::FRAC_PI_2;
    for (i, (label, v)) in slices.iter().enumerate() {
        let share = if total > 0.0 { v / total } else { 0.0 };
        let sweep = share * std::f64::consts::TAU;
        let (a0, a1) = (angle, angle + sweep.min(std::f64::consts::TAU - 1e-4));
        let large = if sweep > std::f64::consts::PI { 1 } else { 0 };
        body.push_str(&format!(
            r##"<path d="M {:.1} {:.1} A {r} {r} 0 {large} 1 {:.1} {:.1}" fill="none" stroke="{}" stroke-width="90"/>"##,
            cx + r * a0.cos(),
            cy + r * a0.sin(),
            cx + r * a1.cos(),
            cy + r * a1.sin(),
            COLORS[i % COLORS.len()]
        ));
        body.push_str(&format!(
            r##"<rect x="700" y="{:.1}" width="34" height="34" rx="8" fill="{}"/><text x="750" y="{:.1}" font-family="Segoe UI, system-ui, sans-serif" font-size="30" fill="#f5f6f8">{} · {}</text>"##,
            400.0 + i as f64 * 64.0,
            COLORS[i % COLORS.len()],
            428.0 + i as f64 * 64.0,
            esc(label),
            esc(&pct(share * 100.0, 1))
        ));
        angle += sweep;
    }
    frame(title, subtitle, &body)
}

/// One file of the kit.
pub struct File {
    pub name: String,
    pub svg: String,
    pub about: String,
}

const MONTHS: [&str; 12] = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];

fn month_label(d: Date) -> String {
    format!("{}/{}", MONTHS[(d.m - 1) as usize], d.y % 100)
}

/// The last snapshot of each month, up to `month`'s end, at most twelve.
fn monthly(snapshots: &[Snapshot], end: Date) -> Vec<&Snapshot> {
    let mut by: BTreeMap<String, &Snapshot> = BTreeMap::new();
    for s in snapshots.iter().filter(|s| s.date <= end) {
        by.insert(s.date.month_key(), s);
    }
    let list: Vec<&Snapshot> = by.into_values().collect();
    list[list.len().saturating_sub(12)..].to_vec()
}

/// Weeks (Monday to Sunday) ending at the week of `end`, oldest first: their Mondays.
fn weeks(end: Date, n: usize) -> Vec<Date> {
    let monday = end.add_days(-(end.weekday() as i64));
    (0..n).rev().map(|i| monday.add_days(-7 * i as i64)).collect()
}

/// The kit of the month that ends at `end` (the month's last day, or today).
pub fn kit(store: &Store, end: Date) -> (Vec<File>, String) {
    let private = store.content.private;
    let mut files = Vec::new();
    let mut facts: Vec<String> = Vec::new();
    let label = month_label(end);
    let months = monthly(&store.snapshots, end);
    if let (Some(first), Some(last)) = (months.first(), months.last()) {
        let base = first.value.max(1e-9);
        if months.len() >= 2 {
            let points: Vec<(String, f64)> = months.iter().map(|s| (month_label(s.date), if private { s.value / base * 100.0 } else { s.value })).collect();
            let growth = (last.value / base - 1.0) * 100.0;
            let subtitle = if private { format!("Índice (início = 100) · {} no período", pct(growth, 1)) } else { format!("{} no período", pct(growth, 1)) };
            files.push(File {
                name: "1-patrimonio.svg".into(),
                svg: line_chart(&format!("Minha carteira até {label}"), &subtitle, &points, |v| if private { grouped(v, 1) } else { brl(v) }),
                about: "The portfolio's growth, month by month.".into(),
            });
            facts.push(format!("Portfolio growth since {}: {}{}", month_label(first.date), pct(growth, 1), if private { String::new() } else { format!(" ({} → {})", brl(first.value), brl(last.value)) }));
        } else {
            facts.push("This is the first month Coucou follows the portfolio: its growth shows from next month's kit (do not make one up).".into());
        }
        let incomes: Vec<(String, f64)> = months.iter().filter(|s| s.income > 0.0).map(|s| (month_label(s.date), s.income)).collect();
        if incomes.len() >= 2 {
            let base = incomes[0].1.max(1e-9);
            let shown: Vec<(String, f64)> = incomes.iter().map(|(l, v)| (l.clone(), if private { v / base * 100.0 } else { *v })).collect();
            let g = (incomes.last().map(|x| x.1).unwrap_or(base) / base - 1.0) * 100.0;
            files.push(File {
                name: "2-renda-passiva.svg".into(),
                svg: bar_chart("Renda passiva por mês", &format!("{} desde {}", pct(g, 1), incomes[0].0), &shown, |v| if private { grouped(v, 0) } else { brl(v) }),
                about: "The estimated passive income each month.".into(),
            });
            facts.push(format!("Passive income: {} since {}", pct(g, 1), incomes[0].0));
        }
        let slices: Vec<(String, f64)> = last.by_class.iter().filter(|(_, v)| **v > 0.0).map(|(c, v)| (c.label().to_string(), *v)).collect();
        if !slices.is_empty() {
            let fii = last.by_class.get(&AssetClass::Fii).copied().unwrap_or(0.0) / last.value.max(1e-9) * 100.0;
            files.push(File { name: "3-alocacao.svg".into(), svg: donut("Como a carteira está dividida", &format!("{label} · em %"), &slices), about: "The allocation by class, in percent.".into() });
            facts.push(format!("Allocation: {}", slices.iter().map(|(l, v)| format!("{l} {}", pct(v / last.value.max(1e-9) * 100.0, 0))).collect::<Vec<_>>().join(", ")));
            if fii > 50.0 {
                facts.push(format!("FIIs are {} of the portfolio", pct(fii, 0)));
            }
        }
    }
    let ws = weeks(end, 8);
    let week_label = |m: Date| m.short();
    let study: Vec<(String, f64)> = ws
        .iter()
        .map(|m| (week_label(*m), store.study_logs.iter().filter(|l| l.date >= *m && l.date < m.add_days(7)).map(|l| l.minutes as f64 / 60.0).sum()))
        .collect();
    let total_study: f64 = study.iter().map(|s| s.1).sum();
    if total_study > 0.0 {
        files.push(File {
            name: "4-estudos.svg".into(),
            svg: bar_chart("Horas de estudo por semana", &format!("{} nas últimas 8 semanas", format!("{}h", grouped(total_study, 0))), &study, |v| format!("{}h", grouped(v, 1))),
            about: "Hours of study each week.".into(),
        });
        facts.push(format!("Study: {}h in 8 weeks{}", grouped(total_study, 0), if store.study.exam.is_empty() { String::new() } else { format!(" for {}", store.study.exam) }));
    }
    let training: Vec<(String, f64)> = ws.iter().map(|m| (week_label(*m), store.workouts.iter().filter(|w| w.date >= *m && w.date < m.add_days(7)).count() as f64)).collect();
    let sessions: f64 = training.iter().map(|t| t.1).sum();
    if sessions > 0.0 {
        files.push(File {
            name: "5-treinos.svg".into(),
            svg: bar_chart("Treinos por semana", &format!("{} treinos em 8 semanas", grouped(sessions, 0)), &training, |v| grouped(v, 0)),
            about: "Training sessions each week.".into(),
        });
        facts.push(format!("Training: {} sessions in 8 weeks", grouped(sessions, 0)));
        // Personal records: the heaviest set of each exercise this month against before.
        let month_start = end.first_of_month();
        let mut prs = Vec::new();
        let mut names: Vec<&str> = store.workouts.iter().flat_map(|w| w.exercises.iter().map(|e| e.name.as_str())).collect();
        names.sort();
        names.dedup();
        for n in names {
            let best = |f: &dyn Fn(Date) -> bool| {
                store.workouts.iter().filter(|w| f(w.date)).flat_map(|w| w.exercises.iter().filter(|e| e.name == n)).flat_map(|e| e.sets.iter().map(|s| s.kg)).fold(0.0, f64::max)
            };
            let now = best(&|d| d >= month_start && d <= end);
            let before = best(&|d| d < month_start);
            if now > before && before > 0.0 {
                prs.push(format!("{n} {} → {} kg", grouped(before, 1), grouped(now, 1)));
            }
        }
        if !prs.is_empty() {
            facts.push(format!("Personal records this month: {}", prs.join("; ")));
        }
    }
    let english: u32 = store.practice.iter().filter(|p| p.date >= end.first_of_month() && p.date <= end).map(|p| p.minutes).sum();
    if english > 0 {
        facts.push(format!("English: {} minutes this month, streak {} days", english, store.english.streak_days));
    }
    let brief = if facts.is_empty() {
        "Not enough of the user's records yet for a month's content: the portfolio's history starts with its first refresh, studies and training with their first logs.".to_string()
    } else {
        format!(
            "Content kit for {label} ({}):\n- {}\nCharts made: {}.\nWrite with the user, in Portuguese and in their voice: (1) a 6-slide carousel \"Minha jornada em {label}\" (hook, portfolio, passive income, studies, training, lesson of the month + a question for comments); (2) a 30-second Reels script; (3) a story with a poll. Real numbers only, {}. Nothing is posted by Zheus: the user posts it.",
            if private { "private mode: percentages, no reais" } else { "with reais" },
            facts.join("\n- "),
            files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>().join(", "),
            if private { "percentages and growth only, never amounts" } else { "amounts allowed" }
        )
    };
    (files, brief)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ExerciseLog, SetLog, StudyLog, Workout};

    #[test]
    fn a_months_kit_from_the_users_records() {
        let end = Date::new(2026, 10, 31).unwrap();
        let mut store = Store::default();
        for (i, v) in [5800.0, 6050.0, 6300.0, 6489.6].iter().enumerate() {
            store.snapshots.push(Snapshot {
                date: Date::new(2026, 7 + i as u32, 15).unwrap(),
                value: *v,
                cost: 7000.0,
                by_class: [(AssetClass::Fii, v * 0.84), (AssetClass::Etf, v * 0.10), (AssetClass::Acao, v * 0.06)].into(),
                cost_by_class: BTreeMap::new(),
                income: 50.0 + i as f64 * 5.0,
                prices: BTreeMap::new(),
            });
        }
        store.study_logs.push(StudyLog { date: Date::new(2026, 10, 20).unwrap(), subject: "Economia".into(), minutes: 120, questions: 0, correct: 0 });
        let w = |d: u32, kg: f64| Workout {
            date: Date::new(2026, 10, d).unwrap(),
            session: "Upper A".into(),
            exercises: vec![ExerciseLog { name: "Supino reto".into(), sets: vec![SetLog { reps: 8, kg }] }],
            minutes: 60,
            effort: None,
            note: String::new(),
        };
        store.workouts = vec![Workout { date: Date::new(2026, 9, 20).unwrap(), ..w(1, 60.0) }, w(20, 62.5)];
        let (files, brief) = kit(&store, end);
        let names: Vec<&str> = files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["1-patrimonio.svg", "2-renda-passiva.svg", "3-alocacao.svg", "4-estudos.svg", "5-treinos.svg"]);
        assert!(files.iter().all(|f| f.svg.starts_with("<svg") && f.svg.trim_end().ends_with("</svg>")));
        assert!(!files[0].svg.contains("R$"), "private mode shows no reais");
        assert!(brief.contains("Supino reto 60,0 → 62,5 kg"), "{brief}");
        assert!(brief.contains("Nothing is posted by Zheus"));
        store.content.private = false;
        let (files, _) = kit(&store, end);
        assert!(files[0].svg.contains("R$ 6.489,60"));
        assert!(kit(&Store::default(), end).1.contains("Not enough"));
        // The first month followed: no growth to show yet, the allocation is.
        store.snapshots.drain(..3);
        let (files, brief) = kit(&store, end);
        assert_eq!(files[0].name, "3-alocacao.svg");
        assert!(brief.contains("first month") && !brief.contains("Portfolio growth"), "{brief}");
    }
}
