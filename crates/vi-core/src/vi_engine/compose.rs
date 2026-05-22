//! Composition transforms (targets, spell checks, undo, shortcuts).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use parking_lot::RwLock;
use regex::Regex;

use crate::vi_engine::flat;
use crate::vi_engine::spell::is_valid_cvc;
use crate::vi_engine::text::{find_tone_from_char, in_key_list, is_alpha, is_vowel};
use crate::vi_engine::types::{
    EffectType, Mark, Rule, Tone, Trans, TransInner, EFREE_TONE_MARKING, ENGLISH_MODE,
    ESTANDARD_TONE_STYLE, LOWERCASE_MODE, MARK_LESS, TONE_LESS, VIETNAMESE_MODE,
};

static REG_UOH_TAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(uơ|ưo)\p{L}+").expect("REG_UOH_TAIL"));
static REG_UH_O: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(ưo|ươ)").expect("REG_UH_O"));

pub(crate) fn new_trans(rule: Rule, target: Option<Trans>, is_upper_case: bool) -> Trans {
    Arc::new(RwLock::new(TransInner {
        rule,
        target,
        is_upper_case,
    }))
}

pub(crate) fn find_last_appending_trans(composition: &[Trans]) -> Option<Trans> {
    for trans in composition.iter().rev() {
        if trans.read().rule.effect_type == EffectType::Appending {
            return Some(trans.clone());
        }
    }
    None
}

pub(crate) fn new_appending_trans(key: char, is_upper_case: bool) -> Trans {
    new_trans(
        Rule {
            key,
            effect_on: key,
            effect_type: EffectType::Appending,
            result: key,
            effect: 0,
            appended_rules: Vec::new(),
        },
        None,
        is_upper_case,
    )
}

pub(crate) fn generate_appending_trans(
    rules: &[Rule],
    lower_key: char,
    is_upper_case: bool,
) -> Trans {
    for rule in rules {
        if rule.key == lower_key && rule.effect_type == EffectType::Appending {
            let mut r = rule.clone();
            let _is_upper_case = is_upper_case || r.effect_on.is_uppercase();
            r.effect_on = r.effect_on.to_lowercase().next().unwrap_or(r.effect_on);
            r.result = r.effect_on;
            return new_trans(r, None, _is_upper_case);
        }
    }
    new_appending_trans(lower_key, is_upper_case)
}

pub(crate) fn filter_appending_composition(composition: &[Trans]) -> Vec<Trans> {
    composition
        .iter()
        .filter(|t| t.read().rule.effect_type == EffectType::Appending)
        .cloned()
        .collect()
}

pub(crate) fn find_root_target(target: &Trans) -> Trans {
    let inner = target.read();
    match &inner.target {
        None => target.clone(),
        Some(t) => find_root_target(t),
    }
}

pub(crate) fn is_valid(composition: &[Trans], input_is_full_complete: bool) -> bool {
    if composition.len() <= 1 {
        return true;
    }
    for trans in composition.iter().rev() {
        let inner = trans.read();
        if inner.rule.effect_type == EffectType::ToneTransformation {
            let last_tone = Tone::from(inner.rule.effect);
            if !has_valid_tone(composition, last_tone) {
                return false;
            }
            break;
        }
    }
    let (fc, vo, lc) = extract_cvc_trans(composition);
    let flatten_mode = VIETNAMESE_MODE | LOWERCASE_MODE | TONE_LESS;
    let fc_s = flat::flatten(&fc, flatten_mode);
    let vo_s = flat::flatten(&vo, flatten_mode);
    let lc_s = flat::flatten(&lc, flatten_mode);
    is_valid_cvc(&fc_s, &vo_s, &lc_s, input_is_full_complete)
}

pub(crate) fn get_right_most_vowels(composition: &[Trans]) -> Vec<Trans> {
    let (_, vo, _) = extract_cvc_trans(composition);
    vo
}

pub(crate) fn find_tone_target(composition: &[Trans], std_style: bool) -> Option<Trans> {
    if composition.is_empty() {
        return None;
    }
    let (_, vo, lc) = extract_cvc_trans(composition);
    let vowels = filter_appending_composition(&vo);
    let mut target: Option<Trans> = None;
    if vowels.len() == 1 {
        target = Some(vowels[0].clone());
    } else if vowels.len() == 2 && std_style {
        for trans in &vo {
            let r = trans.read();
            if r.rule.result == 'ơ' || r.rule.result == 'ê' {
                target = Some(r.target.clone().unwrap_or_else(|| trans.clone()));
            }
        }
        if target.is_none() {
            target = Some(if !lc.is_empty() {
                vowels[1].clone()
            } else {
                vowels[0].clone()
            });
        }
    } else if vowels.len() == 2 {
        if !lc.is_empty() {
            target = Some(vowels[1].clone());
        } else {
            let s = flat::flatten(
                &vowels,
                ENGLISH_MODE | LOWERCASE_MODE | TONE_LESS | MARK_LESS,
            );
            target = Some(
                if s == "oa" || s == "oe" || s == "uy" || s == "ue" || s == "uo" {
                    vowels[1].clone()
                } else {
                    vowels[0].clone()
                },
            );
        }
    } else if vowels.len() == 3 {
        let s = flat::flatten(
            &vowels,
            ENGLISH_MODE | LOWERCASE_MODE | TONE_LESS | MARK_LESS,
        );
        target = Some(if s == "uye" {
            vowels[2].clone()
        } else {
            vowels[1].clone()
        });
    }
    target
}

pub(crate) fn has_valid_tone(composition: &[Trans], tone: Tone) -> bool {
    if matches!(tone, Tone::None | Tone::Acute | Tone::Dot) {
        return true;
    }
    let (_, _, lc) = extract_cvc_trans(composition);
    if lc.is_empty() {
        return true;
    }
    let last_consonants = flat::flatten(&lc, ENGLISH_MODE | LOWERCASE_MODE);
    let dot_with_consonants = ["c", "k", "p", "t", "ch"];
    !dot_with_consonants.contains(&last_consonants.as_str())
}

pub(crate) fn get_last_tone_transformation(composition: &[Trans]) -> Option<Trans> {
    for t in composition.iter().rev() {
        let inner = t.read();
        if inner.rule.effect_type == EffectType::ToneTransformation && inner.target.is_some() {
            return Some(t.clone());
        }
    }
    None
}

pub(crate) fn is_free(composition: &[Trans], trans: &Trans, effect_type: EffectType) -> bool {
    for t in composition {
        let inner = t.read();
        if inner
            .target
            .as_ref()
            .map(|x| Arc::ptr_eq(x, trans))
            .unwrap_or(false)
            && inner.rule.effect_type == effect_type
        {
            return false;
        }
    }
    true
}

fn extract_atomic_trans(
    composition: &[Trans],
    last: Vec<Trans>,
    last_is_vowel: bool,
) -> (Vec<Trans>, Vec<Trans>) {
    if composition.is_empty() {
        return (composition.to_vec(), last);
    }
    let tmp = &composition[composition.len() - 1];
    let tmp_inner = tmp.read();
    let tmp_nil = tmp_inner.target.is_none();
    let tmp_res_vowel = is_vowel(tmp_inner.rule.result);
    if tmp_nil && last_is_vowel != tmp_res_vowel {
        return (composition.to_vec(), last);
    }
    drop(tmp_inner);
    let prev = composition[..composition.len() - 1].to_vec();
    let mut new_last = vec![composition[composition.len() - 1].clone()];
    new_last.extend(last);
    extract_atomic_trans(&prev, new_last, last_is_vowel)
}

pub(crate) fn extract_cvc_appending_trans(
    composition: &[Trans],
) -> (Vec<Trans>, Vec<Trans>, Vec<Trans>) {
    let (head, last_consonant) = extract_atomic_trans(composition, Vec::new(), false);
    let (first_consonant, vowel) = extract_atomic_trans(&head, Vec::new(), true);
    let mut first_consonant = first_consonant;
    let mut vowel = vowel;
    let mut last_consonant = last_consonant;

    if !last_consonant.is_empty() && vowel.is_empty() && first_consonant.is_empty() {
        first_consonant = last_consonant;
        vowel = Vec::new();
        last_consonant = Vec::new();
    }

    if first_consonant.len() == 1 && !vowel.is_empty() {
        let g_ok = first_consonant[0].read().rule.result == 'g';
        let q_ok = first_consonant[0].read().rule.result == 'q';
        let v0 = vowel[0].read().rule.result;
        let gi_cluster = g_ok
            && v0 == 'i'
            && vowel.len() > 1
            && (last_consonant.is_empty() || vowel[1].read().rule.result != 'e');
        let qu_cluster = q_ok && v0 == 'u';
        if gi_cluster || qu_cluster {
            first_consonant.push(vowel[0].clone());
            vowel.remove(0);
        }
    }
    (first_consonant, vowel, last_consonant)
}

pub(crate) fn extract_cvc_trans(composition: &[Trans]) -> (Vec<Trans>, Vec<Trans>, Vec<Trans>) {
    let mut trans_map: HashMap<usize, Vec<Trans>> = HashMap::new();
    let mut appending_list: Vec<Trans> = Vec::new();
    for trans in composition {
        let inner = trans.read();
        if inner.target.is_none() {
            appending_list.push(trans.clone());
        } else if let Some(ref targ) = inner.target {
            trans_map
                .entry(Arc::as_ptr(targ) as usize)
                .or_default()
                .push(trans.clone());
        }
    }

    let (mut fc, mut vo, mut lc) = extract_cvc_appending_trans(&appending_list);
    let fc_clone = fc.clone();
    for t in fc_clone {
        if let Some(extra) = trans_map.remove(&(Arc::as_ptr(&t) as usize)) {
            fc.extend(extra);
        }
    }
    let vo_clone = vo.clone();
    for t in vo_clone {
        if let Some(extra) = trans_map.remove(&(Arc::as_ptr(&t) as usize)) {
            vo.extend(extra);
        }
    }
    let lc_clone = lc.clone();
    for t in lc_clone {
        if let Some(extra) = trans_map.remove(&(Arc::as_ptr(&t) as usize)) {
            lc.extend(extra);
        }
    }
    (fc, vo, lc)
}

pub(crate) fn extract_last_word_with_punctuation_marks(
    composition: &[Trans],
) -> (Vec<Trans>, Vec<Trans>) {
    for i in (0..composition.len()).rev() {
        let canvas = flat::get_canvas(&composition[i..], ENGLISH_MODE);
        if canvas.is_empty() {
            continue;
        }
        let c = canvas[0];
        if c.is_whitespace() {
            if i == composition.len() - 1 {
                return (composition.to_vec(), Vec::new());
            }
            return (composition[..i + 1].to_vec(), composition[i + 1..].to_vec());
        }
    }
    (Vec::new(), composition.to_vec())
}

pub(crate) fn extract_last_word(
    composition: &[Trans],
    effect_keys: &[char],
) -> (Vec<Trans>, Vec<Trans>) {
    for i in (0..composition.len()).rev() {
        let canvas = flat::get_canvas(
            &composition[i..],
            VIETNAMESE_MODE | LOWERCASE_MODE | TONE_LESS | MARK_LESS,
        );
        if canvas.is_empty() {
            continue;
        }
        let c = canvas[0];
        if !is_alpha(c) && !in_key_list(effect_keys, c) {
            if i == composition.len() - 1 {
                return (composition.to_vec(), Vec::new());
            }
            return (composition[..i + 1].to_vec(), composition[i + 1..].to_vec());
        }
    }
    (Vec::new(), composition.to_vec())
}

pub(crate) fn extract_last_syllable(composition: &[Trans]) -> (Vec<Trans>, Vec<Trans>) {
    let (mut previous, last) = extract_last_word(composition, &[]);
    let mut anchor = 0usize;
    for i in 0..last.len() {
        if !is_valid(&last[anchor..=i], false) {
            anchor = i;
        }
    }
    if anchor > 0 {
        previous.extend_from_slice(&last[..anchor]);
        return (previous, last[anchor..].to_vec());
    }
    (previous, last)
}

pub(crate) fn find_mark_target(composition: &[Trans], rules: &[Rule]) -> Option<(Trans, Rule)> {
    let str_full = flat::flatten(composition, VIETNAMESE_MODE);
    for i in (0..composition.len()).rev() {
        let trans = &composition[i];
        let tr = trans.read();
        for rule in rules {
            if rule.effect_type != EffectType::MarkTransformation {
                continue;
            }
            if tr.rule.result == rule.effect_on && rule.effect > 0 {
                let target = find_root_target(trans);
                let hypot = new_trans(rule.clone(), Some(target.clone()), false);
                let mut cand = composition.to_vec();
                cand.push(hypot);
                if str_full == flat::flatten(&cand, VIETNAMESE_MODE) {
                    continue;
                }
                let mut tmp = composition.to_vec();
                tmp.push(new_trans(rule.clone(), Some(target.clone()), false));
                if is_valid(&tmp, false) {
                    return Some((target, rule.clone()));
                }
            }
        }
    }
    None
}

/// `findTarget`: thử các luật **tone** trước, sau đó `find_mark_target`.
pub(crate) fn find_target(
    composition: &[Trans],
    applicable_rules: &[Rule],
    flags: u32,
) -> Option<(Option<Trans>, Rule)> {
    let str_full = flat::flatten(composition, VIETNAMESE_MODE);
    for applicable_rule in applicable_rules {
        if applicable_rule.effect_type != EffectType::ToneTransformation {
            continue;
        }
        let mut target: Option<Trans> = None;
        if flags & EFREE_TONE_MARKING != 0 {
            if has_valid_tone(composition, Tone::from(applicable_rule.effect)) {
                target = find_tone_target(composition, flags & ESTANDARD_TONE_STYLE != 0);
            }
        } else if let Some(last_appending) = find_last_appending_trans(composition) {
            let la = last_appending.read();
            if is_vowel(la.rule.effect_on) {
                target = Some(last_appending.clone());
            }
        }
        let hypot = new_trans(applicable_rule.clone(), target.clone(), false);
        let mut appended = composition.to_vec();
        appended.push(hypot);
        if str_full == flat::flatten(&appended, VIETNAMESE_MODE) {
            continue;
        }
        if Tone::from(applicable_rule.effect) == Tone::None {
            if let Some(ref t) = target {
                if is_free(composition, t, EffectType::ToneTransformation)
                    && find_tone_from_char(t.read().rule.result) == Tone::None
                {
                    target = None;
                }
            }
        }
        return Some((target, applicable_rule.clone()));
    }
    find_mark_target(composition, applicable_rules).map(|(t, r)| (Some(t), r))
}

pub(crate) fn generate_undo_transformations(
    composition: &[Trans],
    rules: &[Rule],
    flags: u32,
) -> Vec<Trans> {
    let mut transformations: Vec<Trans> = Vec::new();
    let str_flat = flat::flatten(composition, VIETNAMESE_MODE | TONE_LESS | LOWERCASE_MODE);
    for rule in rules {
        if rule.effect_type == EffectType::ToneTransformation {
            let mut target: Option<Trans> = None;
            if flags & EFREE_TONE_MARKING != 0 {
                if has_valid_tone(composition, Tone::from(rule.effect)) {
                    target = find_tone_target(composition, flags & ESTANDARD_TONE_STYLE != 0);
                }
            } else if let Some(last_appending) = find_last_appending_trans(composition) {
                let la = last_appending.read();
                if is_vowel(la.rule.effect_on) {
                    target = Some(last_appending.clone());
                }
            }
            let Some(target) = target else { continue };
            transformations.push(new_trans(
                Rule {
                    effect_type: EffectType::ToneTransformation,
                    effect: 0,
                    key: '\0',
                    effect_on: '\0',
                    result: '\0',
                    appended_rules: Vec::new(),
                },
                Some(target),
                false,
            ));
        } else if rule.effect_type == EffectType::MarkTransformation {
            for i in (0..composition.len()).rev() {
                let trans = &composition[i];
                let tr = trans.read();
                if tr.rule.result == rule.effect_on {
                    let target = find_root_target(trans);
                    let undo_trans = new_trans(
                        Rule {
                            key: '\0',
                            effect_type: EffectType::MarkTransformation,
                            effect: 0,
                            effect_on: '\0',
                            result: '\0',
                            appended_rules: Vec::new(),
                        },
                        Some(target.clone()),
                        false,
                    );
                    let mut app = composition.to_vec();
                    app.push(undo_trans.clone());
                    if str_flat == flat::flatten(&app, VIETNAMESE_MODE | TONE_LESS | LOWERCASE_MODE)
                    {
                        continue;
                    }
                    transformations.push(undo_trans);
                }
            }
        }
    }
    transformations
}

pub(crate) fn generate_transformations(
    composition: &[Trans],
    applicable_rules: &[Rule],
    flags: u32,
    lower_key: char,
    is_upper_case: bool,
) -> Vec<Trans> {
    let mut transformations: Vec<Trans> = Vec::new();
    if let Some(last) = composition.last() {
        let lr = last.read();
        if lr.rule.effect_type == EffectType::Appending
            && lr.rule.key == lower_key
            && lr.rule.key != lr.rule.result
        {
            transformations.push(new_trans(
                Rule {
                    effect_type: EffectType::MarkTransformation,
                    effect: Mark::Raw as u8,
                    key: '\0',
                    effect_on: '\0',
                    result: '\0',
                    appended_rules: Vec::new(),
                },
                Some(last.clone()),
                false,
            ));
            return transformations;
        }
    }

    if let Some((Some(target), applicable_rule)) =
        find_target(composition, applicable_rules, flags)
    {
        transformations.push(new_trans(
            applicable_rule.clone(),
            Some(target.clone()),
            is_upper_case,
        ));
        if applicable_rule.effect_type != EffectType::MarkTransformation {
            return transformations;
        }
        let mut new_comp = composition.to_vec();
        new_comp.push(transformations[0].clone());
        if is_valid(&new_comp, true) {
            return transformations;
        }
        if let Some((Some(t2), mut vr)) = find_target(&new_comp, applicable_rules, flags) {
            vr.key = '\0';
            transformations.push(new_trans(vr, Some(t2), false));
        }
        return transformations;
    }

    if REG_UH_O.is_match(&flat::flatten(
        composition,
        VIETNAMESE_MODE | TONE_LESS | LOWERCASE_MODE,
    )) {
        let vowels = filter_appending_composition(&get_right_most_vowels(composition));
        if !vowels.is_empty() {
            let trans = new_trans(
                Rule {
                    effect_type: EffectType::MarkTransformation,
                    key: '\0',
                    effect: Mark::None as u8,
                    effect_on: '\0',
                    result: '\0',
                    appended_rules: Vec::new(),
                },
                Some(vowels[0].clone()),
                false,
            );
            let mut try_comp = composition.to_vec();
            try_comp.push(trans.clone());
            if let Some((Some(target), applicable_rule)) =
                find_target(&try_comp, applicable_rules, flags)
            {
                if !Arc::ptr_eq(&target, &vowels[0]) {
                    transformations.push(trans);
                    transformations.push(new_trans(applicable_rule, Some(target), is_upper_case));
                    return transformations;
                }
            }
        }
    }
    let undo = generate_undo_transformations(composition, applicable_rules, flags);
    if !undo.is_empty() {
        transformations.extend(undo);
        transformations.push(new_appending_trans(lower_key, is_upper_case));
    }
    transformations
}

pub(crate) fn generate_fallback_transformations(
    _composition: &[Trans],
    applicable_rules: &[Rule],
    lower_key: char,
    is_upper_case: bool,
) -> Vec<Trans> {
    let mut transformations: Vec<Trans> = Vec::new();
    let trans = generate_appending_trans(applicable_rules, lower_key, is_upper_case);
    let appended: Vec<Rule> = trans.read().rule.appended_rules.clone();
    transformations.push(trans.clone());
    for mut appended_rule in appended {
        let _is_upper_case = is_upper_case || appended_rule.effect_on.is_uppercase();
        appended_rule.key = '\0';
        appended_rule.effect_on = appended_rule
            .effect_on
            .to_lowercase()
            .next()
            .unwrap_or(appended_rule.effect_on);
        appended_rule.result = appended_rule.effect_on;
        transformations.push(new_trans(appended_rule, None, _is_upper_case));
    }
    transformations
}

pub(crate) fn break_composition(composition: &[Trans]) -> Vec<Trans> {
    let mut result: Vec<Trans> = Vec::new();
    for trans in composition {
        let inner = trans.read();
        if inner.rule.key == '\0' {
            continue;
        }
        result.push(new_appending_trans(inner.rule.key, inner.is_upper_case));
    }
    result
}

pub(crate) fn refresh_last_tone_target(composition: &[Trans], std_style: bool) -> Vec<Trans> {
    let mut transformations: Vec<Trans> = Vec::new();
    let rightmost_vowels = get_right_most_vowels(composition);
    let Some(last_tone_trans) = get_last_tone_transformation(composition) else {
        return transformations;
    };
    if rightmost_vowels.is_empty() {
        return transformations;
    }
    let Some(new_tone_target) = find_tone_target(composition, std_style) else {
        return transformations;
    };

    let cur_target = last_tone_trans.read().target.clone();
    let need_refresh = cur_target
        .as_ref()
        .map(|t| !Arc::ptr_eq(t, &new_tone_target))
        .unwrap_or(true);
    if need_refresh {
        last_tone_trans.write().target = Some(new_tone_target.clone());
        transformations.push(new_trans(
            Rule {
                key: '\0',
                effect_type: EffectType::ToneTransformation,
                effect: Tone::None as u8,
                effect_on: '\0',
                result: '\0',
                appended_rules: Vec::new(),
            },
            Some(new_tone_target.clone()),
            false,
        ));
        let mut override_rule = last_tone_trans.read().rule.clone();
        override_rule.key = '\0';
        transformations.push(new_trans(override_rule, Some(new_tone_target), false));
    }
    transformations
}

pub(crate) fn reg_uoh_tail_matches(s: &str) -> bool {
    REG_UOH_TAIL.is_match(s)
}
