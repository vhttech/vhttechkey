//! 216 golden test cases: every Vietnamese vowel × every tone × 3 input methods.
//!
//! These are spec tests — they document the *expected* behaviour of the full
//! composition stack. Cases that currently fail indicate engine bugs to fix.
#![allow(clippy::unwrap_used)]

use unicode_normalization::UnicodeNormalization;
use vi_testing::golden::{all_216, type_and_commit};

/// Run all 216 cases and collect failures rather than stopping at the first one
/// so the test output lists every broken combination at once.
#[test]
fn golden_216_all_methods() {
    let cases = all_216();
    assert_eq!(cases.len(), 216, "expected exactly 216 cases");

    let mut failures: Vec<String> = Vec::new();

    for case in &cases {
        let actual = type_and_commit(case.method, &case.input);

        // Every committed string must be valid NFC.
        let renfc: String = actual.nfc().collect();
        if actual != renfc {
            failures.push(format!(
                "{}: input={:?} committed output is not NFC — got={:?}",
                case.description, case.input, actual
            ));
        }

        if actual != case.expected {
            failures.push(format!(
                "{}: input={:?} expected={:?} got={:?}",
                case.description, case.input, case.expected, actual
            ));
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} golden case(s) failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

/// Spot-check a few canonical single-vowel + single-tone combinations that are
/// known to work through the engine (no compound vowel involved).
#[test]
fn spot_check_base_vowel_tones() {
    use vi_core::InputMethod;
    let cases: &[(InputMethod, &str, &str)] = &[
        // Telex – simple vowel + tone marker
        (InputMethod::Telex, "af", "à"),
        (InputMethod::Telex, "as", "á"),
        (InputMethod::Telex, "ar", "ả"),
        (InputMethod::Telex, "ax", "ã"),
        (InputMethod::Telex, "aj", "ạ"),
        (InputMethod::Telex, "ef", "è"),
        (InputMethod::Telex, "es", "é"),
        (InputMethod::Telex, "if", "ì"),
        (InputMethod::Telex, "is", "í"),
        (InputMethod::Telex, "of", "ò"),
        (InputMethod::Telex, "os", "ó"),
        (InputMethod::Telex, "uf", "ù"),
        (InputMethod::Telex, "us", "ú"),
        (InputMethod::Telex, "yf", "ỳ"),
        (InputMethod::Telex, "ys", "ý"),
        // VNI – simple vowel + digit
        (InputMethod::Vni, "a2", "à"),
        (InputMethod::Vni, "a1", "á"),
        (InputMethod::Vni, "a3", "ả"),
        (InputMethod::Vni, "a4", "ã"),
        (InputMethod::Vni, "a5", "ạ"),
        (InputMethod::Vni, "e2", "è"),
        (InputMethod::Vni, "i1", "í"),
        (InputMethod::Vni, "o2", "ò"),
        (InputMethod::Vni, "u1", "ú"),
        (InputMethod::Vni, "y2", "ỳ"),
        // VIQR – simple vowel + punctuation
        (InputMethod::Viqr, "a`", "à"),
        (InputMethod::Viqr, "a'", "á"),
        (InputMethod::Viqr, "a?", "ả"),
        (InputMethod::Viqr, "a~", "ã"),
        (InputMethod::Viqr, "a.", "ạ"),
        (InputMethod::Viqr, "e`", "è"),
        (InputMethod::Viqr, "i'", "í"),
        (InputMethod::Viqr, "o`", "ò"),
        (InputMethod::Viqr, "u'", "ú"),
        (InputMethod::Viqr, "y'", "ý"),
    ];

    let mut failures: Vec<String> = Vec::new();
    for &(method, input, expected) in cases {
        let actual = type_and_commit(method, input);
        if actual != expected {
            failures.push(format!(
                "{method:?} {input:?}: expected {expected:?}, got {actual:?}"
            ));
        }
    }
    if !failures.is_empty() {
        panic!("spot check failures:\n{}", failures.join("\n"));
    }
}
