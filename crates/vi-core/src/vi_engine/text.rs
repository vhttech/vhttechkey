//! Unicode helpers for composition engine.

use crate::vi_engine::types::{Mark, Tone};

pub(crate) const VOWELS_STR: &str =
    "aàáảãạăằắẳẵặâầấẩẫậeèéẻẽẹêềếểễệiìíỉĩịoòóỏõọôồốổỗộơờớởỡợuùúủũụưừứửữựyỳýỷỹỵ";

fn vowels_slice() -> &'static [char] {
    static V: std::sync::OnceLock<Vec<char>> = std::sync::OnceLock::new();
    V.get_or_init(|| VOWELS_STR.chars().collect()).as_slice()
}

pub(crate) fn is_punctuation_mark(key: char) -> bool {
    matches!(
        key,
        ',' | ';' | ':' | '.' | '"' | '\'' | '!' | '?' | ' ' | '<' | '>' | '='
            | '+' | '-' | '*' | '/' | '\\' | '_' | '~' | '`' | '@' | '#' | '$'
            | '%' | '^' | '&' | '(' | ')' | '{' | '}' | '[' | ']' | '|'
    )
}

pub(crate) fn is_word_break_symbol(key: char) -> bool {
    is_punctuation_mark(key) || key.is_ascii_digit()
}

pub(crate) fn is_vowel(chr: char) -> bool {
    vowels_slice().contains(&chr)
}

pub(crate) fn find_vowel_position(chr: char) -> Option<usize> {
    vowels_slice().iter().position(|&v| v == chr)
}

fn marks_row_for_char(chr: char) -> Option<&'static str> {
    Some(match chr {
        'a' | 'â' | 'ă' => "aâă__",
        'e' | 'ê' => "eê___",
        'o' | 'ô' | 'ơ' => "oô_ơ_",
        'u' | 'ư' => "u__ư_",
        'd' | 'đ' => "d___đ",
        _ => return None,
    })
}

pub(crate) fn get_mark_family(chr: char) -> Vec<char> {
    let Some(row) = marks_row_for_char(chr) else {
        return Vec::new();
    };
    row.chars().filter(|&c| c != '_').collect()
}

pub(crate) fn find_mark_position(chr: char) -> Option<usize> {
    let row = marks_row_for_char(chr)?;
    row.chars().position(|c| c == chr)
}

pub(crate) fn find_mark_from_char(chr: char) -> Option<Mark> {
    let pos = find_mark_position(chr)?;
    Some(match pos {
        0 => Mark::None,
        1 => Mark::Hat,
        2 => Mark::Breve,
        3 => Mark::Horn,
        _ => Mark::Dash,
    })
}

pub(crate) fn add_mark_to_toneless_char(chr: char, mark: u8) -> char {
    let Some(row) = marks_row_for_char(chr) else {
        return chr;
    };
    let marks: Vec<char> = row.chars().collect();
    let i = mark as usize;
    if i < marks.len() && marks[i] != '_' {
        marks[i]
    } else {
        chr
    }
}

pub(crate) fn find_tone_from_char(chr: char) -> Tone {
    let Some(pos) = find_vowel_position(chr) else {
        return Tone::None;
    };
    Tone::from((pos % 6) as u8)
}

pub(crate) fn add_tone_to_char(chr: char, tone: u8) -> char {
    let Some(pos) = find_vowel_position(chr) else {
        return chr;
    };
    let current_tone = (pos % 6) as i32;
    let v = vowels_slice();
    let offset = tone as i32 - current_tone;
    let ni = pos as i32 + offset;
    if ni >= 0 && (ni as usize) < v.len() {
        v[ni as usize]
    } else {
        chr
    }
}

pub(crate) fn add_mark_to_char(chr: char, mark: u8) -> char {
    let tone = find_tone_from_char(chr);
    let mut x = add_tone_to_char(chr, 0);
    x = add_mark_to_toneless_char(x, mark);
    add_tone_to_char(x, tone as u8)
}

pub(crate) fn is_alpha(c: char) -> bool {
    c.is_ascii_alphabetic()
}

pub(crate) fn in_key_list(keys: &[char], key: char) -> bool {
    keys.iter().any(|&k| k == key)
}

pub(crate) fn is_vietnamese_rune(lower_key: char) -> bool {
    if find_tone_from_char(lower_key) != Tone::None {
        return true;
    }
    lower_key != add_mark_to_toneless_char(lower_key, 0)
}

pub(crate) fn can_process_key(lower_key: char, effect_keys: &[char]) -> bool {
    if is_alpha(lower_key) || in_key_list(effect_keys, lower_key) {
        return true;
    }
    if is_word_break_symbol(lower_key) {
        return false;
    }
    is_vietnamese_rune(lower_key)
}
