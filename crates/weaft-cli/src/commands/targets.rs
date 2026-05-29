//! `weaft targets` — print the host capability matrix.

use std::process::ExitCode;
use weaft_core::capability::{self, AgentLayout};

pub fn run() -> miette::Result<ExitCode> {
    println!(
        "{}",
        row("ID", "NAME", "SUBAGENTS", "ASSETS", "TOOLS", "BUDGET")
    );
    for host in capability::all() {
        let subagents = match host.agent_layout {
            AgentLayout::None => "no".to_string(),
            _ => format!("yes ({})", host.subagent_tool.unwrap_or("?")),
        };
        let budget = host
            .max_skill_tokens
            .map(|b| b.to_string())
            .unwrap_or_else(|| "—".to_string());
        println!(
            "{}",
            row(
                host.id,
                host.display_name,
                &subagents,
                yn(host.supports_assets),
                yn(host.supports_tool_allowlist),
                &budget,
            )
        );
    }
    println!("\nbudgets are weaft heuristics; token counts use approximate tokenizers.");
    Ok(ExitCode::SUCCESS)
}

fn row(id: &str, name: &str, sub: &str, assets: &str, tools: &str, budget: &str) -> String {
    format!("{id:<12} {name:<20} {sub:<11} {assets:<8} {tools:<8} {budget}")
}

fn yn(b: bool) -> &'static str {
    if b { "yes" } else { "no" }
}
