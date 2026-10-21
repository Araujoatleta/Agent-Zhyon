//! Training that talks with the rest of the user's life: a split with its sessions and exercises, the next session in
//! the rotation, each exercise's last performance and its progression (double progression: all sets at the top of
//! the range means more weight next time), the week's sets per muscle against 10–20, and the day's readiness from the
//! user's sleep, soreness, energy and study load — a lighter session, or rest, when the day needs it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::model::{ExerciseLog, GymProfile, Wellness, Workout};
use crate::text::{grouped, plain};

/// An exercise of a template: name, muscle group, sets, rep range.
pub type Exercise = (&'static str, &'static str, u32, (u32, u32));

/// The sessions of a split, in rotation.
pub fn template(split: &str) -> Vec<(&'static str, Vec<Exercise>)> {
    match plain(split).replace(' ', "_").as_str() {
        "full_body" | "fullbody" | "full" => vec![
            (
                "Full A",
                vec![
                    ("Agachamento livre", "pernas", 3, (6, 10)),
                    ("Supino reto", "peito", 3, (6, 10)),
                    ("Remada curvada", "costas", 3, (8, 12)),
                    ("Desenvolvimento com halteres", "ombros", 3, (8, 12)),
                    ("Rosca direta", "bíceps", 2, (10, 15)),
                    ("Prancha", "core", 3, (30, 60)),
                ],
            ),
            (
                "Full B",
                vec![
                    ("Levantamento terra romeno", "posteriores", 3, (6, 10)),
                    ("Supino inclinado com halteres", "peito", 3, (8, 12)),
                    ("Puxada frente", "costas", 3, (8, 12)),
                    ("Elevação lateral", "ombros", 3, (12, 15)),
                    ("Tríceps corda", "tríceps", 2, (10, 15)),
                    ("Panturrilha em pé", "panturrilhas", 3, (10, 15)),
                ],
            ),
        ],
        "abc" | "abcabc" => vec![
            (
                "A — peito, ombros, tríceps",
                vec![
                    ("Supino reto", "peito", 4, (6, 10)),
                    ("Supino inclinado com halteres", "peito", 3, (8, 12)),
                    ("Desenvolvimento com halteres", "ombros", 3, (8, 12)),
                    ("Elevação lateral", "ombros", 3, (12, 15)),
                    ("Tríceps corda", "tríceps", 3, (10, 15)),
                    ("Tríceps francês", "tríceps", 2, (10, 12)),
                ],
            ),
            (
                "B — costas, bíceps",
                vec![
                    ("Barra fixa ou puxada frente", "costas", 4, (6, 10)),
                    ("Remada curvada", "costas", 3, (8, 12)),
                    ("Remada baixa", "costas", 3, (10, 12)),
                    ("Face pull", "ombros", 3, (12, 15)),
                    ("Rosca direta", "bíceps", 3, (8, 12)),
                    ("Rosca martelo", "bíceps", 2, (10, 15)),
                ],
            ),
            (
                "C — pernas",
                vec![
                    ("Agachamento livre", "pernas", 4, (6, 10)),
                    ("Leg press", "pernas", 3, (10, 15)),
                    ("Stiff", "posteriores", 3, (8, 12)),
                    ("Cadeira extensora", "pernas", 3, (12, 15)),
                    ("Mesa flexora", "posteriores", 3, (10, 15)),
                    ("Panturrilha em pé", "panturrilhas", 4, (10, 15)),
                ],
            ),
        ],
        "ppl" | "push_pull_legs" => vec![
            (
                "Push",
                vec![
                    ("Supino reto", "peito", 4, (6, 10)),
                    ("Desenvolvimento militar", "ombros", 3, (6, 10)),
                    ("Supino inclinado com halteres", "peito", 3, (8, 12)),
                    ("Elevação lateral", "ombros", 3, (12, 15)),
                    ("Tríceps corda", "tríceps", 3, (10, 15)),
                ],
            ),
            (
                "Pull",
                vec![
                    ("Barra fixa ou puxada frente", "costas", 4, (6, 10)),
                    ("Remada curvada", "costas", 3, (8, 12)),
                    ("Face pull", "ombros", 3, (12, 15)),
                    ("Rosca direta", "bíceps", 3, (8, 12)),
                    ("Rosca martelo", "bíceps", 2, (10, 15)),
                ],
            ),
            (
                "Legs",
                vec![
                    ("Agachamento livre", "pernas", 4, (6, 10)),
                    ("Stiff", "posteriores", 3, (8, 12)),
                    ("Leg press", "pernas", 3, (10, 15)),
                    ("Mesa flexora", "posteriores", 3, (10, 15)),
                    ("Panturrilha em pé", "panturrilhas", 4, (10, 15)),
                ],
            ),
        ],
        _ => vec![
            (
                "Upper A",
                vec![
                    ("Supino reto", "peito", 3, (6, 10)),
                    ("Remada curvada", "costas", 3, (8, 12)),
                    ("Desenvolvimento com halteres", "ombros", 3, (8, 12)),
                    ("Puxada frente", "costas", 3, (8, 12)),
                    ("Rosca direta", "bíceps", 2, (10, 15)),
                    ("Tríceps corda", "tríceps", 2, (10, 15)),
                ],
            ),
            (
                "Lower A",
                vec![
                    ("Agachamento livre", "pernas", 3, (6, 10)),
                    ("Stiff", "posteriores", 3, (8, 12)),
                    ("Leg press", "pernas", 3, (10, 15)),
                    ("Mesa flexora", "posteriores", 2, (10, 15)),
                    ("Panturrilha em pé", "panturrilhas", 3, (10, 15)),
                    ("Abdominal na polia", "core", 3, (10, 15)),
                ],
            ),
            (
                "Upper B",
                vec![
                    ("Supino inclinado com halteres", "peito", 3, (8, 12)),
                    ("Remada baixa", "costas", 3, (10, 12)),
                    ("Elevação lateral", "ombros", 3, (12, 15)),
                    ("Barra fixa ou puxada frente", "costas", 3, (6, 10)),
                    ("Rosca martelo", "bíceps", 2, (10, 15)),
                    ("Tríceps francês", "tríceps", 2, (10, 12)),
                ],
            ),
            (
                "Lower B",
                vec![
                    ("Levantamento terra romeno", "posteriores", 3, (6, 10)),
                    ("Agachamento búlgaro", "pernas", 3, (8, 12)),
                    ("Cadeira extensora", "pernas", 3, (12, 15)),
                    ("Cadeira flexora", "posteriores", 2, (10, 15)),
                    ("Panturrilha sentado", "panturrilhas", 3, (12, 15)),
                    ("Prancha", "core", 3, (30, 60)),
                ],
            ),
        ],
    }
}

fn lower_body(group: &str) -> bool {
    matches!(group, "pernas" | "posteriores" | "panturrilhas")
}

/// The muscle group of an exercise, from the templates (by its name), else "outros".
pub fn group_of(name: &str) -> &'static str {
    let n = plain(name);
    for split in ["upper_lower", "abc", "ppl", "full_body"] {
        for (_, list) in template(split) {
            if let Some(e) = list.iter().find(|e| plain(e.0) == n) {
                return e.1;
            }
        }
    }
    match n.as_str() {
        x if x.contains("supino") || x.contains("crucifixo") || x.contains("flexao") => "peito",
        x if x.contains("remada") || x.contains("puxada") || x.contains("barra") || x.contains("pulldown") => "costas",
        x if x.contains("agachamento") || x.contains("leg") || x.contains("extensora") || x.contains("afundo") => "pernas",
        x if x.contains("stiff") || x.contains("flexora") || x.contains("terra") => "posteriores",
        x if x.contains("rosca") => "bíceps",
        x if x.contains("triceps") => "tríceps",
        x if x.contains("desenvolvimento") || x.contains("elevacao") || x.contains("face pull") => "ombros",
        x if x.contains("panturrilha") => "panturrilhas",
        x if x.contains("abdominal") || x.contains("prancha") => "core",
        _ => "outros",
    }
}

/// The session after the last one logged, in the rotation of the user's split.
pub fn next_session(profile: &GymProfile, workouts: &[Workout]) -> (&'static str, Vec<Exercise>) {
    let t = template(&profile.split);
    let last = workouts.iter().max_by_key(|w| w.date).map(|w| plain(&w.session));
    let i = last.and_then(|s| t.iter().position(|(name, _)| plain(name) == s || plain(name).starts_with(&s) && !s.is_empty())).map(|i| (i + 1) % t.len()).unwrap_or(0);
    t[i].clone()
}

/// What to do on one exercise next time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub exercise: String,
    pub group: String,
    pub sets: u32,
    pub reps: (u32, u32),
    /// The last time: its sets as "reps × kg".
    pub last: Vec<String>,
    pub kg: Option<f64>,
    pub advice: String,
}

fn last_of<'a>(workouts: &'a [Workout], exercise: &str) -> Option<&'a ExerciseLog> {
    let n = plain(exercise);
    let mut sorted: Vec<&Workout> = workouts.iter().collect();
    sorted.sort_by_key(|w| std::cmp::Reverse(w.date));
    sorted.into_iter().find_map(|w| w.exercises.iter().find(|e| plain(&e.name) == n))
}

/// Double progression: all sets at the top of the range at the same weight → more weight (2.5 kg upper body, 5 kg
/// lower); any set under the bottom → the same weight; else the same weight and one more rep.
pub fn targets(session: &[Exercise], workouts: &[Workout]) -> Vec<Target> {
    session
        .iter()
        .map(|(name, group, sets, (lo, hi))| {
            let last = last_of(workouts, name);
            let (kg, advice) = match last {
                None => (None, "First time: pick a weight you could lift 2 more times at the end of each set.".to_string()),
                Some(l) => {
                    let heaviest = l.sets.iter().map(|s| s.kg).fold(0.0, f64::max);
                    let at_top = !l.sets.is_empty() && l.sets.iter().all(|s| s.reps >= *hi && (s.kg - heaviest).abs() < 0.01);
                    let under = l.sets.iter().any(|s| s.reps < *lo);
                    if at_top {
                        let step = if lower_body(group) { 5.0 } else { 2.5 };
                        (Some(heaviest + step), format!("All sets reached {hi}: go up to {} kg and aim for {lo}+ reps.", grouped(heaviest + step, 1)))
                    } else if under {
                        (Some(heaviest), format!("Keep {} kg until every set reaches {lo} reps.", grouped(heaviest, 1)))
                    } else {
                        (Some(heaviest), format!("Keep {} kg and add a rep where you can (up to {hi}).", grouped(heaviest, 1)))
                    }
                }
            };
            Target {
                exercise: name.to_string(),
                group: group.to_string(),
                sets: *sets,
                reps: (*lo, *hi),
                last: last.map(|l| l.sets.iter().map(|s| format!("{}×{}kg", s.reps, grouped(s.kg, 1))).collect()).unwrap_or_default(),
                kg,
                advice,
            }
        })
        .collect()
}

/// Sets per muscle group over the seven days up to `today`.
pub fn weekly_sets(workouts: &[Workout], today: Date) -> BTreeMap<String, u32> {
    let mut out = BTreeMap::new();
    for w in workouts.iter().filter(|w| w.date > today.add_days(-7) && w.date <= today) {
        for e in &w.exercises {
            *out.entry(group_of(&e.name).to_string()).or_default() += e.sets.len() as u32;
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Readiness {
    Normal,
    Light,
    Rest,
}

/// The day's readiness, from the last wellness note (of today or yesterday) and the hours of study planned.
pub fn readiness(wellness: &[Wellness], today: Date, study_hours: f64) -> (Readiness, Vec<String>) {
    let w = wellness.iter().filter(|w| w.date >= today.add_days(-1) && w.date <= today).max_by_key(|w| w.date);
    let mut score = 0i32;
    let mut why = Vec::new();
    if let Some(w) = w {
        if let Some(s) = w.sleep_hours {
            if s < 5.5 {
                score += 2;
                why.push(format!("slept {}h", grouped(s, 1)));
            } else if s < 6.5 {
                score += 1;
                why.push(format!("slept {}h", grouped(s, 1)));
            }
        }
        match w.soreness {
            Some(3) => {
                score += 2;
                why.push("very sore".into());
            }
            Some(2) => {
                score += 1;
                why.push("sore".into());
            }
            _ => {}
        }
        match w.energy {
            Some(1) => {
                score += 2;
                why.push("drained".into());
            }
            Some(2) => {
                score += 1;
                why.push("low energy".into());
            }
            _ => {}
        }
    }
    if study_hours >= 4.0 {
        score += 1;
        why.push(format!("{}h of study planned", grouped(study_hours, 1)));
    }
    let r = match score {
        0..=1 => Readiness::Normal,
        2..=3 => Readiness::Light,
        _ => Readiness::Rest,
    };
    (r, why)
}

/// Sessions in the last 7 days, and how many weeks in a row had at least the planned number.
pub fn consistency(profile: &GymProfile, workouts: &[Workout], today: Date) -> (usize, u32) {
    let this_week = workouts.iter().filter(|w| w.date > today.add_days(-7) && w.date <= today).count();
    let mut weeks = 0;
    loop {
        let end = today.add_days(-7 * weeks as i64);
        let n = workouts.iter().filter(|w| w.date > end.add_days(-7) && w.date <= end).count();
        if n >= profile.days_per_week.max(1) as usize && weeks < 52 {
            weeks += 1;
        } else {
            break;
        }
    }
    (this_week, weeks)
}

pub fn for_model(profile: &GymProfile, workouts: &[Workout], wellness: &[Wellness], today: Date, study_hours: f64) -> String {
    let (name, session) = next_session(profile, workouts);
    let (ready, why) = readiness(wellness, today, study_hours);
    let (week, streak) = consistency(profile, workouts, today);
    let mut out = format!(
        "Training: {} days a week, split {}, goal {}. This week {} session(s); {} week(s) in a row on plan.",
        profile.days_per_week, profile.split, profile.goal, week, streak
    );
    out.push_str(&format!(
        "\nReadiness today: {}{}.",
        match ready {
            Readiness::Normal => "normal",
            Readiness::Light => "train light (technique, fewer sets, leave 3+ reps in reserve)",
            Readiness::Rest => "rest or walk today",
        },
        if why.is_empty() { String::new() } else { format!(" ({})", why.join(", ")) }
    ));
    if streak >= 6 {
        out.push_str(" Six weeks or more on plan: a lighter (deload) week now helps the next ones.");
    }
    out.push_str(&format!("\nNext session: {name}"));
    for t in targets(&session, workouts) {
        out.push_str(&format!(
            "\n- {} ({}): {} sets of {}–{}{}. {}",
            t.exercise,
            t.group,
            t.sets,
            t.reps.0,
            t.reps.1,
            if t.last.is_empty() { String::new() } else { format!(", last time {}", t.last.join(", ")) },
            t.advice
        ));
    }
    let sets = weekly_sets(workouts, today);
    if !sets.is_empty() {
        let low: Vec<String> = sets.iter().filter(|(_, n)| **n < 10).map(|(g, n)| format!("{g} {n}")).collect();
        out.push_str(&format!("\nSets in the last 7 days: {}.", sets.iter().map(|(g, n)| format!("{g} {n}")).collect::<Vec<_>>().join(", ")));
        if !low.is_empty() {
            out.push_str(&format!(" Under 10 a week: {}.", low.join(", ")));
        }
    }
    out.push_str("\nLog a workout with life_gym (action log) once the user tells how it went; their sleep and energy with action wellness.");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SetLog;

    fn d(s: &str) -> Date {
        Date::parse(s).unwrap()
    }

    fn workout(date: &str, session: &str, ex: &[(&str, &[(u32, f64)])]) -> Workout {
        Workout {
            date: d(date),
            session: session.into(),
            exercises: ex.iter().map(|(n, sets)| ExerciseLog { name: n.to_string(), sets: sets.iter().map(|(r, k)| SetLog { reps: *r, kg: *k }).collect() }).collect(),
            minutes: 60,
            effort: Some(8),
            note: String::new(),
        }
    }

    #[test]
    fn the_rotation_and_double_progression() {
        let p = GymProfile::default();
        let ws = vec![
            workout("2026-10-19", "Upper A", &[("Supino reto", &[(10, 60.0), (10, 60.0), (10, 60.0)]), ("Remada curvada", &[(9, 50.0), (7, 50.0), (6, 50.0)])]),
            workout("2026-10-20", "Lower A", &[("Agachamento livre", &[(10, 80.0), (10, 80.0), (10, 80.0)]), ("Stiff", &[(8, 60.0)])]),
        ];
        let (name, session) = next_session(&p, &ws);
        assert_eq!(name, "Upper B");
        let (name_a, session_a) = next_session(&p, &ws[..1]);
        assert_eq!(name_a, "Lower A");
        let lower = targets(&session_a, &ws);
        let squat = lower.iter().find(|t| t.exercise == "Agachamento livre").unwrap();
        assert_eq!(squat.kg, Some(85.0), "lower body: +5 kg");
        let upper_a = targets(&template("upper_lower")[0].1, &ws);
        let bench = &upper_a[0];
        assert_eq!(bench.kg, Some(62.5), "upper body: +2.5 kg");
        let row = &upper_a[1];
        assert_eq!(row.kg, Some(50.0));
        assert!(row.advice.contains("until every set reaches 8"));
        assert!(targets(&session, &[])[0].advice.contains("First time"));
        let sets = weekly_sets(&ws, d("2026-10-21"));
        assert_eq!(sets["peito"], 3);
        assert_eq!(sets["pernas"], 3);
        assert_eq!(group_of("Supino declinado"), "peito");
        assert_eq!(group_of("Agachamento búlgaro"), "pernas");
    }

    #[test]
    fn readiness_and_consistency() {
        let today = d("2026-10-21");
        let good = vec![Wellness { date: today, sleep_hours: Some(8.0), soreness: Some(0), energy: Some(4) }];
        assert_eq!(readiness(&good, today, 2.0).0, Readiness::Normal);
        let tired = vec![Wellness { date: today, sleep_hours: Some(5.0), soreness: Some(2), energy: Some(2) }];
        let (r, why) = readiness(&tired, today, 5.0);
        assert_eq!(r, Readiness::Rest);
        assert_eq!(why.len(), 4);
        let meh = vec![Wellness { date: today.add_days(-1), sleep_hours: Some(6.0), soreness: None, energy: Some(2) }];
        assert_eq!(readiness(&meh, today, 0.0).0, Readiness::Light);
        let p = GymProfile { days_per_week: 2, ..GymProfile::default() };
        let ws: Vec<Workout> = (0..14).filter(|i| i % 3 == 0).map(|i| workout(&today.add_days(-i).to_string(), "Upper A", &[])).collect();
        let (week, streak) = consistency(&p, &ws, today);
        assert_eq!(week, 3);
        assert_eq!(streak, 2);
        let text = for_model(&p, &ws, &tired, today, 5.0);
        assert!(text.contains("rest or walk") && text.contains("Next session"), "{text}");
    }
}
