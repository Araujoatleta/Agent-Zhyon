//! Spaced repetition (SM-2, the algorithm behind Anki): each card comes back after an interval that grows when it is
//! remembered and starts over when it is not. One engine for the exam's cards and the English words.

use serde::{Deserialize, Serialize};

use crate::date::Date;
use crate::model::Card;

/// How well a card was remembered, from the user's words or a number 0–5: "errei"/"again" 1, "difícil"/"hard" 3,
/// "bom"/"good"/"acertei" 4, "fácil"/"easy" 5.
pub fn quality(word: &str) -> Option<u8> {
    let p = crate::text::plain(word);
    if let Ok(n) = p.trim().parse::<u8>() {
        return (n <= 5).then_some(n);
    }
    Some(match p.trim() {
        w if w.starts_with("again") || w.starts_with("err") || w.starts_with("esqueci") || w.starts_with("nao lembr") || w == "wrong" => 1,
        w if w.starts_with("hard") || w.starts_with("dificil") || w.starts_with("quase") => 3,
        w if w.starts_with("good") || w.starts_with("bom") || w.starts_with("acert") || w.starts_with("certo") || w == "ok" || w == "right" => 4,
        w if w.starts_with("easy") || w.starts_with("facil") || w.starts_with("moleza") => 5,
        _ => return None,
    })
}

pub fn new_card(id: String, deck: &str, front: &str, back: &str, tags: Vec<String>, today: Date) -> Card {
    Card {
        id,
        deck: deck.trim().to_lowercase(),
        front: front.trim().to_string(),
        back: back.trim().to_string(),
        tags,
        ease: 2.5,
        interval: 0,
        reps: 0,
        lapses: 0,
        due: today,
        created: today,
    }
}

/// SM-2: a card graded `q` (0–5) on `today`.
pub fn grade(card: &mut Card, q: u8, today: Date) {
    let q = q.min(5);
    if q < 3 {
        card.reps = 0;
        card.interval = 1;
        card.lapses += 1;
    } else {
        card.reps += 1;
        card.interval = match card.reps {
            1 => 1,
            2 => 6,
            _ => ((card.interval.max(1) as f64) * card.ease).round() as u32,
        };
    }
    let d = 5.0 - q as f64;
    card.ease = (card.ease + 0.1 - d * (0.08 + d * 0.02)).max(1.3);
    card.due = today.add_days(card.interval as i64);
}

/// The cards of `deck` (all decks when empty) due by `today`, the most overdue first, at most `limit`.
pub fn due<'a>(cards: &'a [Card], deck: &str, today: Date, limit: usize) -> Vec<&'a Card> {
    let deck = deck.trim().to_lowercase();
    let mut list: Vec<&Card> = cards.iter().filter(|c| (deck.is_empty() || c.deck == deck) && c.due <= today).collect();
    list.sort_by_key(|c| (c.due, c.reps));
    list.truncate(limit);
    list
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckStats {
    pub deck: String,
    pub cards: usize,
    pub due: usize,
    /// Cards seen at least once.
    pub learned: usize,
    /// Cards remembered for three weeks or more.
    pub mature: usize,
}

pub fn stats(cards: &[Card], today: Date) -> Vec<DeckStats> {
    let mut decks: Vec<String> = cards.iter().map(|c| c.deck.clone()).collect();
    decks.sort();
    decks.dedup();
    decks
        .into_iter()
        .map(|deck| {
            let mine: Vec<&Card> = cards.iter().filter(|c| c.deck == deck).collect();
            DeckStats {
                cards: mine.len(),
                due: mine.iter().filter(|c| c.due <= today).count(),
                learned: mine.iter().filter(|c| c.reps > 0 || c.lapses > 0).count(),
                mature: mine.iter().filter(|c| c.interval >= 21).count(),
                deck,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervals_grow_when_remembered_and_start_over_when_not() {
        let today = Date::new(2026, 10, 21).unwrap();
        let mut c = new_card("c1".into(), "Concurso", "Prazo do recurso administrativo (Lei 9.784)?", "10 dias", vec![], today);
        assert_eq!(c.deck, "concurso");
        grade(&mut c, 4, today);
        assert_eq!((c.reps, c.interval, c.due), (1, 1, today.add_days(1)));
        grade(&mut c, 4, today.add_days(1));
        assert_eq!(c.interval, 6);
        grade(&mut c, 5, today.add_days(7));
        assert_eq!(c.interval, 15, "6 days × an ease of 2.5");
        assert!((c.ease - 2.6).abs() < 1e-9, "an easy answer raises the ease");
        let before = c.ease;
        grade(&mut c, 1, today.add_days(30));
        assert_eq!((c.reps, c.interval, c.lapses), (0, 1, 1));
        assert!(c.ease < before && c.ease >= 1.3);
        assert_eq!(quality("acertei"), Some(4));
        assert_eq!(quality("Fácil"), Some(5));
        assert_eq!(quality("errei"), Some(1));
        assert_eq!(quality("3"), Some(3));
        assert_eq!(quality("talvez"), None);
    }

    #[test]
    fn due_cards_and_deck_stats() {
        let today = Date::new(2026, 10, 21).unwrap();
        let mut cards = vec![
            new_card("a".into(), "english", "to hedge", "proteger (uma posição)", vec![], today.add_days(-3)),
            new_card("b".into(), "english", "yield", "rendimento", vec![], today),
            new_card("c".into(), "concurso", "art. 37", "LIMPE", vec![], today.add_days(5)),
        ];
        cards[2].due = today.add_days(5);
        let d = due(&cards, "english", today, 10);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].id, "a", "the most overdue first");
        assert_eq!(due(&cards, "", today, 1).len(), 1);
        let s = stats(&cards, today);
        assert_eq!(s.len(), 2);
        let en = s.iter().find(|x| x.deck == "english").unwrap();
        assert_eq!((en.cards, en.due), (2, 2));
    }
}
