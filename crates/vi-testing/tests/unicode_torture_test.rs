//! Run all Unicode torture cases through `UnicodePipeline::process`.
#![allow(clippy::unwrap_used)]

use vi_core::UnicodePipeline;
use vi_testing::unicode_torture;

#[test]
fn all_torture_cases_pass() {
    let cases = unicode_torture::cases();
    assert!(!cases.is_empty(), "torture case list must not be empty");
    assert!(
        cases.len() >= 20,
        "need at least 20 cases, got {}",
        cases.len()
    );

    let mut failures: Vec<String> = Vec::new();

    for case in &cases {
        let result = UnicodePipeline::process(case.raw_input);

        match (result, case.expected_nfc) {
            (Ok(nfc), Some(expected)) => {
                if nfc.as_str() != expected {
                    failures.push(format!(
                        "WRONG OUTPUT [{}]: input={:?} expected={:?} got={:?}",
                        case.description,
                        case.raw_input,
                        expected,
                        nfc.as_str(),
                    ));
                }
            }
            (Err(_), None) => {
                // Expected error, got error — correct.
            }
            (Ok(nfc), None) => {
                failures.push(format!(
                    "SHOULD HAVE ERRORED [{}]: input={:?} but got Ok({:?})",
                    case.description,
                    case.raw_input,
                    nfc.as_str(),
                ));
            }
            (Err(e), Some(expected)) => {
                failures.push(format!(
                    "UNEXPECTED ERROR [{}]: input={:?} expected Ok({:?}) but got Err({e})",
                    case.description, case.raw_input, expected,
                ));
            }
        }
    }

    if !failures.is_empty() {
        panic!(
            "{} torture case(s) failed:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
