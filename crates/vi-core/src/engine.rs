use tracing::trace;
use unicode_normalization::UnicodeNormalization;

use crate::{
    commit_engine::CommitEngine,
    keyboard::combine_dead_key,
    methods::InputMethod,
    preedit_buffer::PreeditBuffer,
    spell::SpellOptions,
    types::{InputEvent, Key, Modifiers, NfcString, PreeditText, StateTransition, TransitionResult},
    unicode_pipeline::UnicodePipeline,
    vi_engine::{
        ViEngine,
        types::{VIETNAMESE_MODE as VI_MODE, FULL_TEXT as VI_FULL, ENGLISH_MODE as VI_ENGLISH},
    },
    vietnamese_dict::VietnameseDict,
};

/// Core trait every composition engine must implement.
///
/// Implementations must be pure state machines: given the same sequence of
/// `InputEvent`s they must always produce the same sequence of
/// `StateTransition`s. No I/O, no side effects, no platform calls.
pub trait CompositionEngine: Send + Sync {
    /// Process one event and return the resulting state transition.
    fn process(&mut self, event: &InputEvent) -> TransitionResult;

    /// Current preedit text (empty if nothing is being composed).
    fn preedit(&self) -> PreeditText;

    /// Reset to initial state, discarding any in-progress composition.
    fn reset(&mut self);

    /// Return pending preedit as an NFC-normalized string and clear the buffer.
    ///
    /// Call this on FocusOut so the platform can commit in-progress text instead
    /// of silently discarding it. Returns `None` if the buffer is already empty.
    /// The returned [`NfcString`] is guaranteed to be in NFC form.
    fn flush_commit(&mut self) -> Option<NfcString> {
        None
    }
}

/// Suppresses nested duplicate `KeyDown(Char)` (driver bounce: two Downs before Up).
/// Alternating keys typed quickly (`x`…`o`…`x`) always see `KeyUp` between Downs and
/// are never suppressed (unlike a naive per-key millisecond debounce).
struct KeyRepeatGuard {
    /// `KeyDown(Char)` received without a matching `KeyUp` for that character yet.
    pending_char_down: Option<char>,
}

impl KeyRepeatGuard {
    fn new() -> Self {
        Self { pending_char_down: None }
    }

    /// Returns `true` if this `KeyDown(Char)` is a ghost duplicate (second Down
    /// before Up for the same character).
    fn is_duplicate_char(&mut self, ch: char) -> bool {
        if self.pending_char_down == Some(ch) {
            return true;
        }
        self.pending_char_down = Some(ch);
        false
    }

    fn reset_key(&mut self, key: &Key) {
        if let Key::Char(c) = key {
            if self.pending_char_down == Some(*c) {
                self.pending_char_down = None;
            }
        }
    }

    fn reset_all(&mut self) {
        self.pending_char_down = None;
    }
}

/// Standard implementation backed by a `PreeditBuffer` và `ViEngine`.
pub struct StandardEngine {
    method: InputMethod,
    buf: PreeditBuffer,
    /// vhttechkey composition engine — nguồn sự thật cho ký tự tiếng Việt.
    vi_engine: ViEngine,
    /// The last character processed by a `KeyDown(Char)` event.
    last_char: Option<char>,
    /// True if the most recent `KeyDown(Char)` caused a composition rule to fire.
    /// Used to guard against re-firing the same rule on the first `KeyRepeat`.
    last_rule_fired: bool,
    /// Dead key waiting to combine with the next character press.
    pending_dead_key: Option<char>,
    /// Deduplicates ghost nested `KeyDown(Char)` before `KeyUp`.
    repeat_guard: KeyRepeatGuard,
    /// Most recent surrounding-text snapshot from the application (text, cursor byte offset).
    surrounding: Option<(String, usize)>,
    /// vhttechkey dictionary checks on commit (optional).
    spell: SpellOptions,
}

/// Snapshot of engine composition state for rollback on dispatch failure.
#[derive(Clone)]
pub struct EngineSnapshot {
    buf: PreeditBuffer,
    /// Key log của `ViEngine` để replay khi restore.
    vi_keys: Vec<char>,
    last_char: Option<char>,
    last_rule_fired: bool,
    pending_dead_key: Option<char>,
}

impl StandardEngine {
    pub fn new(method: InputMethod) -> Self {
        Self::with_spell(method, SpellOptions::default())
    }

    /// Construct with optional `vietnamese.cm.dict` behaviour (see [`SpellOptions`]).
    pub fn with_spell(method: InputMethod, spell: SpellOptions) -> Self {
        Self {
            vi_engine: Self::make_vi_engine(method),
            method,
            buf: PreeditBuffer::new(),
            last_char: None,
            last_rule_fired: false,
            pending_dead_key: None,
            repeat_guard: KeyRepeatGuard::new(),
            surrounding: None,
            spell,
        }
    }

    /// Tạo `ViEngine` tương ứng với phương thức nhập.
    fn make_vi_engine(method: InputMethod) -> ViEngine {
        match method {
            InputMethod::Telex => ViEngine::new_telex(),
            InputMethod::Vni => ViEngine::new_vni(),
            InputMethod::Viqr => ViEngine::new_viqr(),
        }
    }

    /// Replace dictionary / spell-check flags at runtime (e.g. config reload).
    pub fn set_spell_options(&mut self, spell: SpellOptions) {
        self.spell = spell;
    }

    /// Swap the active input method and discard any in-progress composition.
    pub fn set_method(&mut self, method: InputMethod) {
        self.vi_engine = Self::make_vi_engine(method);
        self.method = method;
        self.buf.clear();
        self.last_char = None;
        self.last_rule_fired = false;
        self.pending_dead_key = None;
        self.repeat_guard.reset_all();
        self.surrounding = None;
    }

    /// Capture a snapshot of the current composition state.
    pub fn snapshot(&self) -> EngineSnapshot {
        EngineSnapshot {
            buf: self.buf.clone(),
            vi_keys: self.vi_engine.clone_key_log(),
            last_char: self.last_char,
            last_rule_fired: self.last_rule_fired,
            pending_dead_key: self.pending_dead_key,
        }
    }

    /// Restore engine state from a previously captured snapshot.
    pub fn restore(&mut self, snap: EngineSnapshot) {
        self.buf = snap.buf;
        self.vi_engine.restore_from_key_log(snap.vi_keys, VI_MODE);
        self.last_char = snap.last_char;
        self.last_rule_fired = snap.last_rule_fired;
        self.pending_dead_key = snap.pending_dead_key;
    }

    fn handle_char(&mut self, ch: char, mods: Modifiers) -> TransitionResult {
        // Bug 2: Alt without Ctrl may represent AltGr on systems that do not set
        // the dedicated `altgr` flag. Commit any pending preedit and let the key
        // reach the application unchanged.
        if mods.alt && !mods.ctrl {
            self.last_char = None;
            self.last_rule_fired = false;
            return self.commit_current();
        }

        self.last_char = Some(ch);

        // Kiểm tra composition gate (như trước — biết khi nào không chuyển đổi).
        let current_display: Vec<char> = self.buf.display_chars().chars().collect();
        // If the current composition is already invalid (fallback state), subsequent keys
        // must use EnglishMode so they are appended raw rather than re-triggering Vietnamese
        // transforms. This prevents double-consonant loss (e.g. "process" → "proces") caused
        // by the undo mechanism firing on the second tone key when it should just be a raw append.
        let composition_is_invalid =
            !self.vi_engine.is_empty() && !self.vi_engine.is_valid(false);
        let vi_mode = if ch.is_uppercase() {
            // Uppercase letters bypass composition rules (tone/mark keys like Shift+S must
            // not trigger sắc). Raw append — ViEngine renders uppercase via is_upper_case flag.
            VI_ENGLISH
        } else if composition_is_invalid {
            VI_ENGLISH
        } else {
            match self.method {
                InputMethod::Telex => {
                    if crate::composition_gate::telex_apply_allowed(&current_display, ch, mods) {
                        VI_MODE
                    } else {
                        VI_ENGLISH
                    }
                }
                InputMethod::Vni => {
                    if crate::composition_gate::vni_apply_allowed(&current_display, ch, mods) {
                        VI_MODE
                    } else {
                        VI_ENGLISH
                    }
                }
                InputMethod::Viqr => VI_MODE,
            }
        };

        // Gửi ký tự vào ViEngine (nguồn sự thật cho composition).
        self.vi_engine.process_key(ch, vi_mode);
        let vn_out = self.vi_engine.get_processed_string(VI_FULL);
        // Show raw keys when the composition is (or was already) structurally invalid Vietnamese
        // AND the output still contains Vietnamese chars that need to be undone to raw.
        // Gating on `has_vietnamese_out` prevents the raw fallback from firing after a
        // double-tone undo (e.g. "perr" → "per") whose output is already pure ASCII —
        // without this guard, the next char ('m' in "perrm") would show "perrm" (raw_keys)
        // instead of the correct "perm".
        let has_vietnamese_out =
            vn_out.chars().any(crate::composition_gate::is_vietnamese_composed_char);
        let new_display_str =
            if has_vietnamese_out
                && (composition_is_invalid
                    || (vi_mode == VI_MODE && !self.vi_engine.is_valid(false)))
            {
                self.vi_engine.raw_keys_string()
            } else {
                vn_out
            };
        let new_display: Vec<char> = new_display_str.chars().collect();
        self.last_rule_fired = vi_mode == VI_MODE;

        // Cập nhật buf để giữ raw keys (spell check) và display (preedit text).
        let push_result = self.buf.set_display(new_display, ch);

        match push_result {
            Ok(()) => {
                let text = self.buf.to_preedit_text();
                trace!(preedit = %text, "preedit updated");
                Ok(StateTransition::PreeditUpdated(text))
            }
            Err(_) => {
                // Buffer full (64 codepoints): commit what we have, then start
                // a fresh composition with the triggering character.
                let preedit = self.build_commit_preedit_text();
                let committed = CommitEngine::commit(&preedit)?;
                self.buf.clear();
                self.vi_engine.reset();
                self.last_rule_fired = false;
                // Gõ lại ký tự hiện tại vào engine mới.
                self.vi_engine.process_key(ch, VI_MODE);
                let first_display: Vec<char> =
                    self.vi_engine.get_processed_string(VI_FULL).chars().collect();
                self.buf.set_display(first_display, ch).unwrap_or_else(|e| {
                    panic!("Post-flush set_display must succeed but failed: {e}. Char: {ch:?}")
                });
                let new_preedit = self.buf.to_preedit_text();
                Ok(StateTransition::CommitThenPreedit(committed, new_preedit))
            }
        }
    }

    fn handle_backspace(&mut self) -> TransitionResult {
        if self.buf.is_empty() {
            // When the composition buffer is empty, check surrounding text: if the
            // character immediately before the cursor is a Vietnamese precomposed
            // syllable, strip its tone mark and re-enter composition with the base
            // character so the user can re-compose without retyping the whole syllable.
            if let Some((ref text, cursor)) = self.surrounding {
                if cursor <= text.len() && text.is_char_boundary(cursor) {
                    if let Some(ch) = text[..cursor].chars().next_back() {
                        if is_vietnamese_char(ch) {
                            if let Some(base) = vietnamese_base_char(ch) {
                                self.vi_engine.process_key(base, VI_MODE);
                                let disp: Vec<char> =
                                    self.vi_engine.get_processed_string(VI_FULL).chars().collect();
                                let _ = self.buf.set_display(disp, base);
                                return Ok(StateTransition::PreeditUpdated(
                                    self.buf.to_preedit_text(),
                                ));
                            }
                        }
                    }
                }
            }
            return Ok(StateTransition::PassThrough);
        }

        // Delete the last visual character: skip any intermediate modifier-only
        // snapshots and land on the snapshot that has fewer displayed chars than the current state.
        self.buf.rollback_to_shorter();
        if self.buf.is_empty() {
            self.vi_engine.reset();
            return Ok(StateTransition::Cleared);
        }

        // Replay toàn bộ key sequence của buf vào vi_engine để đồng bộ trạng thái.
        let keys: Vec<char> = self.buf.key_sequence().to_vec();
        self.vi_engine.restore_from_key_log(keys, VI_MODE);

        Ok(StateTransition::PreeditUpdated(self.buf.to_preedit_text()))
    }

    /// Choose composed NFC text vs raw Telex/VNI keys for commit (dictionary gate).
    fn build_commit_preedit_text(&self) -> PreeditText {
        if self.should_commit_raw_vi_sequence() {
            PreeditText::new(self.vi_engine.raw_keys_string())
        } else {
            self.buf.to_preedit_text()
        }
    }

    /// Fallback to raw keys khi `IBspellCheckWithDicts` bật.
    fn should_commit_raw_vi_sequence(&self) -> bool {
        let opts = &self.spell;
        if !opts.commit_spell_check_dict {
            return false;
        }
        let Some(dict) = opts.dictionary.as_ref() else {
            return false;
        };
        let composed = self.buf.as_string();
        if !crate::spell::buffer_contains_vietnamese(&composed) {
            return false;
        }
        if opts.dd_freestyle && composed.contains('đ') {
            return false;
        }
        let key = VietnameseDict::normalize_key(&composed);
        !dict.contains_normalized(&key)
    }

    fn commit_current(&mut self) -> TransitionResult {
        if self.buf.is_empty() {
            return Ok(StateTransition::PassThrough);
        }
        let preedit = self.build_commit_preedit_text();
        let committed = CommitEngine::commit(&preedit)?;
        self.buf.clear();
        self.vi_engine.reset();
        Ok(StateTransition::CommitAndClear(committed))
    }
}

/// Return true when `c` is a Vietnamese character that carries a tone mark or
/// a modified base vowel (circumflex, breve, horn).  Includes both precomposed
/// forms (NFC) and standalone combining diacritics (U+0300–U+036F).
fn is_vietnamese_char(c: char) -> bool {
    let n = c as u32;
    // Standalone combining diacritical marks
    (0x0300..=0x036F).contains(&n)
    // Latin Extended Additional — the primary Vietnamese block
    || (0x1EA0..=0x1EF9).contains(&n)
    // Selected precomposed chars in Latin-1 Supplement and Latin Extended A/B
    || matches!(
        c,
        'à' | 'á' | 'â' | 'ã' | 'è' | 'é' | 'ê' | 'ì' | 'í'
        | 'ò' | 'ó' | 'ô' | 'õ' | 'ù' | 'ú' | 'ý'
        | 'À' | 'Á' | 'Â' | 'Ã' | 'È' | 'É' | 'Ê' | 'Ì' | 'Í'
        | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ù' | 'Ú' | 'Ý'
        | 'ă' | 'Ă' | 'ơ' | 'Ơ' | 'ư' | 'Ư' | 'đ' | 'Đ'
    )
}

/// Strip the tone mark from a Vietnamese precomposed character and return the
/// base form (which may still carry a vowel modifier such as circumflex or breve).
///
/// Tone marks are the five Vietnamese diacritics: grave (U+0300), acute (U+0301),
/// tilde (U+0303), hook-above (U+0309), and dot-below (U+0323).  Base modifiers
/// (circumflex U+0302, breve U+0306, horn U+031B) are preserved.
///
/// Returns `None` if `c` has no decomposable tone mark (i.e. it is already a base
/// character and re-entering composition is not useful).
fn vietnamese_base_char(c: char) -> Option<char> {
    // NFD-decompose, filter out tone marks only, NFC-recompose.
    let nfd: String = c.to_string().nfd().collect();
    let without_tone: String = nfd
        .chars()
        .filter(|&d| !matches!(d as u32, 0x0300 | 0x0301 | 0x0303 | 0x0309 | 0x0323))
        .collect();
    // If nothing was removed, the character had no tone mark — don't re-enter.
    if without_tone == nfd {
        return None;
    }
    without_tone.nfc().next()
}

/// True for keysyms that represent bare modifier keys: Shift, Ctrl, Alt, Meta,
/// Super, Hyper, Caps_Lock, AltGr (ISO_Level3_Shift), Compose (Multi_key),
/// Num_Lock, Scroll_Lock. Pressing these alone must not commit preedit.
fn is_modifier_keysym(sym: u32) -> bool {
    matches!(
        sym,
        // Shift_L/R, Control_L/R, Caps_Lock, Shift_Lock, Meta_L/R,
        // Alt_L/R, Super_L/R, Hyper_L/R
        0xffe1..=0xffee
        // AltGr — ISO_Level3_Shift
        | 0xfe03
        // Multi_key (Compose)
        | 0xff20
        // Num_Lock
        | 0xff7f
        // Scroll_Lock
        | 0xff14
    )
}

impl CompositionEngine for StandardEngine {
    fn process(&mut self, event: &InputEvent) -> TransitionResult {
        // AltGr safety: flush pending preedit and forward the key without
        // attempting Vietnamese composition. Dead-key prefixed characters
        // must reach the application unchanged.
        let altgr = match event {
            InputEvent::KeyDown(_, mods) | InputEvent::KeyRepeat(_, mods) => mods.altgr,
            _ => false,
        };
        if altgr {
            self.last_char = None;
            self.last_rule_fired = false;
            self.pending_dead_key = None;
            return if !self.buf.is_empty() {
                self.commit_current()
            } else {
                Ok(StateTransition::PassThrough)
            };
        }

        let result = match event {
            // Bug 1: Ctrl or Super + char key means a keyboard shortcut. Commit
            // any pending preedit so the text is not lost, then pass the shortcut
            // through to the application without applying composition rules.
            InputEvent::KeyDown(Key::Char(_), mods) if mods.ctrl || mods.super_key => {
                self.last_char = None;
                self.last_rule_fired = false;
                self.pending_dead_key = None;
                if !self.buf.is_empty() {
                    self.commit_current()
                } else {
                    Ok(StateTransition::PassThrough)
                }
            }
            // Space and common punctuation terminate Vietnamese composition and must pass
            // through to the application unchanged. They are never Vietnamese composition
            // characters so must never extend the preedit buffer.
            // Exception: VIQR uses some of these characters as tone/form marks, so
            // skip this arm for those characters to let normal composition handle them.
            InputEvent::KeyDown(Key::Char(ch), _)
                if matches!(*ch, ' ' | ',' | '.' | ';' | ':' | '!' | '?'
                                | '\'' | '"' | '(' | ')' | '[' | ']'
                                | '{' | '}' | '/' | '\\' | '-' | '_')
                && !self.method.is_composition_char(*ch) =>
            {
                self.last_char = None;
                self.last_rule_fired = false;
                self.pending_dead_key = None;
                if self.buf.is_empty() {
                    Ok(StateTransition::PassThrough)
                } else {
                    let preedit = self.build_commit_preedit_text();
                    let committed = CommitEngine::commit(&preedit)?;
                    self.buf.clear();
                    self.vi_engine.reset();
                    Ok(StateTransition::CommitThenPassThrough(committed))
                }
            }
            InputEvent::KeyDown(Key::Char(ch), mods) => {
                // Suppress ghost presses: identical key arriving within the
                // dedup threshold without an intervening KeyUp.
                // Skip for alt-modified keys — ghost presses never carry alt.
                if !mods.alt && self.repeat_guard.is_duplicate_char(*ch) {
                    return Ok(StateTransition::Consumed);
                }
                if let Some(dead) = self.pending_dead_key.take() {
                    if let Some(combined) = combine_dead_key(dead, *ch) {
                        // Dead key combined successfully; treat as a single char.
                        return self.handle_char(combined, *mods);
                    }
                    // No combination: push dead char literally vào vi_engine + buf.
                    self.vi_engine.process_key(dead, VI_MODE);
                    let dead_disp: Vec<char> =
                        self.vi_engine.get_processed_string(VI_FULL).chars().collect();
                    match self.buf.set_display(dead_disp, dead) {
                        Ok(()) => {}
                        Err(_) => {
                            // Buffer overflow during dead char flush: commit and restart.
                            let preedit = self.build_commit_preedit_text();
                            let committed = CommitEngine::commit(&preedit)?;
                            self.buf.clear();
                            self.vi_engine.reset();
                            self.last_rule_fired = false;
                            self.vi_engine.process_key(dead, VI_MODE);
                            let d2: Vec<char> =
                                self.vi_engine.get_processed_string(VI_FULL).chars().collect();
                            let _ = self.buf.set_display(d2, dead);
                            // Return the commit; new char will land on the next event.
                            return Ok(StateTransition::CommitThenPreedit(
                                committed,
                                self.buf.to_preedit_text(),
                            ));
                        }
                    }
                }
                self.handle_char(*ch, *mods)
            }
            InputEvent::KeyDown(Key::Backspace, _) => {
                self.last_char = None;
                self.last_rule_fired = false;
                self.pending_dead_key = None;
                self.handle_backspace()
            }
            InputEvent::KeyDown(Key::Return, _) | InputEvent::KeyDown(Key::Tab, _) => {
                self.last_char = None;
                self.last_rule_fired = false;
                self.pending_dead_key = None;
                self.commit_current()
            }
            InputEvent::KeyDown(Key::Escape, _) => {
                self.last_char = None;
                self.last_rule_fired = false;
                self.pending_dead_key = None;
                self.buf.clear();
                self.vi_engine.reset();
                Ok(StateTransition::Cleared)
            }
            // Dead key: store and wait for the next character to combine with.
            // If a dead key was already pending, emit it as a literal first.
            InputEvent::KeyDown(Key::DeadKey(dead_char), _) => {
                self.last_char = None;
                self.last_rule_fired = false;
                if let Some(prev) = self.pending_dead_key.replace(*dead_char) {
                    // Previous dead key could not combine; emit it as a literal.
                    self.vi_engine.process_key(prev, VI_MODE);
                    let prev_disp: Vec<char> =
                        self.vi_engine.get_processed_string(VI_FULL).chars().collect();
                    let _ = self.buf.set_display(prev_disp, prev);
                    let combined = format!("{}{}", self.buf.as_string(), dead_char);
                    Ok(StateTransition::PreeditUpdated(PreeditText::new(combined)))
                } else {
                    Ok(StateTransition::PreeditUpdated(PreeditText::new(dead_char.to_string())))
                }
            }
            // Compose key: initiates a compose sequence; pass through without
            // disturbing preedit (the platform layer handles compose resolution).
            InputEvent::KeyDown(Key::ComposeKey, _) => Ok(StateTransition::PassThrough),
            // Bare modifier keys (Shift, Ctrl, Alt, Super, Hyper, Caps Lock,
            // AltGr/ISO_Level3_Shift, Num_Lock, Scroll_Lock).
            // These must not disturb in-progress composition — the chord event
            // (e.g. Ctrl+C) that follows will handle any necessary commit.
            InputEvent::KeyDown(Key::Keysym(sym), _) if is_modifier_keysym(*sym) => {
                Ok(StateTransition::PassThrough)
            }
            InputEvent::KeyDown(_, _) => {
                // Non-character keys: commit pending preedit then pass through.
                self.last_char = None;
                self.last_rule_fired = false;
                self.pending_dead_key = None;
                if !self.buf.is_empty() {
                    self.commit_current()
                } else {
                    Ok(StateTransition::PassThrough)
                }
            }
            // Bug 3: Key repeat extends preedit with the raw character WITHOUT applying
            // composition rules. The repeated key is appended as-is so holding 'o'
            // gives "ooo" not "ô" + "o".
            InputEvent::KeyRepeat(Key::Char(ch), _) => {
                let same = self.last_char == Some(*ch);
                if same && self.last_rule_fired {
                    self.last_rule_fired = false;
                } else {
                    self.last_char = Some(*ch);
                    self.last_rule_fired = false;
                }
                // Append raw char to vi_engine (ENGLISH_MODE = no composition rules).
                self.vi_engine.process_key(*ch, VI_ENGLISH);
                match self.buf.push(*ch) {
                    Ok(()) => Ok(StateTransition::PreeditUpdated(self.buf.to_preedit_text())),
                    Err(_) => {
                        let preedit = self.build_commit_preedit_text();
                        let committed = CommitEngine::commit(&preedit)?;
                        self.buf.clear();
                        self.vi_engine.reset();
                        self.vi_engine.process_key(*ch, VI_ENGLISH);
                        if let Err(e) = self.buf.push(*ch) {
                            tracing::warn!("preedit push failed after clear: {e}");
                        }
                        Ok(StateTransition::CommitThenPreedit(
                            committed,
                            self.buf.to_preedit_text(),
                        ))
                    }
                }
            }
            InputEvent::KeyRepeat(Key::Backspace, _) => {
                self.last_char = None;
                self.last_rule_fired = false;
                self.handle_backspace()
            }
            InputEvent::KeyRepeat(_, _) => Ok(StateTransition::PassThrough),
            InputEvent::KeyUp(key) => {
                // Reset the dedup timer for this key so a genuine re-press after
                // release is never mistakenly suppressed.
                self.repeat_guard.reset_key(key);
                Ok(StateTransition::Consumed)
            }
            InputEvent::FocusOut => {
                self.last_char = None;
                self.last_rule_fired = false;
                self.pending_dead_key = None;
                self.repeat_guard.reset_all();
                if !self.buf.is_empty() {
                    self.commit_current()
                } else {
                    Ok(StateTransition::Consumed)
                }
            }
            InputEvent::FocusIn => {
                self.repeat_guard.reset_all();
                Ok(StateTransition::Consumed)
            }
            InputEvent::Reset => {
                self.last_char = None;
                self.last_rule_fired = false;
                self.pending_dead_key = None;
                self.repeat_guard.reset_all();
                self.buf.clear();
                self.vi_engine.reset();
                Ok(StateTransition::Cleared)
            }
            InputEvent::SurroundingText { text, cursor } => {
                self.surrounding = Some((text.clone(), *cursor));
                Ok(StateTransition::Consumed)
            }
        };
        if let Ok(StateTransition::PreeditUpdated(ref p)) = result {
            debug_assert!(!p.is_empty(), "PreeditUpdated must never carry empty text");
        }
        result
    }

    fn preedit(&self) -> PreeditText {
        self.buf.to_preedit_text()
    }

    fn reset(&mut self) {
        self.buf.clear();
        self.vi_engine.reset();
        self.pending_dead_key = None;
    }

    fn flush_commit(&mut self) -> Option<NfcString> {
        // If a dead key was pending with no following character, treat it as a
        // literal character so it is not silently dropped on focus change.
        if let Some(dead) = self.pending_dead_key.take() {
            self.vi_engine.process_key(dead, VI_MODE);
            let disp: Vec<char> = self.vi_engine.get_processed_string(VI_FULL).chars().collect();
            let _ = self.buf.set_display(disp, dead);
        }
        if self.buf.is_empty() {
            return None;
        }
        let text = self.build_commit_preedit_text();
        self.buf.clear();
        self.vi_engine.reset();
        self.last_char = None;
        self.last_rule_fired = false;
        self.repeat_guard.reset_all();
        Some(UnicodePipeline::normalize_only(text.as_str()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn telex() -> StandardEngine {
        StandardEngine::new(InputMethod::Telex)
    }

    fn vni() -> StandardEngine {
        StandardEngine::new(InputMethod::Vni)
    }

    fn viqr() -> StandardEngine {
        StandardEngine::new(InputMethod::Viqr)
    }

    fn key(c: char) -> InputEvent {
        InputEvent::KeyDown(Key::Char(c), Modifiers::none())
    }

    fn backspace() -> InputEvent {
        InputEvent::KeyDown(Key::Backspace, Modifiers::none())
    }

    fn process_str(engine: &mut StandardEngine, s: &str) {
        for ch in s.chars() {
            engine.process(&key(ch)).unwrap();
            let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
        }
    }

    fn committed(t: StateTransition) -> Option<String> {
        match t {
            StateTransition::CommitAndClear(c) => Some(c.as_str().to_owned()),
            StateTransition::Commit(c) => Some(c.as_str().to_owned()),
            StateTransition::CommitThenPreedit(c, _) => Some(c.as_str().to_owned()),
            StateTransition::CommitThenPassThrough(c) => Some(c.as_str().to_owned()),
            _ => None,
        }
    }

    // ── Basic composition ────────────────────────────────────────────────────

    #[test]
    fn basic_telex_toi() {
        let mut e = telex();
        process_str(&mut e, "too");
        assert_eq!(e.preedit().as_str(), "tô");
        e.process(&key('i')).unwrap();
        assert_eq!(e.preedit().as_str(), "tôi");
        let t = e.process(&InputEvent::KeyDown(Key::Return, Modifiers::none())).unwrap();
        assert_eq!(committed(t).as_deref(), Some("tôi"));
    }

    /// Production gate: `xox…` + tone stays literal; `toto` is not spam (→ `totos` = `tốt`).
    #[test]
    fn telex_ascii_spam_tone_key_stays_literal() {
        let mut e = telex();
        process_str(&mut e, "xoxoxoxo");
        e.process(&key('s')).unwrap();
        let _ = e.process(&InputEvent::KeyUp(Key::Char('s')));
        assert_eq!(e.preedit().as_str(), "xoxoxoxos");
    }

    #[test]
    fn telex_totos_produces_tot_sac() {
        let mut e = telex();
        process_str(&mut e, "totos");
        assert_eq!(e.preedit().as_str(), "tốt");
    }

    #[test]
    fn backspace_rollback_tooi() {
        // Character-level delete: backspace removes one visual char at a time,
        // skipping intermediate composition snapshots with the same char count.
        let mut e = telex();
        process_str(&mut e, "tooi");
        assert_eq!(e.preedit().as_str(), "tôi");
        e.process(&backspace()).unwrap();
        assert_eq!(e.preedit().as_str(), "tô");
        // "to" (2 chars) is skipped — rollback goes directly to "t" (1 char).
        e.process(&backspace()).unwrap();
        assert_eq!(e.preedit().as_str(), "t");
        e.process(&backspace()).unwrap();
        assert_eq!(e.preedit().as_str(), "");
    }

    #[test]
    fn focus_out_commits() {
        let mut e = telex();
        process_str(&mut e, "aa");
        let t = e.process(&InputEvent::FocusOut).unwrap();
        assert_eq!(committed(t).as_deref(), Some("â"));
    }

    #[test]
    fn escape_clears() {
        let mut e = telex();
        process_str(&mut e, "too");
        e.process(&InputEvent::KeyDown(Key::Escape, Modifiers::none())).unwrap();
        assert!(e.preedit().is_empty());
    }

    #[test]
    fn reset_event_clears() {
        let mut e = telex();
        process_str(&mut e, "aaa");
        e.process(&InputEvent::Reset).unwrap();
        assert!(e.preedit().is_empty());
    }

    // ── Bug fix: compound vowel + tone uses display state not raw keys ────────

    #[test]
    fn telex_aa_s_gives_a_circ_acute() {
        // "aa" → â (display), then "s" must tone the displayed 'â' to get 'ấ',
        // not the raw key 'a' to get 'á'.
        let mut e = telex();
        process_str(&mut e, "aas");
        assert_eq!(e.preedit().as_str(), "ấ");
    }

    #[test]
    fn telex_oo_j_gives_o_circ_nang() {
        let mut e = telex();
        process_str(&mut e, "ooj");
        assert_eq!(e.preedit().as_str(), "ộ");
    }

    #[test]
    fn telex_uw_s_gives_u_horn_sac() {
        let mut e = telex();
        process_str(&mut e, "uws");
        assert_eq!(e.preedit().as_str(), "ứ");
    }

    #[test]
    fn telex_aw_j_gives_a_breve_nang() {
        let mut e = telex();
        process_str(&mut e, "awj");
        assert_eq!(e.preedit().as_str(), "ặ");
    }

    // ── Key repeat ───────────────────────────────────────────────────────────

    #[test]
    fn key_repeat_char_extends_preedit_without_rules() {
        // Holding 'o' after an initial press should append 'o' raw, not re-run
        // the "oo→ô" rule a second time.
        let mut e = telex();
        process_str(&mut e, "o");
        assert_eq!(e.preedit().as_str(), "o");
        // First repeat: raw push, no rule — should give "oo" not "ô".
        e.process(&InputEvent::KeyRepeat(Key::Char('o'), Modifiers::none())).unwrap();
        assert_eq!(e.preedit().as_str(), "oo");
        // Second repeat: "ooo"
        e.process(&InputEvent::KeyRepeat(Key::Char('o'), Modifiers::none())).unwrap();
        assert_eq!(e.preedit().as_str(), "ooo");
    }

    #[test]
    fn key_repeat_backspace_deletes_one_per_repeat() {
        let mut e = telex();
        process_str(&mut e, "abc");
        e.process(&InputEvent::KeyRepeat(Key::Backspace, Modifiers::none())).unwrap();
        assert_eq!(e.preedit().as_str(), "ab");
        e.process(&InputEvent::KeyRepeat(Key::Backspace, Modifiers::none())).unwrap();
        assert_eq!(e.preedit().as_str(), "a");
    }

    // ── AltGr safety ─────────────────────────────────────────────────────────

    #[test]
    fn altgr_flushes_preedit_and_passes_through() {
        let mut e = telex();
        process_str(&mut e, "too"); // preedit = "tô"
        assert_eq!(e.preedit().as_str(), "tô");
        // AltGr + some key: should commit "tô" and return CommitAndClear.
        let t = e.process(&InputEvent::KeyDown(
            Key::Char('e'),
            Modifiers::altgr(),
        )).unwrap();
        assert_eq!(committed(t).as_deref(), Some("tô"));
        assert!(e.preedit().is_empty());
    }

    #[test]
    fn altgr_on_empty_preedit_passes_through() {
        let mut e = telex();
        let t = e.process(&InputEvent::KeyDown(
            Key::Char('e'),
            Modifiers::altgr(),
        )).unwrap();
        assert_eq!(t, StateTransition::PassThrough);
    }

    #[test]
    fn altgr_on_key_repeat_flushes() {
        let mut e = telex();
        process_str(&mut e, "aa"); // preedit = "â"
        let t = e.process(&InputEvent::KeyRepeat(
            Key::Char('e'),
            Modifiers::altgr(),
        )).unwrap();
        assert_eq!(committed(t).as_deref(), Some("â"));
    }

    // ── flush_commit ─────────────────────────────────────────────────────────

    #[test]
    fn flush_commit_returns_preedit_and_clears() {
        let mut e = telex();
        process_str(&mut e, "too"); // preedit = "tô"
        let s = e.flush_commit();
        assert_eq!(s.as_ref().map(|n| n.as_str()), Some("tô"));
        assert!(e.preedit().is_empty());
    }

    #[test]
    fn flush_commit_empty_returns_none() {
        let mut e = telex();
        assert!(e.flush_commit().is_none());
    }

    // ── 72-syllable completeness: Telex ──────────────────────────────────────

    fn telex_preedit(keys: &str) -> String {
        let mut e = telex();
        process_str(&mut e, keys);
        e.preedit().as_str().to_owned()
    }

    #[test]
    fn telex_72_syllables() {
        let cases: &[(&str, &str)] = &[
            // a × 6 tones
            ("a", "a"), ("as", "á"), ("af", "à"), ("ar", "ả"), ("ax", "ã"), ("aj", "ạ"),
            // ă × 6
            ("aw", "ă"), ("aws", "ắ"), ("awf", "ằ"), ("awr", "ẳ"), ("awx", "ẵ"), ("awj", "ặ"),
            // â × 6
            ("aa", "â"), ("aas", "ấ"), ("aaf", "ầ"), ("aar", "ẩ"), ("aax", "ẫ"), ("aaj", "ậ"),
            // e × 6
            ("e", "e"), ("es", "é"), ("ef", "è"), ("er", "ẻ"), ("ex", "ẽ"), ("ej", "ẹ"),
            // ê × 6
            ("ee", "ê"), ("ees", "ế"), ("eef", "ề"), ("eer", "ể"), ("eex", "ễ"), ("eej", "ệ"),
            // i × 6
            ("i", "i"), ("is", "í"), ("if", "ì"), ("ir", "ỉ"), ("ix", "ĩ"), ("ij", "ị"),
            // o × 6
            ("o", "o"), ("os", "ó"), ("of", "ò"), ("or", "ỏ"), ("ox", "õ"), ("oj", "ọ"),
            // ô × 6
            ("oo", "ô"), ("oos", "ố"), ("oof", "ồ"), ("oor", "ổ"), ("oox", "ỗ"), ("ooj", "ộ"),
            // ơ × 6
            ("ow", "ơ"), ("ows", "ớ"), ("owf", "ờ"), ("owr", "ở"), ("owx", "ỡ"), ("owj", "ợ"),
            // u × 6
            ("u", "u"), ("us", "ú"), ("uf", "ù"), ("ur", "ủ"), ("ux", "ũ"), ("uj", "ụ"),
            // ư × 6
            ("uw", "ư"), ("uws", "ứ"), ("uwf", "ừ"), ("uwr", "ử"), ("uwx", "ữ"), ("uwj", "ự"),
            // y × 6
            ("y", "y"), ("ys", "ý"), ("yf", "ỳ"), ("yr", "ỷ"), ("yx", "ỹ"), ("yj", "ỵ"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                telex_preedit(input),
                *expected,
                "Telex: engine preedit for {:?} should be {:?}",
                input,
                expected
            );
        }
    }

    // ── 72-syllable completeness: VNI ────────────────────────────────────────

    fn vni_preedit(keys: &str) -> String {
        let mut e = vni();
        process_str(&mut e, keys);
        e.preedit().as_str().to_owned()
    }

    #[test]
    fn vni_72_syllables() {
        let cases: &[(&str, &str)] = &[
            // a × 6 tones
            ("a", "a"), ("a1", "á"), ("a2", "à"), ("a3", "ả"), ("a4", "ã"), ("a5", "ạ"),
            // ă × 6 (a8 → ă, then tone digit)
            ("a8", "ă"), ("a81", "ắ"), ("a82", "ằ"), ("a83", "ẳ"), ("a84", "ẵ"), ("a85", "ặ"),
            // â × 6 (a6 → â, then tone digit)
            ("a6", "â"), ("a61", "ấ"), ("a62", "ầ"), ("a63", "ẩ"), ("a64", "ẫ"), ("a65", "ậ"),
            // e × 6
            ("e", "e"), ("e1", "é"), ("e2", "è"), ("e3", "ẻ"), ("e4", "ẽ"), ("e5", "ẹ"),
            // ê × 6 (e6 → ê)
            ("e6", "ê"), ("e61", "ế"), ("e62", "ề"), ("e63", "ể"), ("e64", "ễ"), ("e65", "ệ"),
            // i × 6
            ("i", "i"), ("i1", "í"), ("i2", "ì"), ("i3", "ỉ"), ("i4", "ĩ"), ("i5", "ị"),
            // o × 6
            ("o", "o"), ("o1", "ó"), ("o2", "ò"), ("o3", "ỏ"), ("o4", "õ"), ("o5", "ọ"),
            // ô × 6 (o6 → ô)
            ("o6", "ô"), ("o61", "ố"), ("o62", "ồ"), ("o63", "ổ"), ("o64", "ỗ"), ("o65", "ộ"),
            // ơ × 6 (o7 → ơ)
            ("o7", "ơ"), ("o71", "ớ"), ("o72", "ờ"), ("o73", "ở"), ("o74", "ỡ"), ("o75", "ợ"),
            // u × 6
            ("u", "u"), ("u1", "ú"), ("u2", "ù"), ("u3", "ủ"), ("u4", "ũ"), ("u5", "ụ"),
            // ư × 6 (u7 → ư)
            ("u7", "ư"), ("u71", "ứ"), ("u72", "ừ"), ("u73", "ử"), ("u74", "ữ"), ("u75", "ự"),
            // y × 6
            ("y", "y"), ("y1", "ý"), ("y2", "ỳ"), ("y3", "ỷ"), ("y4", "ỹ"), ("y5", "ỵ"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                vni_preedit(input),
                *expected,
                "VNI: engine preedit for {:?} should be {:?}",
                input,
                expected
            );
        }
    }

    // ── 72-syllable completeness: VIQR ───────────────────────────────────────

    fn viqr_preedit(keys: &str) -> String {
        let mut e = viqr();
        process_str(&mut e, keys);
        e.preedit().as_str().to_owned()
    }

    // ── Bug-fix regression tests ─────────────────────────────────────────────

    #[test]
    fn test_ctrl_c_commits_and_passes_through() {
        // Bug 1: Ctrl+char while composing must commit preedit, not extend it.
        let mut e = telex();
        process_str(&mut e, "too"); // preedit = "tô"
        assert_eq!(e.preedit().as_str(), "tô");
        let t = e.process(&InputEvent::KeyDown(
            Key::Char('c'),
            Modifiers { ctrl: true, ..Modifiers::none() },
        ))
        .unwrap();
        assert_eq!(committed(t).as_deref(), Some("tô"));
        assert!(e.preedit().is_empty());
        // On empty buffer, Ctrl+char returns PassThrough.
        let t2 = e
            .process(&InputEvent::KeyDown(
                Key::Char('c'),
                Modifiers { ctrl: true, ..Modifiers::none() },
            ))
            .unwrap();
        assert_eq!(t2, StateTransition::PassThrough);
    }

    #[test]
    fn test_altgr_o_does_not_fire_telex() {
        // Bug 2: Alt (without Ctrl) must commit preedit and not apply vowel rules.
        let mut e = telex();
        e.process(&key('o')).unwrap(); // preedit = "o"
        let t = e.process(&InputEvent::KeyDown(
            Key::Char('o'),
            Modifiers { alt: true, ..Modifiers::none() },
        ))
        .unwrap();
        // Without fix, apply(['o'], 'o') would give "ô". With fix, we commit "o".
        assert_eq!(committed(t).as_deref(), Some("o"));
        assert!(e.preedit().is_empty());
    }

    #[test]
    fn test_key_repeat_no_double_vowel_rule() {
        // Bug 3: KeyRepeat must not re-fire a composition rule.
        let mut e = telex();
        e.process(&key('o')).unwrap();
        assert_eq!(e.preedit().as_str(), "o");
        // KeyRepeat appends raw char; rule must NOT fire.
        e.process(&InputEvent::KeyRepeat(Key::Char('o'), Modifiers::none())).unwrap();
        assert_eq!(e.preedit().as_str(), "oo"); // not "ô"
        // Another repeat keeps extending raw.
        e.process(&InputEvent::KeyRepeat(Key::Char('o'), Modifiers::none())).unwrap();
        assert_eq!(e.preedit().as_str(), "ooo");
    }

    #[test]
    fn test_key_repeat_after_rule_fired() {
        // Bug 3: after oo→ô fires, repeat of 'o' appends raw 'o' (gives "ôo").
        let mut e = telex();
        process_str(&mut e, "oo"); // fires oo→ô, last_rule_fired = true
        assert_eq!(e.preedit().as_str(), "ô");
        e.process(&InputEvent::KeyRepeat(Key::Char('o'), Modifiers::none())).unwrap();
        assert_eq!(e.preedit().as_str(), "ôo");
    }

    #[test]
    fn test_shift_uppercase_s_no_tone() {
        // Bug 4: uppercase 'S' (Shift held) must not trigger the sắc tone rule.
        let mut e = telex();
        e.process(&key('a')).unwrap(); // preedit = "a"
        e.process(&InputEvent::KeyDown(Key::Char('S'), Modifiers::shift())).unwrap();
        assert_eq!(e.preedit().as_str(), "aS");
    }

    #[test]
    fn viqr_72_syllables() {
        let cases: &[(&str, &str)] = &[
            // a × 6 tones
            ("a", "a"), ("a'", "á"), ("a`", "à"), ("a?", "ả"), ("a~", "ã"), ("a.", "ạ"),
            // ă × 6 (a( → ă)
            ("a(", "ă"), ("a('", "ắ"), ("a(`", "ằ"), ("a(?", "ẳ"), ("a(~", "ẵ"), ("a(.", "ặ"),
            // â × 6 (a^ → â)
            ("a^", "â"), ("a^'", "ấ"), ("a^`", "ầ"), ("a^?", "ẩ"), ("a^~", "ẫ"), ("a^.", "ậ"),
            // e × 6
            ("e", "e"), ("e'", "é"), ("e`", "è"), ("e?", "ẻ"), ("e~", "ẽ"), ("e.", "ẹ"),
            // ê × 6 (e^ → ê)
            ("e^", "ê"), ("e^'", "ế"), ("e^`", "ề"), ("e^?", "ể"), ("e^~", "ễ"), ("e^.", "ệ"),
            // i × 6
            ("i", "i"), ("i'", "í"), ("i`", "ì"), ("i?", "ỉ"), ("i~", "ĩ"), ("i.", "ị"),
            // o × 6
            ("o", "o"), ("o'", "ó"), ("o`", "ò"), ("o?", "ỏ"), ("o~", "õ"), ("o.", "ọ"),
            // ô × 6 (o^ → ô)
            ("o^", "ô"), ("o^'", "ố"), ("o^`", "ồ"), ("o^?", "ổ"), ("o^~", "ỗ"), ("o^.", "ộ"),
            // ơ × 6 (o+ → ơ)
            ("o+", "ơ"), ("o+'", "ớ"), ("o+`", "ờ"), ("o+?", "ở"), ("o+~", "ỡ"), ("o+.", "ợ"),
            // u × 6
            ("u", "u"), ("u'", "ú"), ("u`", "ù"), ("u?", "ủ"), ("u~", "ũ"), ("u.", "ụ"),
            // ư × 6 (u+ → ư)
            ("u+", "ư"), ("u+'", "ứ"), ("u+`", "ừ"), ("u+?", "ử"), ("u+~", "ữ"), ("u+.", "ự"),
            // y × 6
            ("y", "y"), ("y'", "ý"), ("y`", "ỳ"), ("y?", "ỷ"), ("y~", "ỹ"), ("y.", "ỵ"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                viqr_preedit(input),
                *expected,
                "VIQR: engine preedit for {:?} should be {:?}",
                input,
                expected
            );
        }
    }

    // ── Word-boundary / pass-through regression tests ────────────────────────

    #[test]
    fn space_triggers_commit_not_preedit() {
        let mut e = telex();
        process_str(&mut e, "viet");
        assert_eq!(e.preedit().as_str(), "viet");
        let t = e.process(&InputEvent::KeyDown(Key::Char(' '), Modifiers::none())).unwrap();
        assert!(
            matches!(t, StateTransition::CommitThenPassThrough(_)),
            "space must return CommitThenPassThrough, got {t:?}"
        );
        if let StateTransition::CommitThenPassThrough(c) = t {
            assert_eq!(c.as_str(), "viet");
        }
        assert!(e.preedit().is_empty(), "preedit must be empty after space");
    }

    #[test]
    fn invalid_syllable_shows_raw_not_vietnamese() {
        // "monitor": 'r' fires as hỏi on 'o' → "monitỏ" is invalid Vietnamese
        // → preedit must show raw "monitor" not "monitỏ"
        let mut e = telex();
        process_str(&mut e, "monitor");
        assert_eq!(e.preedit().as_str(), "monitor");
    }

    #[test]
    fn invalid_syllable_telex_shows_raw() {
        // "telex": 'x' fires as ngã on 'e' → "telẽ" is invalid → show raw "telex"
        let mut e = telex();
        process_str(&mut e, "telex");
        assert_eq!(e.preedit().as_str(), "telex");
    }

    #[test]
    fn vi_engine_resets_after_space_commit() {
        // Regression: vi_engine was not reset on space-triggered commit, causing
        // the next word to receive accumulated raw keys from the previous word.
        // e.g. "toi gox" would show "tooigox" as preedit for the second word.
        let mut e = telex();
        process_str(&mut e, "tooi");
        e.process(&InputEvent::KeyDown(Key::Char(' '), Modifiers::none())).unwrap();
        // Start second word — vi_engine must be clean
        process_str(&mut e, "gox");
        assert_eq!(e.preedit().as_str(), "gõ", "vi_engine must reset after space commit");
    }

    #[test]
    fn space_on_empty_buffer_is_passthrough() {
        let mut e = telex();
        let t = e.process(&InputEvent::KeyDown(Key::Char(' '), Modifiers::none())).unwrap();
        assert_eq!(t, StateTransition::PassThrough);
    }

    #[test]
    fn comma_triggers_commit() {
        let mut e = telex();
        process_str(&mut e, "xin");
        let t = e.process(&InputEvent::KeyDown(Key::Char(','), Modifiers::none())).unwrap();
        assert!(matches!(t, StateTransition::CommitThenPassThrough(_)));
    }

    #[test]
    fn preedit_updated_never_empty() {
        let mut eng = StandardEngine::new(InputMethod::Telex);
        let mods = Modifiers::none();
        for ch in ['v', 'i', 'e', 't'] {
            let result = eng.process(&InputEvent::KeyDown(Key::Char(ch), mods));
            if let Ok(StateTransition::PreeditUpdated(ref p)) = result {
                assert!(
                    !p.is_empty(),
                    "PreeditUpdated must not be empty after typing '{ch}'"
                );
            }
        }
    }

    // ── Early-tone vowel-form regression ────────────────────────────────────

    #[test]
    fn telex_early_tone_then_vowel_form_asw() {
        let mut e = telex();
        process_str(&mut e, "asw");
        assert_eq!(e.preedit().as_str(), "ắ");
    }

    #[test]
    fn telex_early_tone_then_vowel_form_toswn() {
        let mut e = telex();
        process_str(&mut e, "toswn");
        assert_eq!(e.preedit().as_str(), "tớn");
    }

    #[test]
    fn dead_key_preedit_not_empty() {
        let mut eng = StandardEngine::new(InputMethod::Telex);
        let mods = Modifiers::none();
        // Dead key followed by char — neither step should produce empty PreeditUpdated
        let _ = eng.process(&InputEvent::KeyDown(Key::DeadKey('^'), mods));
        let result = eng.process(&InputEvent::KeyDown(Key::Char('a'), mods));
        if let Ok(StateTransition::PreeditUpdated(ref p)) = result {
            assert!(!p.is_empty(), "Dead key + char PreeditUpdated must not be empty");
        }
    }

    #[test]
    fn dead_key_shows_preedit() {
        let mut eng = StandardEngine::new(InputMethod::Telex);
        let mods = Modifiers::none();
        let result = eng.process(&InputEvent::KeyDown(Key::DeadKey('^'), mods)).unwrap();
        assert!(
            matches!(&result, StateTransition::PreeditUpdated(p) if p.as_str() == "^"),
            "single dead key must return PreeditUpdated(\"^\"), got {result:?}",
        );
    }

    #[test]
    fn two_non_combining_dead_keys_preedit() {
        let mut eng = StandardEngine::new(InputMethod::Telex);
        let mods = Modifiers::none();
        eng.process(&InputEvent::KeyDown(Key::DeadKey('^'), mods)).unwrap();
        // '(' cannot combine with '^' as a dead-key pair, so '^' is pushed to
        // the buffer as a literal and '(' becomes the new pending dead key.
        let result = eng.process(&InputEvent::KeyDown(Key::DeadKey('('), mods)).unwrap();
        assert!(
            matches!(&result, StateTransition::PreeditUpdated(p) if p.as_str() == "^("),
            "second non-combining dead key must return PreeditUpdated(\"^(\"), got {result:?}",
        );
    }

    // ── Tone-anywhere for multi-vowel words ──────────────────────────────────

    #[test]
    fn telex_toas_preedit_toa_acute() {
        // EstdToneStyle: với 2 nguyên âm "oa" không có phụ âm cuối,
        // tone được đặt trên nguyên âm đầu ('o') → "tóa".
        let mut e = telex();
        process_str(&mut e, "toas");
        assert_eq!(e.preedit().as_str(), "tóa");
    }

    #[test]
    fn telex_toas_space_commits_toa_acute() {
        let mut e = telex();
        process_str(&mut e, "toas");
        let t = e.process(&InputEvent::KeyDown(Key::Char(' '), Modifiers::none())).unwrap();
        assert_eq!(committed(t).as_deref(), Some("tóa"));
    }

    #[test]
    fn telex_toasn_commits_toan() {
        // Với phụ âm cuối 'n', tone di chuyển sang nguyên âm cuối ('a') → "toán".
        let mut e = telex();
        process_str(&mut e, "toasn");
        let t = e.process(&InputEvent::KeyDown(Key::Return, Modifiers::none())).unwrap();
        assert_eq!(committed(t).as_deref(), Some("toán"));
    }

    #[test]
    fn telex_towns_gives_to_horn_sac_n() {
        // t+o+w → tơ (vowel form), then n → tơn, then s applies sắc to ơ → tớn.
        let mut e = telex();
        process_str(&mut e, "towns");
        assert_eq!(e.preedit().as_str(), "tớn");
    }

    #[test]
    fn telex_aswf_gives_a_breve_grave() {
        // a+s → á, then w applies breve → ắ, then f changes tone to huyền → ằ.
        let mut e = telex();
        process_str(&mut e, "aswf");
        assert_eq!(e.preedit().as_str(), "ằ");
    }

    // ── Backspace rollback with tone-anywhere ────────────────────────────────

    #[test]
    fn backspace_rollback_tone_anywhere_asw() {
        // After "asw" → "ắ", one backspace removes the whole composed char.
        // Intermediate snapshots ("á", "a") have the same char count (1) so
        // rollback_to_shorter skips them and lands on empty.
        let mut e = telex();
        process_str(&mut e, "asw");
        assert_eq!(e.preedit().as_str(), "ắ");
        e.process(&backspace()).unwrap();
        assert_eq!(e.preedit().as_str(), "");
    }

    // ── Double tone-key undo (perr → per, err → er, etc.) ────────────────────

    #[test]
    fn telex_perr_gives_per() {
        let mut e = telex();
        process_str(&mut e, "perr");
        assert_eq!(e.preedit().as_str(), "per");
    }

    #[test]
    fn telex_err_gives_er() {
        let mut e = telex();
        process_str(&mut e, "err");
        assert_eq!(e.preedit().as_str(), "er");
    }

    #[test]
    fn telex_aff_gives_af() {
        // "af" → "à", second "f" undoes huyền → "af"
        let mut e = telex();
        process_str(&mut e, "aff");
        assert_eq!(e.preedit().as_str(), "af");
    }

    #[test]
    fn telex_ass_gives_as() {
        // "as" → "á", second "s" undoes sắc → "as"
        let mut e = telex();
        process_str(&mut e, "ass");
        assert_eq!(e.preedit().as_str(), "as");
    }

    #[test]
    fn telex_eerr_falls_back_to_raw() {
        // "ee" → "ê", "r" → "ể", second "r" would undo tone → "êr",
        // but 'ê' is a Vietnamese char and "êr" is not a valid syllable,
        // so the fallback-to-raw fires → "eerr". Same behavior as ibus-lotus.
        let mut e = telex();
        process_str(&mut e, "eerr");
        assert_eq!(e.preedit().as_str(), "eerr");
    }

    #[test]
    fn telex_aarr_falls_back_to_raw() {
        // "aa" → "â", "r" → "ẩ", second "r" would undo tone → "âr",
        // but 'â' is a Vietnamese char and "âr" is not valid → falls back to "aarr".
        let mut e = telex();
        process_str(&mut e, "aarr");
        assert_eq!(e.preedit().as_str(), "aarr");
    }

    #[test]
    fn telex_oorr_falls_back_to_raw() {
        // "oo" → "ô", "r" → "ổ", second "r" would undo tone → "ôr",
        // but 'ô' is a Vietnamese char and "ôr" is not valid → falls back to "oorr".
        let mut e = telex();
        process_str(&mut e, "oorr");
        assert_eq!(e.preedit().as_str(), "oorr");
    }

    // ── Continuation after double tone-key undo ──────────────────────────────

    #[test]
    fn telex_perrm_gives_perm() {
        // "perr" → "per" (undo), then 'm' must append → "perm", not "perrm".
        let mut e = telex();
        process_str(&mut e, "perrm");
        assert_eq!(e.preedit().as_str(), "perm");
    }

    #[test]
    fn telex_errm_gives_erm() {
        let mut e = telex();
        process_str(&mut e, "errm");
        assert_eq!(e.preedit().as_str(), "erm");
    }

    #[test]
    fn telex_assm_gives_asm() {
        // "ass" → "as", then 'm' → "asm", not "assm"
        let mut e = telex();
        process_str(&mut e, "assm");
        assert_eq!(e.preedit().as_str(), "asm");
    }

    // ── Intentional: form marks without tone do not re-enter composition ─────

    #[test]
    fn backspace_on_surrounding_breve_a_passes_through() {
        // intentional: form marks without tone do not re-enter composition
        // 'ă' (U+0103) carries only a breve diacritic, not a tone mark.
        // vietnamese_base_char('ă') returns None because NFD-decomposition of 'ă'
        // contains only U+0306 (breve, not in the tone-mark set), so nothing is
        // stripped.  Backspace on an empty buffer passes through and the
        // application deletes 'ă' directly rather than re-entering composition.
        let mut e = telex();
        let text = "ă".to_owned(); // U+0103, 2 UTF-8 bytes
        let cursor = text.len();
        e.process(&InputEvent::SurroundingText { text, cursor }).unwrap();
        let t = e.process(&backspace()).unwrap();
        assert_eq!(t, StateTransition::PassThrough);
    }
}
