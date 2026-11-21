//! Player names. PGN sources spell names differently ("Carlsen, Magnus",
//! "Carlsen,M", "Magnus Carlsen"), so games are linked to players by a
//! normalised key: the surname plus the first initial, e.g. "carlsen m".

/// Remove accents from the common Latin letters.
fn fold(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ą' => 'a',
        'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => 'e',
        'í' | 'ì' | 'î' | 'ï' | 'ī' => 'i',
        'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ō' | 'ő' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => 'u',
        'ñ' | 'ń' | 'ň' => 'n',
        'ç' | 'č' | 'ć' => 'c',
        'š' | 'ś' | 'ş' => 's',
        'ž' | 'ź' | 'ż' => 'z',
        'ř' => 'r',
        'ł' => 'l',
        'ý' | 'ÿ' => 'y',
        'đ' | 'ď' => 'd',
        'ť' => 't',
        'ğ' => 'g',
        other => other,
    }
}

/// Name particles that some sources put first and others leave out.
const PARTICLES: &[&str] = &["von", "van", "de", "der", "den", "da", "di", "du", "le", "la", "del"];
/// Titles sometimes written into names.
const TITLES: &[&str] = &["gm", "im", "fm", "wgm", "wim", "cm", "dr", "prof", "sir", "mr", "mrs"];

fn words(s: &str) -> Vec<String> {
    s.split(|c: char| c.is_whitespace() || c == '.' || c == '_')
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '-'))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

fn surname(words: &[String]) -> String {
    let w: Vec<&str> = words.iter().map(String::as_str).filter(|w| !PARTICLES.contains(w)).collect();
    w.join(" ")
}

/// "carlsen m" for "Carlsen, Magnus", "Carlsen,M" and "Magnus Carlsen".
pub fn player_key(name: &str) -> String {
    let lower: String = name.to_lowercase().chars().map(fold).collect();
    // Drop "(NOR)", ratings and similar bracketed extras.
    let cleaned: String = {
        let mut out = String::new();
        let mut depth = 0;
        for c in lower.chars() {
            match c {
                '(' | '[' => depth += 1,
                ')' | ']' => depth = (depth - 1).max(0),
                _ if depth == 0 => out.push(c),
                _ => {}
            }
        }
        out
    };
    let (last, first): (Vec<String>, Vec<String>) = match cleaned.split_once(',') {
        Some((last, first)) => (words(last), words(first)),
        None => {
            let mut w: Vec<String> = words(&cleaned).into_iter().filter(|w| !TITLES.contains(&w.as_str())).collect();
            if w.len() <= 1 {
                return w.pop().unwrap_or_default();
            }
            // "Carlsen M": a trailing initial means the surname came first.
            if w.last().is_some_and(|l| l.chars().count() == 1) {
                let initial = w.pop().unwrap_or_default();
                return format!("{} {initial}", surname(&w));
            }
            // Particles before the surname belong to it: "Curt von Bardeleben".
            let mut split = w.len() - 1;
            while split > 1 && PARTICLES.contains(&w[split - 1].as_str()) {
                split -= 1;
            }
            let last = w.split_off(split);
            (last, w)
        }
    };
    let last: Vec<String> = last.into_iter().filter(|w| !TITLES.contains(&w.as_str())).collect();
    let surname = surname(&last);
    match first.iter().find(|w| !TITLES.contains(&w.as_str())).and_then(|f| f.chars().next()) {
        Some(initial) if !surname.is_empty() => format!("{surname} {initial}"),
        _ => surname,
    }
}

/// The surname part of a key ("carlsen" for "carlsen m").
pub fn key_surname(key: &str) -> &str {
    match key.rsplit_once(' ') {
        Some((s, initial)) if initial.chars().count() == 1 => s,
        _ => key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spellings_of_one_player_share_a_key() {
        for name in ["Carlsen, Magnus", "Carlsen,M", "Magnus Carlsen", "GM Magnus Carlsen", "Carlsen, Magnus (NOR)", "carlsen m."] {
            assert_eq!(player_key(name), "carlsen m", "{name}");
        }
        assert_eq!(player_key("José Raúl Capablanca"), "capablanca j");
        assert_eq!(player_key("Capablanca, Jose Raul"), "capablanca j");
        assert_eq!(player_key("Von Bardeleben, Curt"), "bardeleben c");
        assert_eq!(player_key("Curt von Bardeleben"), "bardeleben c");
        assert_eq!(player_key("Polgar,Ju"), "polgar j");
        assert_ne!(player_key("Polgar, Susan"), player_key("Polgar, Judit"));
        assert_eq!(player_key("Stockfish"), "stockfish");
        assert_eq!(player_key(""), "");
        assert_eq!(key_surname("carlsen m"), "carlsen");
        assert_eq!(key_surname("stockfish"), "stockfish");
    }
}
