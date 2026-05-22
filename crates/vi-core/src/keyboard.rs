use crate::{
    error::CompositionError,
    types::{InputEvent, Key, Modifiers},
};

/// Abstract representation of a key event after layout resolution.
///
/// The concrete xkbcommon integration lives in `vi-wayland` and `vi-x11`.
/// `vi-core` only depends on this trait so the composition engine remains
/// platform-free.
pub trait KeyboardLayout: Send + Sync {
    /// Resolve a raw platform keysym + modifier state into a logical `InputEvent`.
    fn resolve(
        &self,
        keysym: u32,
        modifiers: Modifiers,
        pressed: bool,
    ) -> Result<InputEvent, CompositionError>;
}

/// A layout that treats every keysym as a direct Unicode codepoint.
///
/// Useful for testing and for platforms that already do keysym→char mapping
/// before calling vi-core.
#[derive(Debug, Default)]
pub struct PassThroughLayout;

impl KeyboardLayout for PassThroughLayout {
    fn resolve(
        &self,
        keysym: u32,
        modifiers: Modifiers,
        pressed: bool,
    ) -> Result<InputEvent, CompositionError> {
        let key = keysym_to_key(keysym)?;
        if pressed {
            Ok(InputEvent::KeyDown(key, modifiers))
        } else {
            Ok(InputEvent::KeyUp(key))
        }
    }
}

fn keysym_to_key(keysym: u32) -> Result<Key, CompositionError> {
    match keysym {
        0xFF08 => Ok(Key::Backspace),
        0xFF09 => Ok(Key::Tab),
        0xFF0D => Ok(Key::Return),
        0xFF1B => Ok(Key::Escape),
        0xFF51 => Ok(Key::Left),
        0xFF52 => Ok(Key::Up),
        0xFF53 => Ok(Key::Right),
        0xFF54 => Ok(Key::Down),
        0xFF50 => Ok(Key::Home),
        0xFF57 => Ok(Key::End),
        0xFFFF => Ok(Key::Delete),
        // Compose / Multi_key
        0xFF20 => Ok(Key::ComposeKey),
        // X11 dead keys (XK_dead_* range 0xfe50–0xfe63).
        // The char identifier follows VIQR/ASCII conventions so the engine's
        // combination table can use the same identifiers across keysym sources.
        0xfe50 => Ok(Key::DeadKey('`')),  // dead_grave
        0xfe51 => Ok(Key::DeadKey('\'')), // dead_acute
        0xfe52 => Ok(Key::DeadKey('^')),  // dead_circumflex → â ê ô
        0xfe53 => Ok(Key::DeadKey('~')),  // dead_tilde
        0xfe55 => Ok(Key::DeadKey('(')),  // dead_breve → ă
        0xfe60 => Ok(Key::DeadKey('.')),  // dead_belowdot → nặng
        0xfe61 => Ok(Key::DeadKey('?')),  // dead_hook → hỏi
        0xfe62 => Ok(Key::DeadKey('+')),  // dead_horn → ơ ư
        0xfe63 => Ok(Key::DeadKey('d')),  // dead_stroke → đ
        // Unicode range: XKB uses 0x01000000 + Unicode codepoint for printable chars.
        k if k >= 0x01000000 => {
            let cp = k - 0x01000000;
            char::from_u32(cp)
                .map(Key::Char)
                .ok_or(CompositionError::UnknownKey(format!("{k:#010x}")))
        }
        // Latin-1 printable range maps directly.
        k if (0x0020..=0x007E).contains(&k) || (0x00A0..=0x00FF).contains(&k) => char::from_u32(k)
            .map(Key::Char)
            .ok_or(CompositionError::UnknownKey(format!("{k:#010x}"))),
        k => Err(CompositionError::UnknownKey(format!("{k:#010x}"))),
    }
}

/// Try to combine a dead key with the following base character.
///
/// Returns `Some(composed)` when the pair is a recognized Vietnamese
/// combination, or `None` when they do not combine (the caller should then
/// emit the dead key character as a literal and process the base character
/// through the normal input-method rules).
///
/// Dead key identifiers follow VIQR/ASCII conventions:
/// - `'^'` = circumflex (dead_circumflex)
/// - `'('` = breve (dead_breve)
/// - `'+'` = horn (dead_horn)
/// - `'d'` / `'D'` = stroke (dead_stroke)
pub(crate) fn combine_dead_key(dead: char, next: char) -> Option<char> {
    Some(match (dead, next) {
        // Circumflex (^): â ê ô
        ('^', 'a') => 'â',
        ('^', 'A') => 'Â',
        ('^', 'e') => 'ê',
        ('^', 'E') => 'Ê',
        ('^', 'o') => 'ô',
        ('^', 'O') => 'Ô',
        // Breve ((): ă
        ('(', 'a') => 'ă',
        ('(', 'A') => 'Ă',
        // Horn (+): ơ ư
        ('+', 'o') => 'ơ',
        ('+', 'O') => 'Ơ',
        ('+', 'u') => 'ư',
        ('+', 'U') => 'Ư',
        // Stroke (d): đ — either case of the dead identifier combines
        ('d', 'd') | ('D', 'd') => 'đ',
        ('d', 'D') | ('D', 'D') => 'Đ',
        _ => return None,
    })
}
