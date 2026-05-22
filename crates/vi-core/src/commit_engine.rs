use crate::{
    error::CompositionError,
    types::{CommittedText, PreeditText},
    unicode_pipeline::UnicodePipeline,
};

pub struct CommitEngine;

impl CommitEngine {
    /// Validate and normalize `preedit` to NFC, producing a `CommittedText`.
    pub fn commit(preedit: &PreeditText) -> Result<CommittedText, CompositionError> {
        let nfc = UnicodePipeline::process(preedit.as_str())?;
        Ok(CommittedText::new(nfc))
    }

    /// Commit a raw string directly (e.g., pass-through keys).
    pub fn commit_str(s: &str) -> Result<CommittedText, CompositionError> {
        let nfc = UnicodePipeline::process(s)?;
        Ok(CommittedText::new(nfc))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn plain_ascii_commits() {
        let pt = PreeditText::new("hello".into());
        let ct = CommitEngine::commit(&pt).unwrap();
        assert_eq!(ct.as_str(), "hello");
    }

    #[test]
    fn vietnamese_nfc_committed() {
        let pt = PreeditText::new("tôi".into());
        let ct = CommitEngine::commit(&pt).unwrap();
        // Already NFC — should be unchanged.
        assert_eq!(ct.as_str(), "tôi");
    }

    #[test]
    fn double_encoded_is_fixed_on_commit() {
        // NFD form of "tôi": t + o + U+0302 + i
        let nfd = "to\u{0302}i";
        let ct = CommitEngine::commit_str(nfd).unwrap();
        assert_eq!(ct.as_str(), "tôi");
    }
}
