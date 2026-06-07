//! Two-axis budget model: unit (tokens | bytes) × severity (soft | hard) (WU-3).
//!
//! A [`Budget`] is measured along two independent axes:
//! - **unit** — token budgets count the *rendered body* with a named tokenizer (kept inside
//!   [`BudgetUnit::Tokens`] so they stay labeled approximate at the type level,
//!   C-BUDGET-HONESTY); byte budgets count the *final emitted file* size.
//! - **severity** — a `Soft` budget is advisory (only blocks under `--strict`); a `Hard`
//!   budget always blocks, modeling a host that silently truncates past the cap.
//!
//! The two `check_*` methods deliberately split along the crate boundary: core's lint calls
//! [`Budget::check_tokens`] on the rendered body, while the byte branch
//! ([`Budget::check_bytes`]) is invoked from `weaft-cli` where the merged file size is known.

use crate::capability::Tokenizer;
use crate::tokens;
use serde::Serialize;

/// The axis a budget is measured along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetUnit {
    /// Count tokens in the rendered body with this (approximate) tokenizer.
    Tokens(Tokenizer),
    /// Count bytes of the final emitted file.
    Bytes,
}

/// How hard a budget bites when exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetSeverity {
    /// Advisory: only blocks the build under `--strict`.
    Soft,
    /// Always blocks: the host truncates silently past the cap.
    Hard,
}

/// A single budget cell on the capability matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Budget {
    pub limit: usize,
    pub unit: BudgetUnit,
    pub severity: BudgetSeverity,
    /// Documented origin of the limit, e.g. `"weaft heuristic"` or an ADR citation.
    pub source: &'static str,
}

impl Budget {
    /// Check a rendered body against a token budget. Returns `None` (no opinion) unless the
    /// unit is [`BudgetUnit::Tokens`] — byte budgets are checked via [`Budget::check_bytes`].
    #[must_use]
    pub fn check_tokens(&self, body: &str) -> Option<BudgetViolation> {
        let BudgetUnit::Tokens(tokenizer) = self.unit else {
            return None;
        };
        let actual = tokens::count(body, tokenizer);
        self.violation(actual)
    }

    /// Check a final emitted file size against a byte budget. Returns `None` unless the unit
    /// is [`BudgetUnit::Bytes`] — token budgets are checked via [`Budget::check_tokens`].
    #[must_use]
    pub fn check_bytes(&self, emitted_bytes: usize) -> Option<BudgetViolation> {
        if self.unit != BudgetUnit::Bytes {
            return None;
        }
        self.violation(emitted_bytes)
    }

    /// Build a violation iff `actual` is strictly over the limit (exactly at the limit is
    /// within budget). Carries the cell's unit, severity, and source verbatim.
    fn violation(&self, actual: usize) -> Option<BudgetViolation> {
        let over_by = actual.checked_sub(self.limit)?;
        if over_by == 0 {
            return None;
        }
        Some(BudgetViolation {
            over_by,
            limit: self.limit,
            actual,
            unit: self.unit,
            severity: self.severity,
            source: self.source,
        })
    }
}

/// A budget that was exceeded, carrying enough context to report it without re-deriving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BudgetViolation {
    pub over_by: usize,
    pub limit: usize,
    pub actual: usize,
    pub unit: BudgetUnit,
    pub severity: BudgetSeverity,
    pub source: &'static str,
}

impl BudgetViolation {
    /// Whether this violation should fail the build: a `Hard` violation always blocks; a
    /// `Soft` one blocks only when the caller passes `strict`.
    #[must_use]
    pub fn is_blocking(&self, strict: bool) -> bool {
        self.severity == BudgetSeverity::Hard || strict
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // `Tokenizer` already exists in the capability matrix — token budgets carry it so they
    // stay labeled approximate at the type level (C-BUDGET-HONESTY).
    use crate::capability::Tokenizer;

    /// A soft token budget the rendered body can overflow. `source` records the documented
    /// origin of the limit (here, a weaft heuristic).
    fn soft_token_budget() -> Budget {
        Budget {
            limit: 8_000,
            unit: BudgetUnit::Tokens(Tokenizer::Cl100kBase),
            severity: BudgetSeverity::Soft,
            source: "weaft heuristic",
        }
    }

    /// The one concrete hard-byte cell ADR-0005 requires: Codex's merged AGENTS.md
    /// silently truncates past ~32 KiB.
    fn hard_byte_budget() -> Budget {
        Budget {
            limit: 32_768,
            unit: BudgetUnit::Bytes,
            severity: BudgetSeverity::Hard,
            source: "Codex AGENTS.md ~32 KiB cap (ADR-0005)",
        }
    }

    #[test]
    fn soft_overflow_is_blocking_only_under_strict() {
        // A soft violation is advisory by default; --strict promotes it to blocking.
        let violation = BudgetViolation {
            over_by: 100,
            limit: 8_000,
            actual: 8_100,
            unit: BudgetUnit::Tokens(Tokenizer::Cl100kBase),
            severity: BudgetSeverity::Soft,
            source: "weaft heuristic",
        };
        assert!(
            !violation.is_blocking(false),
            "soft overflow must not block a non-strict run",
        );
        assert!(
            violation.is_blocking(true),
            "soft overflow must block under --strict",
        );
    }

    #[test]
    fn hard_overflow_is_always_blocking() {
        // A hard violation blocks regardless of strictness (the host truncates silently).
        let violation = BudgetViolation {
            over_by: 7_232,
            limit: 32_768,
            actual: 40_000,
            unit: BudgetUnit::Bytes,
            severity: BudgetSeverity::Hard,
            source: "Codex AGENTS.md ~32 KiB cap (ADR-0005)",
        };
        assert!(
            violation.is_blocking(false),
            "hard overflow must block even without --strict",
        );
        assert!(
            violation.is_blocking(true),
            "hard overflow must also block under --strict",
        );
    }

    #[test]
    fn check_tokens_under_budget_is_none() {
        // A tiny body cannot exceed an 8000-token soft limit.
        assert!(
            soft_token_budget()
                .check_tokens("just a few words")
                .is_none()
        );
    }

    #[test]
    fn check_bytes_under_budget_is_none() {
        assert!(hard_byte_budget().check_bytes(1_024).is_none());
    }

    #[test]
    fn check_bytes_reports_exact_overflow() {
        // 40_000 against a 32_768 hard-byte cap overflows by exactly 7_232 bytes, and the
        // resulting violation carries the cell's severity, unit, and source verbatim.
        let budget = hard_byte_budget();
        let violation = budget
            .check_bytes(40_000)
            .expect("40_000 bytes overflows the 32_768 hard cap");
        assert_eq!(violation.over_by, 7_232);
        assert_eq!(violation.limit, 32_768);
        assert_eq!(violation.actual, 40_000);
        assert_eq!(violation.severity, BudgetSeverity::Hard);
        assert!(
            violation.is_blocking(false),
            "the reported byte violation must be blocking",
        );
    }

    #[test]
    fn check_bytes_at_the_limit_is_none() {
        // Exactly at the limit is not an overflow (boundary).
        assert!(hard_byte_budget().check_bytes(32_768).is_none());
    }
}
