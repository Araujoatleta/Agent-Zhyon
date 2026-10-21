// What the user's own message asks of Zheus One, read by Coucou itself, never by a model: "I'm on my way home, I'll be
// there in 25 minutes", "show our situation", "I'm home", and the answer to a plan waiting ("yes", "cancel", "no
// cleaning"). Words of the interface's languages, case and accents aside. The message still goes to the assistant,
// with what Zheus did added, so the rest of it ("…and tell me how the day went") is answered.

use crate::smart_home::model::normalize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// On the way home; the minutes, when the message says them.
    Arrival { eta: Option<u32> },
    /// The situation on the TV.
    Situation,
    /// The user is home.
    Arrived,
    /// Yes to the plan waiting; with a way around its conflict, when the message picks one.
    Approve { choice: Option<&'static str> },
    /// No to the plan waiting, or stop the one running.
    Cancel,
}

const ARRIVAL: &[&str] = &[
    // Português
    "indo para casa", "indo pra casa", "a caminho de casa", "voltando para casa", "voltando pra casa", "chegando em casa",
    "prepara minha chegada", "preparar minha chegada", "prepare minha chegada", "saindo do trabalho",
    // English
    "on my way home", "heading home", "coming home", "going home", "prepare my arrival", "leaving work",
    // Español
    "voy a casa", "camino a casa", "volviendo a casa", "llegando a casa", "prepara mi llegada", "saliendo del trabajo",
    // Français
    "je rentre", "en route pour la maison", "prepare mon arrivee", "je quitte le travail",
    // Bahasa Indonesia
    "pulang ke rumah", "dalam perjalanan pulang",
];

const SITUATION: &[&str] = &[
    "nossa situacao", "mostra a situacao", "mostrar a situacao", "modo tv", "como estamos",
    "our situation", "show the situation", "tv mode", "where we stand",
    "nuestra situacion", "muestra la situacion", "modo tv",
    "notre situation", "montre la situation", "mode tv",
    "situasi kita",
];

const ARRIVED: &[&str] = &[
    "cheguei", "ja cheguei", "estou em casa", "acabei de chegar",
    "i m home", "im home", "i am home", "i just got home",
    "ya llegue", "llegue a casa", "estoy en casa",
    "je suis rentre", "je suis rentree", "je suis a la maison",
    "sudah sampai rumah", "saya sudah di rumah",
];

/// Yes to a plan waiting. Only words that say yes on their own: not "vai" or "isso", which a question may hold.
const YES: &[&str] = &[
    "sim", "pode", "pode sim", "pode ir", "manda ver", "ok", "okay", "beleza", "fechado", "bora", "aprovo", "aprova",
    "confirmo", "confirma", "perfeito", "yes", "yep", "sure", "go ahead", "do it", "approve", "si", "dale", "vale",
    "adelante", "oui", "d accord", "vas y", "boleh", "lanjut", "iya",
];

/// No to a plan waiting: words that only ever cancel. Not "para", "no" or "deixa", which also mean "for", "in the" and
/// "leave it at": those count only as the whole message (NO_ALONE).
const NO: &[&str] = &[
    "cancela", "cancelar", "cancel", "deixa pra la", "esquece", "nao quero", "never mind", "cancela eso", "olvidalo",
    "annule", "laisse tomber", "batal", "jangan",
];
const NO_ALONE: &[&str] = &["nao", "no", "non", "nope", "tidak", "nao obrigado", "no thanks", "no gracias", "non merci"];

/// "no cleaning" / "send it back to the dock" / "only that room" in an answer.
const SKIP_CLEANING: &[&str] = &["sem limpeza", "sem limpar", "nao limpa", "nao precisa limpar", "no cleaning", "dont clean", "sin limpieza", "no limpies", "pas de menage", "sans menage"];
const DOCK: &[&str] = &["volta pra base", "volta para a base", "voltar pra base", "voltar para a base", "back to the dock", "back to its dock", "a la base", "vuelva a la base", "retour a la base"];
const ONLY_ROOMS: &[&str] = &["so o", "so a", "apenas o", "apenas a", "somente o", "somente a", "only the", "solo el", "solo la", "seulement"];

/// `phrase` appears in `text` as whole words (both normalized).
fn has(text: &str, phrase: &str) -> bool {
    format!(" {text} ").contains(&format!(" {phrase} "))
}

fn any(text: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|p| has(text, p))
}

/// The minutes a message gives for the way home: "25 minutos", "em 20 min", "25min", "meia hora", "1 hora", "an hour".
pub fn minutes_in(text: &str) -> Option<u32> {
    let words: Vec<&str> = text.split(' ').collect();
    let number = |w: &str| -> Option<u32> {
        match w {
            // Not "a": "a hora do jantar" is no hour away.
            "um" | "uma" | "one" | "an" | "un" | "una" | "une" | "satu" => Some(1),
            "dois" | "duas" | "two" | "dos" | "deux" | "dua" => Some(2),
            _ => w.parse().ok(),
        }
    };
    for (i, w) in words.iter().enumerate() {
        let next = words.get(i + 1).copied().unwrap_or("");
        // "25min", "25m"
        if let Some(digits) = w.strip_suffix("min").or_else(|| w.strip_suffix('m')) {
            if let Ok(n) = digits.parse::<u32>() {
                return Some(n);
            }
        }
        if let Some(n) = number(w) {
            if next.starts_with("min") || next == "mnt" || next == "menit" {
                return Some(n);
            }
            if next.starts_with("hora") || next.starts_with("hour") || next.starts_with("heure") || next == "jam" {
                // "1 hora e meia", "an hour and a half"
                let half = words.iter().skip(i + 2).take(3).any(|x| *x == "meia" || *x == "half" || *x == "media" || *x == "demie");
                return Some(n * 60 + if half { 30 } else { 0 });
            }
        }
        if (*w == "meia" && next == "hora") || (*w == "half" && next == "an" && words.get(i + 2) == Some(&"hour")) || (*w == "media" && next == "hora") || (*w == "demi" && next == "heure") {
            return Some(30);
        }
    }
    None
}

/// What the message asks of Zheus One. `waiting`: a plan is waiting for an answer (only then "yes" and "no" count, and
/// only as a short answer).
pub fn intent(message: &str, waiting: bool) -> Option<Intent> {
    let text = normalize(message);
    if text.is_empty() {
        return None;
    }
    if waiting {
        let short = text.split(' ').count() <= 12;
        if NO_ALONE.contains(&text.as_str()) || (short && any(&text, NO) && !any(&text, YES)) {
            return Some(Intent::Cancel);
        }
        if short && (any(&text, YES) || any(&text, SKIP_CLEANING) || any(&text, DOCK) || any(&text, ONLY_ROOMS)) {
            let choice = if any(&text, SKIP_CLEANING) {
                Some("skip")
            } else if any(&text, DOCK) {
                Some("dock")
            } else if any(&text, ONLY_ROOMS) {
                Some("rooms")
            } else {
                None
            };
            return Some(Intent::Approve { choice });
        }
    }
    if any(&text, ARRIVAL) {
        return Some(Intent::Arrival { eta: minutes_in(&text).filter(|m| (1..=240).contains(m)) });
    }
    if any(&text, SITUATION) {
        return Some(Intent::Situation);
    }
    if any(&text, ARRIVED) {
        return Some(Intent::Arrived);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_way_home_with_its_minutes_in_several_languages() {
        assert_eq!(intent("Zheus, estou indo para casa. Deixa tudo pronto e me conta o que aconteceu hoje.", false), Some(Intent::Arrival { eta: None }));
        assert_eq!(intent("Tô indo pra casa, chego em 25 minutos", false), Some(Intent::Arrival { eta: Some(25) }));
        assert_eq!(intent("a caminho de casa, uns 40min", false), Some(Intent::Arrival { eta: Some(40) }));
        assert_eq!(intent("Voltando para casa, chego em meia hora!", false), Some(Intent::Arrival { eta: Some(30) }));
        assert_eq!(intent("Saindo do trabalho, 1 hora e meia de trânsito", false), Some(Intent::Arrival { eta: Some(90) }));
        assert_eq!(intent("On my way home, about an hour", false), Some(Intent::Arrival { eta: Some(60) }));
        assert_eq!(intent("heading home, 15 min", false), Some(Intent::Arrival { eta: Some(15) }));
        assert_eq!(intent("Voy a casa, llego en 20 minutos", false), Some(Intent::Arrival { eta: Some(20) }));
        assert_eq!(intent("Je rentre dans une heure", false), Some(Intent::Arrival { eta: Some(60) }));
        assert_eq!(intent("indo pra casa, chego em 900 minutos", false), Some(Intent::Arrival { eta: None }), "beyond four hours is not a way home");
    }

    #[test]
    fn the_situation_and_being_home() {
        assert_eq!(intent("Zheus, mostra nossa situação.", false), Some(Intent::Situation));
        assert_eq!(intent("show me our situation", false), Some(Intent::Situation));
        assert_eq!(intent("Cheguei!", false), Some(Intent::Arrived));
        assert_eq!(intent("I'm home", false), Some(Intent::Arrived));
        assert_eq!(intent("What's the weather tomorrow?", false), None);
        assert_eq!(intent("A casa da minha mãe é longe", false), None, "\"casa\" alone is no mission");
        assert_eq!(intent("", true), None);
    }

    #[test]
    fn a_waiting_plan_takes_a_short_yes_or_no_and_a_way_around() {
        assert_eq!(intent("Pode, Zheus. E prepara meu quarto.", true), Some(Intent::Approve { choice: None }));
        assert_eq!(intent("sim", true), Some(Intent::Approve { choice: None }));
        assert_eq!(intent("ok, mas sem limpeza hoje", true), Some(Intent::Approve { choice: Some("skip") }));
        assert_eq!(intent("pode, e manda o robô voltar pra base antes", true), Some(Intent::Approve { choice: Some("dock") }));
        assert_eq!(intent("pode, só o quarto", true), Some(Intent::Approve { choice: Some("rooms") }));
        assert_eq!(intent("Não, cancela", true), Some(Intent::Cancel));
        assert_eq!(intent("cancel", true), Some(Intent::Cancel));
        assert_eq!(intent("Não.", true), Some(Intent::Cancel));
        // Words that also mean something else never cancel a plan.
        assert_eq!(intent("liga o ar para mim", true), None);
        assert_eq!(intent("deixa o ar em 22", true), None);
        assert_eq!(intent("vai chover hoje?", true), None);
        assert_eq!(intent("ok, não limpa hoje", true), Some(Intent::Approve { choice: Some("skip") }));
        // Without a plan waiting, "yes" is just a word for the assistant.
        assert_eq!(intent("sim", false), None);
        // A long message is a new request, not an answer.
        assert_eq!(intent("sim, e também me lembra de pagar a conta de luz amanhã de manhã e de ligar para o banco às dez", true), None);
    }

    #[test]
    fn minutes_are_read_in_words_and_digits() {
        assert_eq!(minutes_in("chego em 25 minutos"), Some(25));
        assert_eq!(minutes_in("10m"), Some(10));
        assert_eq!(minutes_in("uma hora"), Some(60));
        assert_eq!(minutes_in("2 horas"), Some(120));
        assert_eq!(minutes_in("half an hour"), Some(30));
        assert_eq!(minutes_in("sem pressa"), None);
        assert_eq!(minutes_in("chego a hora do jantar"), None);
    }
}
