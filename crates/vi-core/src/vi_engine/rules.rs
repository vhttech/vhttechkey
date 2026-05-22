//! `rules_parser.go` — parse Telex/VNI/VIQR DSL into [`crate::vi_engine::types::Rule`] lists.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

use crate::vi_engine::text::{
    add_tone_to_char, find_mark_from_char, get_mark_family, is_vowel,
};
use crate::vi_engine::types::{
    EffectType, Mark, ParsedInputMethod, Rule, Tone,
};

static REG_DSL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([a-zA-Z]+)_(\p{L}+)([_\p{L}]*)").expect("REG_DSL"));
static REG_DSL_APPENDING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(_?)_(\p{L}+)").expect("REG_DSL_APPENDING"));

fn tone_from_line(line: &str) -> Option<Tone> {
    Some(match line {
        "XoaDauThanh" => Tone::None,
        "DauSac" => Tone::Acute,
        "DauHuyen" => Tone::Grave,
        "DauNga" => Tone::Tilde,
        "DauNang" => Tone::Dot,
        "DauHoi" => Tone::Hook,
        _ => return None,
    })
}

pub(crate) fn parse_rules(key: char, line: &str) -> Vec<Rule> {
    if let Some(tone) = tone_from_line(line) {
        let mut rule = Rule {
            key,
            effect: tone as u8,
            effect_type: EffectType::ToneTransformation,
            effect_on: '\0',
            result: '\0',
            appended_rules: Vec::new(),
        };
        if tone == Tone::None {
            rule.effect = 0;
        }
        return vec![rule];
    }
    parse_toneless_rules(key, line)
}

fn parse_toneless_rules(key: char, line: &str) -> Vec<Rule> {
    let mut rules = Vec::new();
    let lower = line.to_lowercase();
    if let Some(caps) = REG_DSL.captures(&lower) {
        let effective_ons: Vec<char> = caps.get(1).unwrap().as_str().chars().collect();
        let results: Vec<char> = caps.get(2).unwrap().as_str().chars().collect();
        for (i, &effective_on) in effective_ons.iter().enumerate() {
            if i >= results.len() {
                break;
            }
            let Some(effect) = find_mark_from_char(results[i]) else {
                continue;
            };
            rules.extend(parse_tone_less_rule(key, effective_on, results[i], effect));
        }
        let tail = caps.get(3).map(|m| m.as_str()).unwrap_or("");
        if let Some(rule) = get_appending_rule(key, tail) {
            rules.push(rule);
        }
    } else if let Some(rule) = get_appending_rule(key, line) {
        rules.push(rule);
    }
    rules
}

fn parse_tone_less_rule(key: char, effective_on: char, result: char, effect: Mark) -> Vec<Rule> {
    let mut rules = Vec::new();
    let tones = [
        Tone::None,
        Tone::Dot,
        Tone::Acute,
        Tone::Grave,
        Tone::Hook,
        Tone::Tilde,
    ];
    for chr in get_mark_family(effective_on) {
        if chr == result {
            rules.push(Rule {
                key,
                effect_type: EffectType::MarkTransformation,
                effect: 0,
                effect_on: result,
                result: effective_on,
                appended_rules: Vec::new(),
            });
        } else if is_vowel(chr) {
            for tone in tones {
                rules.push(Rule {
                    key,
                    effect_type: EffectType::MarkTransformation,
                    effect_on: add_tone_to_char(chr, tone as u8),
                    effect: effect as u8,
                    result: add_tone_to_char(result, tone as u8),
                    appended_rules: Vec::new(),
                });
            }
        } else {
            rules.push(Rule {
                key,
                effect_type: EffectType::MarkTransformation,
                effect_on: chr,
                effect: effect as u8,
                result,
                appended_rules: Vec::new(),
            });
        }
    }
    rules
}

fn get_appending_rule(key: char, value: &str) -> Option<Rule> {
    let caps = REG_DSL_APPENDING.captures(value)?;
    let chars: Vec<char> = caps.get(2)?.as_str().chars().collect();
    if chars.is_empty() {
        return None;
    }
    let mut rule = Rule {
        key,
        effect_type: EffectType::Appending,
        effect_on: chars[0],
        result: chars[0],
        effect: 0,
        appended_rules: Vec::new(),
    };
    for &chr in chars.iter().skip(1) {
        rule.appended_rules.push(Rule {
            key,
            effect_type: EffectType::Appending,
            effect_on: chr,
            result: chr,
            effect: 0,
            appended_rules: Vec::new(),
        });
    }
    Some(rule)
}

/// vhttechkey `InputMethodDefinitions` (Unicode path; bỏ charset table).
pub(crate) fn input_method_definitions() -> HashMap<String, HashMap<String, String>> {
    [
        ("Telex", vec![
            ("z", "XoaDauThanh"), ("s", "DauSac"), ("f", "DauHuyen"), ("r", "DauHoi"),
            ("x", "DauNga"), ("j", "DauNang"), ("a", "A_Â"), ("e", "E_Ê"),
            ("o", "O_Ô"), ("w", "UOA_ƯƠĂ"), ("d", "D_Đ"),
        ]),
        ("VNI", vec![
            ("0", "XoaDauThanh"), ("1", "DauSac"), ("2", "DauHuyen"), ("3", "DauHoi"),
            ("4", "DauNga"), ("5", "DauNang"), ("6", "AEO_ÂÊÔ"), ("7", "UO_ƯƠ"),
            ("8", "A_Ă"), ("9", "D_Đ"),
        ]),
        ("VIQR", vec![
            ("0", "XoaDauThanh"), ("'", "DauSac"), ("`", "DauHuyen"), ("?", "DauHoi"),
            ("~", "DauNga"), (".", "DauNang"), ("^", "AEO_ÂÊÔ"), ("+", "UO_ƯƠ"),
            ("*", "UO_ƯƠ"), ("(", "A_Ă"), ("d", "D_Đ"),
        ]),
        ("Telex 2", vec![
            ("z", "XoaDauThanh"), ("s", "DauSac"), ("f", "DauHuyen"), ("r", "DauHoi"),
            ("x", "DauNga"), ("j", "DauNang"), ("a", "A_Â"), ("e", "E_Ê"),
            ("o", "O_Ô"), ("w", "UOA_ƯƠĂ__Ư"), ("d", "D_Đ"),
            ("]", "__ư"), ("[", "__ơ"), ("}", "_Ư"), ("{", "_Ơ"),
        ]),
    ]
    .into_iter()
    .map(|(n, pairs)| {
        (
            n.to_string(),
            pairs
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    })
    .collect()
}

pub(crate) fn parse_input_methods(
    im_def: &HashMap<String, HashMap<String, String>>,
) -> HashMap<String, ParsedInputMethod> {
    let mut out = HashMap::new();
    for (name, def) in im_def {
        let mut im = ParsedInputMethod {
            name: name.clone(),
            rules: Vec::new(),
            super_keys: Vec::new(),
            tone_keys: Vec::new(),
            appending_keys: Vec::new(),
            keys: Vec::new(),
        };
        for (key_str, line) in def {
            let mut key_chars = key_str.chars();
            let Some(key) = key_chars.next() else {
                continue;
            };
            im.rules.extend(parse_rules(key, line));
            if line.to_lowercase().contains("uo") {
                im.super_keys.push(key);
            }
            im.keys.push(key);
        }
        im.keys.sort_unstable();
        im.keys.dedup();
        for rule in &im.rules {
            if rule.effect_type == EffectType::Appending {
                im.appending_keys.push(rule.key);
            }
            if rule.effect_type == EffectType::ToneTransformation {
                im.tone_keys.push(rule.key);
            }
        }
        out.insert(name.clone(), im);
    }
    out
}

pub(crate) fn parsed_telex() -> ParsedInputMethod {
    let defs = input_method_definitions();
    let mut ims = parse_input_methods(&defs);
    ims.remove("Telex").expect("Telex definition missing")
}

pub(crate) fn parsed_vni() -> ParsedInputMethod {
    let defs = input_method_definitions();
    let mut ims = parse_input_methods(&defs);
    ims.remove("VNI").expect("VNI definition missing")
}

pub(crate) fn parsed_viqr() -> ParsedInputMethod {
    let defs = input_method_definitions();
    let mut ims = parse_input_methods(&defs);
    ims.remove("VIQR").expect("VIQR definition missing")
}
