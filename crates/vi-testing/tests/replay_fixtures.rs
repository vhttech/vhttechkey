//! Regression tests: replay the 20 built-in fixture sessions and assert
//! identical committed text.  The sessions also round-trip through bincode to
//! exercise the `.vime-replay` serialization format.
#![allow(clippy::unwrap_used)]

use vi_testing::fixtures::all_fixtures;
use vi_testing::replay::{assert_replay, ReplaySession};

/// Every fixture replays deterministically.
#[test]
fn all_fixtures_replay_deterministically() {
    for session in all_fixtures() {
        assert_replay(&session);
    }
}

/// Every fixture survives a bincode round-trip (serialize → deserialize →
/// replay) and still produces the expected output.
#[test]
fn fixtures_bincode_round_trip() {
    for session in all_fixtures() {
        let bytes = session.to_bytes();
        let restored = ReplaySession::from_bytes(&bytes).expect("bincode round-trip must succeed");
        assert_eq!(session.name, restored.name);
        assert_replay(&restored);
    }
}

/// All 20 fixtures are present.
#[test]
fn exactly_20_fixtures() {
    assert_eq!(all_fixtures().len(), 20);
}
