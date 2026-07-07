pub struct Asset {
    /// Target path relative to ~/.claude/
    pub rel_path: &'static str,
    pub content: &'static str,
}

pub const ASSETS: &[Asset] = &[
    Asset {
        rel_path: "harness/protocol.md",
        content: include_str!("../assets/protocol.md"),
    },
    Asset {
        rel_path: "agents/skeptic.md",
        content: include_str!("../assets/agents/skeptic.md"),
    },
    Asset {
        rel_path: "agents/red-team.md",
        content: include_str!("../assets/agents/red-team.md"),
    },
    Asset {
        rel_path: "agents/simplifier.md",
        content: include_str!("../assets/agents/simplifier.md"),
    },
    Asset {
        rel_path: "agents/evidence-auditor.md",
        content: include_str!("../assets/agents/evidence-auditor.md"),
    },
    Asset {
        rel_path: "agents/user-advocate.md",
        content: include_str!("../assets/agents/user-advocate.md"),
    },
    Asset {
        rel_path: "skills/adversarial-review/SKILL.md",
        content: include_str!("../assets/skills/adversarial-review/SKILL.md"),
    },
];

pub const DEFAULT_CONFIG: &str = include_str!("../assets/default-config.toml");
