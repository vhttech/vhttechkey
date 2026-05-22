//! Deprecated: use the vime-* equivalent crate instead.
//! Shared testing utilities for the vi-ime project.

pub mod compositor_matrix;
pub mod cursor_regression;
pub mod fixtures;
pub mod focus_switch;
pub mod golden;
pub mod integration;
pub mod memory;
pub mod mock_compositor;
pub mod mock_ibus;
pub mod replay;
pub mod stress;
pub mod unicode_ext;
pub mod unicode_torture;

pub use golden::{type_and_commit, GoldenCase};
pub use integration::{IntegrationHarness, MockBackend};
pub use replay::{replay_session, ReplaySession};
