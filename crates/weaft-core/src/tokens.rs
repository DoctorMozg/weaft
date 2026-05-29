//! Token counting per target tokenizer.
//!
//! BPE tables are loaded lazily and cached, since `tiktoken-rs` init costs ~100ms.
//!
//! **Honesty note:** Claude's real tokenizer is not public. `Cl100kBase` is a close
//! approximation (within ~5–10% for typical English+code). `GeminiApprox` reuses the
//! cl100k tables as a rough estimate. These counts are guidance, not billing-accurate.

use crate::capability::Tokenizer;
use once_cell::sync::Lazy;
use tiktoken_rs::{CoreBPE, cl100k_base, o200k_base};

static CL100K: Lazy<CoreBPE> = Lazy::new(|| cl100k_base().expect("cl100k_base BPE table"));
static O200K: Lazy<CoreBPE> = Lazy::new(|| o200k_base().expect("o200k_base BPE table"));

/// Count tokens in `text` using the tokenizer for a target.
pub fn count(text: &str, tk: Tokenizer) -> usize {
    let bpe: &CoreBPE = match tk {
        Tokenizer::Cl100kBase | Tokenizer::GeminiApprox => &CL100K,
        Tokenizer::O200kBase => &O200K,
    };
    bpe.encode_with_special_tokens(text).len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_is_zero() {
        assert_eq!(count("", Tokenizer::Cl100kBase), 0);
    }

    #[test]
    fn counts_are_positive_for_text() {
        assert!(count("Hello, world!", Tokenizer::Cl100kBase) > 0);
        assert!(count("Hello, world!", Tokenizer::O200kBase) > 0);
    }
}
