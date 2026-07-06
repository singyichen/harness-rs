# Harness Engineer — Design Document

Date: 2026-07-06
Status: Finalized (user-approved)
Reference: <https://github.com/Miguok/fable-harness> (benchmark to improve upon)

## 1. Goal and positioning

Harness Engineer is an "engineering-discipline engine" for Claude Code written
in Rust: a CLI binary named `harness` that, once installed, applies to every
project the user works on, while allowing each project to layer its own
customizations on top.

It does not teach the model new capabilities; it establishes behavioral
baselines: evidence before claims, explicit assumptions, adversarial review
for major decisions, and test-verified code changes.

### Four improvements over fable-harness

| fable-harness problem | Our solution |
|---|---|
| Hooks are shell/Python scripts referencing the repo by absolute path; the repo cannot move | Single Rust binary on PATH after `cargo install`; the source repo can be deleted |
| Installation relies on "ask Claude to edit settings.json per INSTALL.md" — fragile and risky | `harness install` does it programmatically: timestamped backups, merge-only (never overwrite), temp+rename atomic writes |
| Discipline rules are hard-coded; no per-project tuning | Every gate has three modes (strict / advisory / off); global defaults with per-project overrides |
| No versioning, no update mechanism, no health check | `harness update` (upgrades preserve user customizations), `harness doctor` (health check and conflict detection) |

### Confirmed key decisions

1. **Primary language: Rust.** The hook engine and all installation logic
   live in the same binary.
2. **Deployment form: Rust CLI first.** No Claude Code plugin; the binary
   itself registers hooks and installs agents and skills.
3. **Gate strength: three adjustable modes.** strict (block) / advisory
   (warn but allow) / off. Global default is advisory; a project layer can
   override to strict.
4. **Asset strategy: embedded + overridable.** All prompts/agents/skills are
   compiled into the binary via `include_str!`; `harness install`
   materializes them into `~/.claude/harness/` where the user can edit them
   directly; `harness update` can reset to the official version (user
   modifications are preserved with a diff notice).

## 2. CLI interface

```
harness install      # release assets to ~/.claude/harness/ + register hooks + install agents/skills
harness uninstall    # clean removal (deletes only what it registered/installed)
harness init         # generate a harness.toml customization layer in the current project
harness doctor       # health check: hooks registered, versions consistent, conflicts with other harnesses
harness update       # re-release assets after upgrade; user-modified files kept with a diff notice
harness config       # print the merged effective config (built-in + global + project)
harness hook <event> # hook engine entry point, invoked by Claude Code (not for direct use)
```

## 3. Hook engine (the discipline core)

Four hooks, all invoked by Claude Code's hooks mechanism as
`harness hook <event>`: read stdin JSON, write stdout JSON.

**Fail-open principle: if the engine itself fails (panic, corrupt state
file, config parse error), it always allows the action — it must never break
the user's session.**

| Event | Behavior |
|---|---|
| `SessionStart` | Inject the behavior protocol (evidence first, explicit assumptions, OODA loop) + a summary of the currently active gates |
| `UserPromptSubmit` | One-line behavior nudge (lightweight, minimal context cost) |
| `PostToolUse` | Silently record state: which code files were Edited/Written, which test commands ran (stored in a per-session state file keyed by session_id) |
| `Stop` (verify gate) | Before the turn ends: code changed without a subsequent test run? strict = return block and demand tests / advisory = warn but allow / off = no check |

State-tracking detail: PostToolUse appends tool events (Edit/Write/Bash) to a
per-session state file (in the user's temp directory, named by session_id).
The Stop gate reads it: if the "last code change" is later than the "last
test command", the turn counts as unverified.

## 4. Config layers (the per-project customization core)

Merge order (later overrides earlier):

1. Built-in defaults (compiled into the binary)
2. `~/.claude/harness/config.toml` (global)
3. `<project root>/harness.toml` (project)

Project-layer example:

```toml
[gates.verify]
mode = "strict"                      # this project uses strict mode
test_commands = ["cargo test"]       # what counts as "ran the tests"
code_globs = ["src/**/*.rs"]         # what counts as a "code change"
exempt_globs = ["docs/**", "*.md"]   # doc changes never trigger the gate

[review]
panel = ["skeptic", "red-team", "simplifier"]  # project-chosen review panel
```

`harness config` prints the merged result for easy debugging.

## 5. Adversarial review system

Five agent personas, installed into `~/.claude/agents/`:

| Agent | Lens |
|---|---|
| `skeptic` | logic holes, unverified inference |
| `red-team` | security risks and failure modes |
| `simplifier` | anti-overengineering, challenges unnecessary complexity |
| `evidence-auditor` | audits every claim for supporting evidence (new) |
| `user-advocate` | requirement fit and the user's perspective (new) |

Paired with an `adversarial-review` skill (installed into
`~/.claude/skills/`) responsible for: dispatching panel members in parallel,
aggregating verdicts, and majority-rule adjudication. A project can choose
its panel members and trigger thresholds via `[review].panel` in
`harness.toml`.

## 6. Repo structure

```
~/project/harness-rs/
├─ Cargo.toml
├─ src/
│  ├─ main.rs            # CLI entry point (clap)
│  ├─ commands/          # install / uninstall / init / doctor / update / config
│  ├─ hooks/             # session_start / prompt_nudge / post_tool_use / stop_gate
│  ├─ config.rs          # three-layer config merge
│  └─ state.rs           # per-session state tracking
├─ assets/               # content assets embedded via include_str!
│  ├─ protocol.md        # behavior protocol
│  ├─ agents/            # five agent persona .md files
│  ├─ skills/adversarial-review/
│  └─ default-config.toml
├─ tests/                # integration tests
└─ README.md
```

## 7. Safety baselines and error handling

- **settings.json protection**: timestamped backup before modification; read
  via serde_json preserving unknown fields; only append our own hook
  entries, never overwrite existing ones; temp file + rename atomic writes.
- **Hooks fail open**: any internal error ends in "allow" (exit 0, no block
  output).
- **uninstall touches only its own items**: entries are recognized by a
  marker (hook commands containing `harness hook`); everything else is left
  alone.
- **update preserves customizations**: install records the original hash of
  every released file; on update, a hash mismatch (user modified the file)
  keeps the current file and emits a diff notice instead of overwriting.

## 8. Test strategy

- Unit tests: three-layer config merge; gate verdict logic (code changed
  without tests / with tests / docs-only changes, etc.); settings.json
  merging (existing settings must not change).
- Integration tests: run `harness hook <event>` against fixture stdin JSON
  and verify stdout conforms to the Claude Code hooks protocol; fail-open
  scenarios (corrupt state file, corrupt config) must allow the action.

## 9. Explicitly out of scope (YAGNI)

- No Claude Code plugin / marketplace publishing (possible later).
- No PreToolUse dangerous-command interception (revisit in v2).
- Model-dispatch routing (Opus/Sonnet/Haiku) is not a core feature; only an
  optional CLAUDE.md template snippet ships in assets.

## Language policy

All repository documents, assets, code comments, commit messages, and
user-facing CLI copy are in English. The only exception: the README also
ships a Traditional Chinese translation as `README.zh-TW.md`.
