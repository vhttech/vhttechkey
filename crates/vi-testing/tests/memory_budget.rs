use vi_core::{InputMethod, StandardEngine};
use vi_testing::memory::idle_rss_kb;

#[test]
fn engine_idle_rss_within_30mb() {
    let _engine = StandardEngine::new(InputMethod::Telex);
    let rss = idle_rss_kb();
    assert!(
        rss <= 30_720,
        "idle RSS {rss} KB exceeds 30 MB (30720 KB) budget"
    );
}
