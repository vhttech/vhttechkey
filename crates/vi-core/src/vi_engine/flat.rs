//! `flattener.go` — flatten composition to NFC string.

use unicode_normalization::UnicodeNormalization;

use crate::vi_engine::text::{add_mark_to_char, add_tone_to_char};
use crate::vi_engine::types::{
    EffectType, EngineMode, Mark, Trans, ENGLISH_MODE, LOWERCASE_MODE, MARK_LESS, TONE_LESS,
};

pub(crate) fn flatten(composition: &[Trans], mode: EngineMode) -> String {
    get_canvas(composition, mode)
        .into_iter()
        .collect::<String>()
        .nfc()
        .collect::<String>()
}

fn ptr_usize(t: &Trans) -> usize {
    std::sync::Arc::as_ptr(t) as usize
}

pub(crate) fn get_canvas(composition: &[Trans], mode: EngineMode) -> Vec<char> {
    use std::collections::HashMap;
    let mut appending_map: HashMap<usize, Vec<Trans>> = HashMap::new();
    let mut appending_list: Vec<Trans> = Vec::new();

    for trans in composition {
        let inner = trans.read();
        if mode & ENGLISH_MODE != 0 {
            if inner.rule.key == '\0' {
                continue;
            }
            appending_list.push(trans.clone());
        } else if inner.rule.effect_type == EffectType::Appending {
            if inner.rule.key == '\0' {
                continue;
            }
            appending_list.push(trans.clone());
        } else if let Some(ref targ) = inner.target {
            appending_map
                .entry(ptr_usize(targ))
                .or_default()
                .push(trans.clone());
        }
    }

    let mut canvas = Vec::new();
    for appending_trans in appending_list {
        let inner = appending_trans.read();
        let trans_list = appending_map
            .remove(&ptr_usize(&appending_trans))
            .unwrap_or_default();

        let mut chr = if mode & ENGLISH_MODE != 0 {
            inner.rule.key
        } else {
            inner.rule.effect_on
        };

        if mode & ENGLISH_MODE == 0 {
            for eff in &trans_list {
                let er = eff.read();
                match er.rule.effect_type {
                    EffectType::MarkTransformation => {
                        if er.rule.effect == Mark::Raw as u8 {
                            chr = inner.rule.key;
                        } else {
                            chr = add_mark_to_char(chr, er.rule.effect);
                        }
                    }
                    EffectType::ToneTransformation => {
                        chr = add_tone_to_char(chr, er.rule.effect);
                    }
                    _ => {}
                }
            }
        }

        if mode & TONE_LESS != 0 {
            chr = add_tone_to_char(chr, 0);
        }
        if mode & MARK_LESS != 0 {
            chr = add_mark_to_char(chr, 0);
        }
        if mode & LOWERCASE_MODE != 0 {
            chr = chr.to_lowercase().next().unwrap_or(chr);
        } else if inner.is_upper_case {
            chr = chr.to_uppercase().next().unwrap_or(chr);
        }
        canvas.push(chr);
    }
    canvas
}
