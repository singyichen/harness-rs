# harness — Engineering-discipline engine for Claude Code

A single Rust binary that establishes behavioral baselines for Claude Code:
evidence before claims, explicit assumptions, adversarial review for major
decisions, and test-verified code changes. Install once, effective across
all your projects; each project can layer its own customizations on top.

[繁體中文說明](README.zh-TW.md)

## Install

```bash
cargo install --path .
harness install    # release assets + register hooks + install agents/skills
harness doctor     # health check
```

## Commands

| Command | Purpose |
|---|---|
| `harness install` | Release assets into ~/.claude/ and register hooks |
| `harness uninstall` | Clean removal (only harness-owned items; your customizations and global config are kept) |
| `harness init` | Generate a harness.toml customization layer in the current project |
| `harness doctor` | Health check: hook registration, version consistency, gate conflicts |
| `harness update` | Re-release assets after an upgrade (your modified files are kept; official copies land in .new files) |
| `harness config` | Print the merged effective config (built-in → global → project) |
| `harness hook <event>` | Hook engine entry point (invoked by Claude Code; not for manual use) |

## The verify gate

Changed code without running tests afterwards? At the end of the turn,
the gate reacts according to its mode:

- `strict`: blocks, demanding tests (or an explanation to the user; the
  second stop attempt passes)
- `advisory` (default): warns but allows
- `off`: no check

Drop a `harness.toml` in a project root to override anything
(`harness init` generates the template):

```toml
[gates.verify]
mode = "strict"
test_commands = ["cargo test"]
```

## Adversarial review

Five agent lenses (skeptic / red-team / simplifier / evidence-auditor /
user-advocate) plus an `adversarial-review` skill: major conclusions are
adopted only when a majority of the panel lets them survive. Panel
membership is set via `[review].panel`.

## Design principles

- **Fail-open**: any internal error in the hook engine allows the action —
  it never breaks your session.
- **settings.json safety**: timestamped backups, append-only merging,
  atomic writes, unknown fields preserved.
- **Your customizations win**: install/update/uninstall never overwrite or
  delete files you have modified.
