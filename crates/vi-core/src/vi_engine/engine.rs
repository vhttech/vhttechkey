//! vhttechkey composition engine — `ViEngine`.

use crate::vi_engine::compose::{
    self, break_composition, extract_last_syllable, extract_last_word,
    extract_last_word_with_punctuation_marks, find_last_appending_trans,
    generate_fallback_transformations, generate_transformations, is_valid,
    new_appending_trans, refresh_last_tone_target, reg_uoh_tail_matches,
};
use crate::vi_engine::flat;
use crate::vi_engine::rules::{parsed_telex, parsed_vni, parsed_viqr};
use crate::vi_engine::text::can_process_key;
use crate::vi_engine::types::{
    EngineMode, EFREE_TONE_MARKING, ENGLISH_MODE, ESTDFLAGS, ESTANDARD_TONE_STYLE, FULL_TEXT,
    IN_REVERSE_ORDER, LOWERCASE_MODE, ParsedInputMethod, PUNCTUATION_MODE, TONE_LESS, Trans,
    VIETNAMESE_MODE,
};

/// Engine xử lý ký tự tiếng Việt của vhttechkey (thuần Rust).
#[derive(Debug)]
pub struct ViEngine {
    composition: Vec<Trans>,
    input_method: ParsedInputMethod,
    flags: u32,
    /// Log ký tự gốc đã gõ (dùng để replay khi restore snapshot).
    key_log: Vec<char>,
}

impl ViEngine {
    pub fn new_telex() -> Self {
        Self::with_method(parsed_telex(), ESTDFLAGS)
    }

    pub fn new_vni() -> Self {
        Self::with_method(parsed_vni(), ESTDFLAGS)
    }

    pub fn new_viqr() -> Self {
        Self::with_method(parsed_viqr(), ESTDFLAGS)
    }

    pub(crate) fn with_method(input_method: ParsedInputMethod, flags: u32) -> Self {
        Self { composition: Vec::new(), input_method, flags, key_log: Vec::new() }
    }

    pub fn set_flag(&mut self, flags: u32) {
        self.flags = flags;
    }

    pub fn flags(&self) -> u32 {
        self.flags
    }

    pub fn reset(&mut self) {
        self.composition.clear();
        self.key_log.clear();
    }

    pub fn composition_len(&self) -> usize {
        self.composition.len()
    }

    /// Trả về bản sao key_log (dùng cho `EngineSnapshot`).
    pub(crate) fn clone_key_log(&self) -> Vec<char> {
        self.key_log.clone()
    }

    /// Khôi phục trạng thái bằng cách reset rồi replay log phím.
    pub(crate) fn restore_from_key_log(&mut self, key_log: Vec<char>, mode: EngineMode) {
        self.composition.clear();
        self.key_log.clear();
        for ch in key_log {
            self.process_key(ch, mode);
        }
    }

    /// String raw chưa xử lý (dùng cho spell-check fallback).
    pub(crate) fn raw_keys_string(&self) -> String {
        self.key_log.iter().collect()
    }

    /// True nếu composition rỗng.
    pub fn is_empty(&self) -> bool {
        self.composition.is_empty()
    }

    fn applicable_rules(&self, key: char) -> Vec<crate::vi_engine::types::Rule> {
        let lk = key.to_lowercase().next().unwrap_or(key);
        self.input_method
            .rules
            .iter()
            .filter(|r| r.key.to_lowercase().next().unwrap_or(r.key) == lk)
            .cloned()
            .collect()
    }

    fn find_target_by_key(
        &self,
        composition: &[Trans],
        key: char,
    ) -> Option<(Option<Trans>, crate::vi_engine::types::Rule)> {
        compose::find_target(composition, &self.applicable_rules(key), self.flags)
    }

    fn generate_transformations_inner(
        &self,
        composition: &[Trans],
        lower_key: char,
        is_upper_case: bool,
    ) -> Vec<Trans> {
        let applicable = self.applicable_rules(lower_key);
        let mut transformations =
            generate_transformations(composition, &applicable, self.flags, lower_key, is_upper_case);
        if transformations.is_empty() {
            transformations =
                generate_fallback_transformations(composition, &applicable, lower_key, is_upper_case);
            let mut new_composition = composition.to_vec();
            new_composition.extend(transformations.clone());
            if let Some(vt) = self.apply_uow_shortcut(&new_composition) {
                transformations.push(vt);
            }
        }
        let merged = [composition, transformations.as_slice()].concat();
        if self.flags & EFREE_TONE_MARKING != 0 && is_valid(&merged, false) {
            transformations.extend(refresh_last_tone_target(
                &merged,
                self.flags & ESTANDARD_TONE_STYLE != 0,
            ));
        }
        transformations
    }

    fn new_composition(&mut self, composition: &[Trans], key: char, is_upper_case: bool) -> Vec<Trans> {
        let (previous_transformations, mut last_syllable) = extract_last_syllable(composition);
        last_syllable.extend(self.generate_transformations_inner(&last_syllable, key, is_upper_case));
        let mut out = previous_transformations;
        out.extend(last_syllable);
        out
    }

    fn apply_uow_shortcut(&self, syllable: &[Trans]) -> Option<Trans> {
        let s = flat::flatten(syllable, VIETNAMESE_MODE | LOWERCASE_MODE | TONE_LESS);
        if self.input_method.super_keys.is_empty() || !reg_uoh_tail_matches(&s) {
            return None;
        }
        let sk = self.input_method.super_keys[0];
        let (target_opt, mut missing_rule) = self.find_target_by_key(syllable, sk)?;
        let target = target_opt?;
        missing_rule.key = '\0';
        Some(compose::new_trans(missing_rule, Some(target), false))
    }

    pub fn can_process_key(&self, key: char) -> bool {
        can_process_key(key.to_lowercase().next().unwrap_or(key), &self.input_method.keys)
    }

    pub fn is_valid(&self, input_is_full_complete: bool) -> bool {
        let (_, last) = extract_last_word(&self.composition, &self.input_method.keys);
        is_valid(&last, input_is_full_complete)
    }

    pub fn get_processed_string(&self, mode: EngineMode) -> String {
        let tmp: Vec<Trans> = if mode & FULL_TEXT != 0 {
            self.composition.clone()
        } else if mode & PUNCTUATION_MODE != 0 {
            let (_, t) = extract_last_word_with_punctuation_marks(&self.composition);
            return flat::flatten(&t, VIETNAMESE_MODE);
        } else {
            extract_last_word(&self.composition, &self.input_method.keys).1
        };
        flat::flatten(&tmp, mode)
    }

    pub fn process_string(&mut self, s: &str, mode: EngineMode) {
        for ch in s.chars() {
            self.process_key(ch, mode);
        }
    }

    pub fn process_key(&mut self, key: char, mode: EngineMode) {
        let lower_key = key.to_lowercase().next().unwrap_or(key);
        let is_upper_case = key.is_uppercase();
        self.key_log.push(key);
        if mode & ENGLISH_MODE != 0 || !self.can_process_key(lower_key) {
            let t = new_appending_trans(lower_key, is_upper_case);
            if mode & IN_REVERSE_ORDER != 0 {
                self.composition.insert(0, t);
            } else {
                self.composition.push(t);
            }
            return;
        }
        let comp = self.composition.clone();
        self.composition = self.new_composition(&comp, lower_key, is_upper_case);
    }

    pub fn restore_last_word(&mut self, to_vietnamese: bool) {
        let (previous, last_comb) = extract_last_word(&self.composition, &self.input_method.keys);
        if last_comb.is_empty() {
            return;
        }
        if !to_vietnamese {
            self.composition =
                [previous.as_slice(), break_composition(&last_comb).as_slice()].concat();
        } else {
            let mut new_comp: Vec<Trans> = Vec::new();
            for tnx in &last_comb {
                let inner = tnx.read();
                let k = inner.rule.key;
                let uc = inner.is_upper_case;
                drop(inner);
                new_comp = self.new_composition(&new_comp, k, uc);
            }
            let mut prev = previous;
            prev.extend(new_comp);
            self.composition = prev;
        }
    }

    pub fn remove_last_char(&mut self, refresh_last_tone_target_flag: bool) {
        let Some(last_appending) = find_last_appending_trans(&self.composition) else {
            return;
        };
        if !self.can_process_key(last_appending.read().rule.key) {
            self.composition.pop();
            return;
        }
        let (previous, last_comb) = extract_last_word(&self.composition, &self.input_method.keys);
        let mut new_comb: Vec<Trans> = Vec::new();
        for t in &last_comb {
            let targets_last = t
                .read()
                .target
                .as_ref()
                .map(|x| std::sync::Arc::ptr_eq(x, &last_appending))
                .unwrap_or(false);
            let is_last = std::sync::Arc::ptr_eq(t, &last_appending);
            if targets_last || is_last {
                continue;
            }
            new_comb.push(t.clone());
        }
        if refresh_last_tone_target_flag {
            let refresh =
                refresh_last_tone_target(&new_comb, self.flags & ESTANDARD_TONE_STYLE != 0);
            new_comb.extend(refresh);
        }
        let mut prev = previous;
        prev.extend(new_comb);
        self.composition = prev;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vi_engine_telex_tooi_to_toi() {
        let mut e = ViEngine::new_telex();
        e.process_string("tooi", VIETNAMESE_MODE);
        assert_eq!(e.get_processed_string(FULL_TEXT), "tôi");
    }

    #[test]
    fn vi_engine_telex_dduowcj_duoc() {
        let mut e = ViEngine::new_telex();
        e.process_string("dduowcj", VIETNAMESE_MODE);
        assert_eq!(e.get_processed_string(FULL_TEXT), "được");
    }

    #[test]
    fn vi_engine_telex_perr_gives_per() {
        let mut e = ViEngine::new_telex();
        e.process_string("perr", VIETNAMESE_MODE);
        assert_eq!(e.get_processed_string(FULL_TEXT), "per");
    }

    #[test]
    fn vi_engine_telex_err_gives_er() {
        // "er" → "ẻ", then second "r" should undo tone → "er"
        let mut e = ViEngine::new_telex();
        e.process_string("err", VIETNAMESE_MODE);
        assert_eq!(e.get_processed_string(FULL_TEXT), "er");
    }

    #[test]
    fn vi_engine_telex_orr_gives_or() {
        let mut e = ViEngine::new_telex();
        e.process_string("orr", VIETNAMESE_MODE);
        assert_eq!(e.get_processed_string(FULL_TEXT), "or");
    }
}
