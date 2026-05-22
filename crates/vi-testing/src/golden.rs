//! Helpers for golden / expected-output test cases.

use vi_core::{
    CompositionEngine, InputEvent, InputMethod, Key, Modifiers, StandardEngine, StateTransition,
};

/// Run `keys` through `method` and return the committed text.
///
/// Each character in `keys` is sent as a `KeyDown` event, then a `Return`
/// event commits whatever is in the preedit buffer.
pub fn type_and_commit(method: InputMethod, keys: &str) -> String {
    let mut engine = StandardEngine::new(method);
    for ch in keys.chars() {
        engine
            .process(&InputEvent::KeyDown(Key::Char(ch), Modifiers::none()))
            .expect("engine must not fail on valid input");
        let _ = engine.process(&InputEvent::KeyUp(Key::Char(ch)));
    }
    match engine
        .process(&InputEvent::KeyDown(Key::Return, Modifiers::none()))
        .expect("return must not fail")
    {
        StateTransition::CommitAndClear(c) => c.as_str().to_owned(),
        StateTransition::Commit(c) => c.as_str().to_owned(),
        StateTransition::PassThrough => String::new(),
        other => panic!("unexpected transition on Return: {other:?}"),
    }
}

/// One entry in the 216 golden test matrix.
pub struct GoldenCase {
    pub method: InputMethod,
    pub input: String,
    pub expected: &'static str,
    pub description: String,
}

/// Generate all 216 golden cases: 12 vowels × 6 tones × 3 input methods.
///
/// Each case specifies the key sequence and the expected committed character.
pub fn all_216() -> Vec<GoldenCase> {
    // (vowel_char, telex_vowel, vni_vowel, viqr_vowel)
    let vowels: &[(&str, &str, &str, &str)] = &[
        ("a", "a", "a", "a"),
        ("ă", "aw", "a8", "a("),
        ("â", "aa", "a6", "a^"),
        ("e", "e", "e", "e"),
        ("ê", "ee", "e6", "e^"),
        ("i", "i", "i", "i"),
        ("o", "o", "o", "o"),
        ("ô", "oo", "o6", "o^"),
        ("ơ", "ow", "o7", "o+"),
        ("u", "u", "u", "u"),
        ("ư", "uw", "u7", "u+"),
        ("y", "y", "y", "y"),
    ];

    struct ToneSpec {
        name: &'static str,
        telex: &'static str,
        vni: &'static str,
        viqr: &'static str,
        expected: [&'static str; 12],
    }

    let tones: &[ToneSpec] = &[
        ToneSpec {
            name: "flat",
            telex: "",
            vni: "",
            viqr: "",
            expected: ["a", "ă", "â", "e", "ê", "i", "o", "ô", "ơ", "u", "ư", "y"],
        },
        ToneSpec {
            name: "huyền",
            telex: "f",
            vni: "2",
            viqr: "`",
            expected: ["à", "ằ", "ầ", "è", "ề", "ì", "ò", "ồ", "ờ", "ù", "ừ", "ỳ"],
        },
        ToneSpec {
            name: "sắc",
            telex: "s",
            vni: "1",
            viqr: "'",
            expected: ["á", "ắ", "ấ", "é", "ế", "í", "ó", "ố", "ớ", "ú", "ứ", "ý"],
        },
        ToneSpec {
            name: "hỏi",
            telex: "r",
            vni: "3",
            viqr: "?",
            expected: ["ả", "ẳ", "ẩ", "ẻ", "ể", "ỉ", "ỏ", "ổ", "ở", "ủ", "ử", "ỷ"],
        },
        ToneSpec {
            name: "ngã",
            telex: "x",
            vni: "4",
            viqr: "~",
            expected: ["ã", "ẵ", "ẫ", "ẽ", "ễ", "ĩ", "õ", "ỗ", "ỡ", "ũ", "ữ", "ỹ"],
        },
        ToneSpec {
            name: "nặng",
            telex: "j",
            vni: "5",
            viqr: ".",
            expected: ["ạ", "ặ", "ậ", "ẹ", "ệ", "ị", "ọ", "ộ", "ợ", "ụ", "ự", "ỵ"],
        },
    ];

    let mut cases: Vec<GoldenCase> = Vec::with_capacity(216);

    for (vi, &(vowel_char, telex_v, vni_v, viqr_v)) in vowels.iter().enumerate() {
        for tone in tones.iter() {
            let expected = tone.expected[vi];

            cases.push(GoldenCase {
                method: InputMethod::Telex,
                input: format!("{}{}", telex_v, tone.telex),
                expected,
                description: format!("Telex {vowel_char}-{}", tone.name),
            });
            cases.push(GoldenCase {
                method: InputMethod::Vni,
                input: format!("{}{}", vni_v, tone.vni),
                expected,
                description: format!("VNI {vowel_char}-{}", tone.name),
            });
            cases.push(GoldenCase {
                method: InputMethod::Viqr,
                input: format!("{}{}", viqr_v, tone.viqr),
                expected,
                description: format!("VIQR {vowel_char}-{}", tone.name),
            });
        }
    }

    cases
}
