//! Full-stack integration tests: engine → MockBackend dispatch.

use vi_core::{InputEvent, InputMethod, Key, Modifiers};
use vi_testing::integration::IntegrationHarness;

fn key_down(c: char) -> InputEvent {
    InputEvent::KeyDown(Key::Char(c), Modifiers::none())
}

fn key_down_special(key: Key) -> InputEvent {
    InputEvent::KeyDown(key, Modifiers::none())
}

/// Replay all 20 built-in fixtures through the full engine→backend stack.
#[tokio::test]
async fn test_all_fixtures() {
    for fixture in vi_testing::fixtures::all_fixtures() {
        let h = IntegrationHarness::new(fixture.method);
        for event in &fixture.events {
            h.send_event(event.clone()).await;
        }
        assert_eq!(
            h.committed().join(""),
            fixture.expected_commits.join(""),
            "fixture {:?} failed",
            fixture.name
        );
    }
}

/// Committed text must always be a single NFC codepoint, not decomposed form.
#[tokio::test]
async fn test_nfc_invariant() {
    // 'a' + 'f' in Telex → à (U+00E0), not U+0061 U+0300 (a + combining grave)
    let h = IntegrationHarness::new(InputMethod::Telex);
    h.send_event(key_down('a')).await;
    h.send_event(key_down('f')).await;
    h.send_event(InputEvent::FocusOut).await;
    let commits = h.committed();
    let commit = &commits[0];
    assert_eq!(commit.chars().count(), 1);
    assert_eq!(commit.chars().next().unwrap() as u32, 0x00E0);
}

/// Key repeat must not commit once per keystroke — only one commit on flush.
#[tokio::test]
async fn test_key_repeat_no_duplicate_commit() {
    let h = IntegrationHarness::new(InputMethod::Telex);
    for _ in 0..10 {
        h.send_event(InputEvent::KeyRepeat(Key::Char('a'), Modifiers::default()))
            .await;
    }
    h.send_event(InputEvent::FocusOut).await;
    assert_eq!(h.committed().len(), 1);
}

/// Three backspaces after three characters must empty the preedit with no commit.
#[tokio::test]
async fn test_backspace_full_rollback() {
    let h = IntegrationHarness::new(InputMethod::Telex);
    for c in ['t', 'o', 'i'] {
        h.send_event(key_down(c)).await;
    }
    for _ in 0..3 {
        h.send_event(key_down_special(Key::Backspace)).await;
    }
    assert_eq!(h.preedit(), "");
    assert!(h.committed().is_empty());
}

/// Rapid FocusIn / FocusOut cycles must not panic or lose commits.
#[tokio::test]
async fn test_focus_cycling() {
    let h = IntegrationHarness::new(InputMethod::Telex);
    for _ in 0..50 {
        h.send_event(InputEvent::FocusIn).await;
        h.send_event(key_down('a')).await;
        h.send_event(InputEvent::FocusOut).await;
    }
    assert_eq!(h.committed().len(), 50);
}

/// Switching input method mid-composition must clear the preedit buffer.
#[tokio::test]
async fn test_method_switch_clears_preedit() {
    let h = IntegrationHarness::new(InputMethod::Telex);
    h.send_event(key_down('t')).await;
    h.send_event(key_down('o')).await;
    h.engine.lock().unwrap().set_method(InputMethod::Vni);
    assert_eq!(h.preedit(), "");
}
