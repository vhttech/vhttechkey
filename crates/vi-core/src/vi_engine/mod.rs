//! **vhttechkey composition engine** — triển khai thuần Rust nội bộ.
//!
//! Module này triển khai lớp xử lý ký tự tiếng Việt cho `StandardEngine`:
//! parse luật Telex/VNI/VIQR, flatten (NFC), kiểm tra chính tả CVC, và `ViEngine`.
//! Không phụ thuộc thư viện ngoài nào ngoài `regex` (luật DSL).

pub(crate) mod compose;
pub(crate) mod engine;
pub(crate) mod flat;
pub(crate) mod rules;
pub(crate) mod spell;
pub(crate) mod text;
pub(crate) mod types;

pub use engine::ViEngine;
