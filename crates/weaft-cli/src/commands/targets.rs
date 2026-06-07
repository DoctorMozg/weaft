//! `weaft targets` — print the host capability matrix.
//!
//! Two views are printed. The first is the v1 host-wide feature table (one row per host). The
//! second is the v2 per-kind **disposition** matrix (WU-20): for every `(host, kind)` cell it
//! shows whether the host emits a real file (`native`), folds the kind into another's output
//! (`fold→<kind>`), or drops it (`drop`) — the data that drives resolve/emit. A cell carrying a
//! budget is tagged with its unit (`·tok` / `·byte`) so the budget *axis* is visible per kind.
//!
//! The honesty footer (budgets are weaft heuristics; token counts are approximate) is preserved
//! verbatim per C-BUDGET-HONESTY — it is a stated repo invariant, extended here to the byte axis.

use std::process::ExitCode;
use weaft_core::budget::BudgetUnit;
use weaft_core::capability::{self, AgentLayout, AskUserSupport, Disposition, KindCapabilities};
use weaft_core::kind::ArtifactKind;

pub fn run() -> miette::Result<ExitCode> {
    print_host_features();
    println!();
    print_kind_dispositions();
    println!(
        "\nbudgets are weaft heuristics; token counts use approximate tokenizers (·tok = token \
         budget, ·byte = hard byte limit)."
    );
    Ok(ExitCode::SUCCESS)
}

/// The v1 host-wide feature table: one row per host with the cross-kind capability flags.
fn print_host_features() {
    println!(
        "{}",
        feature_row([
            "ID",
            "NAME",
            "SUBAGENTS",
            "ASSETS",
            "TOOLS",
            "ASK",
            "BUDGET"
        ])
    );
    for host in capability::all() {
        let subagents = match (host.agent_layout, host.subagent_tool) {
            (AgentLayout::None, _) => "no".to_string(),
            (_, Some(tool)) => format!("yes ({tool})"),
            (_, None) => "yes".to_string(),
        };
        let budget = host
            .max_skill_tokens
            .map_or_else(|| "—".to_string(), |b| b.to_string());
        let ask = match host.ask_user_support {
            AskUserSupport::Structured => "structured",
            AskUserSupport::NonBlocking => "non-blocking",
            AskUserSupport::None => "no",
        };
        println!(
            "{}",
            feature_row([
                host.id,
                host.display_name,
                &subagents,
                yn(host.supports_assets),
                yn(host.supports_tool_allowlist),
                ask,
                &budget,
            ])
        );
    }
}

/// The v2 per-kind disposition matrix: rows are artifact kinds, columns are hosts, each cell is
/// the host's disposition for that kind (`native` / `fold→<kind>` / `drop`), budget-unit tagged.
fn print_kind_dispositions() {
    println!("per-kind disposition (native / fold→<kind> / drop):");
    print!("{:<12}", "KIND");
    for host in capability::all() {
        print!(" {:<13}", host.id);
    }
    println!();
    for kind in ArtifactKind::all() {
        print!("{:<12}", kind.serde_name());
        for host in capability::all() {
            print!(" {:<13}", disposition_cell(host.kinds.get(*kind)));
        }
        println!();
    }
}

/// Render one `(host, kind)` cell as a compact disposition token, tagged with the budget unit
/// when the cell carries a budget (so the budget axis is visible per kind, C-BUDGET-HONESTY).
fn disposition_cell(cell: &KindCapabilities) -> String {
    let disposition = match cell.disposition {
        Disposition::Native => "native".to_string(),
        Disposition::Fold { into } => format!("fold→{}", into.serde_name()),
        Disposition::Drop => "drop".to_string(),
    };
    match cell.budget.map(|b| b.unit) {
        Some(BudgetUnit::Tokens(_)) => format!("{disposition}·tok"),
        Some(BudgetUnit::Bytes) => format!("{disposition}·byte"),
        None => disposition,
    }
}

fn feature_row(cols: [&str; 7]) -> String {
    let [id, name, sub, assets, tools, ask, budget] = cols;
    format!("{id:<12} {name:<20} {sub:<11} {assets:<8} {tools:<8} {ask:<13} {budget}")
}

fn yn(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}
