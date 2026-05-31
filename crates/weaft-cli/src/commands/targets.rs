//! `weaft targets` — print the host capability matrix.

use std::process::ExitCode;
use weaft_core::capability::{self, AgentLayout, AskUserSupport};

pub fn run() -> miette::Result<ExitCode> {
    println!(
        "{}",
        row([
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
            row([
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
    println!("\nbudgets are weaft heuristics; token counts use approximate tokenizers.");
    Ok(ExitCode::SUCCESS)
}

fn row(cols: [&str; 7]) -> String {
    let [id, name, sub, assets, tools, ask, budget] = cols;
    format!("{id:<12} {name:<20} {sub:<11} {assets:<8} {tools:<8} {ask:<13} {budget}")
}

fn yn(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}
