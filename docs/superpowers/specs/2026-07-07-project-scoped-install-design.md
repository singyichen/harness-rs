# 專案範圍安裝（project-scoped install）設計

日期：2026-07-07
狀態：已與使用者確認設計，待實作

## 背景與目標

harness 目前只支援全域安裝（`~/.claude/`），一次安裝對所有專案生效。
本擴充新增「安裝到單一專案」的能力：hooks、agents、skills 只對該專案的
session 生效，其他專案完全不受影響。全域安裝行為維持不變，兩者可共存。

可行性基礎（探索階段確認）：

- 四個生命週期指令的核心函式（`install_to` / `uninstall_from` / `check` /
  `update_at`）都已參數化接受 `claude_dir: &Path`，只有 CLI 層寫死
  `~/.claude`。本擴充不新增安裝邏輯，只新增路徑解析。
- Claude Code 原生支援專案層 `.claude/settings.json` 的 hooks 與專案層
  `.claude/agents/`、`.claude/skills/`。
- 全域與專案同時註冊完全相同的 hook 指令字串時，Claude Code 原生去重，
  只執行一次——這是共存策略的前提。
- session-start 的 protocol 注入已有內嵌 fallback，專案安裝不依賴全域
  檔案存在。

## CLI 介面

四個生命週期指令各加一個 `--project` boolean flag：

```
harness install   [--project]   # 無 flag → ~/.claude（原行為不變）
harness uninstall [--project]   # --project → <cwd>/.claude
harness doctor    [--project]
harness update    [--project]
```

- 心智模型與 `harness init` 一致：cd 到專案再執行。
- 不提供 `--target <dir>`（YAGNI；未來有需要再加）。
- `init` / `config` / `hook` 三個指令不變。

路徑解析抽成共用 helper：`resolve_claude_dir(project: bool)` ——
`project == false` 回傳 `home_dir()/.claude`，`true` 回傳
`current_dir()/.claude`。

## 各指令行為

### install --project

- 釋出全部 assets 到 `<專案>/.claude/`（沿用 `release_assets` 的
  manifest 雜湊規則：不覆蓋使用者修改過的檔案、官方副本落在 `.new`）。
- 在 `<專案>/.claude/settings.json` 註冊四個 hooks（沿用 marker 機制、
  時間戳備份、append-only 合併；檔案不存在則建立）。
- **不建立** `harness/config.toml`——config 引擎只讀「全域 config.toml +
  往上找最近的 harness.toml」，專案內的 config.toml 會是死檔案。
  安裝完提示：「要客製化此專案的設定，請執行 `harness init`」。
- manifest 寫在 `<專案>/.claude/harness/manifest.json`，與全域 manifest
  各自獨立。
- 結尾提示：安裝的檔案會出現在 git status，由使用者自行決定 commit
  進 repo（團隊共享）或加入 `.gitignore`。

### uninstall --project

- 沿用 `uninstall_from`：以專案 manifest 的雜湊驗證所有權，只刪未被
  修改的 harness 檔案與 `.new` 副本，移除專案 settings.json 中帶
  marker 的 hook 條目。
- 專案 `.claude/harness/state/` 一般不存在，現有 NotFound 容錯已涵蓋。
- 結尾訊息不含「global config preserved」字樣（專案安裝本來就不建
  config）。

### update --project

- 沿用 `update_at`：無專案 manifest 時拒絕（避免半安裝），其餘
  `.new` 機制原樣適用。

### doctor 與共存策略

兩份安裝允許共存，doctor 負責讓使用者看見現狀（提示，非警告非錯誤）：

- `harness doctor`（無 flag）：檢查全域安裝；若 `<cwd>/.claude/harness/
  manifest.json` 存在，加一行提示「此專案另有專案層安裝，用
  `harness doctor --project` 檢查」。
- `harness doctor --project`：檢查專案安裝；若全域 manifest 也存在，
  加一行提示「全域與專案安裝共存；hook 指令字串相同時 Claude Code
  會去重，不會雙重觸發」。
- 外來 Stop hook 檢查（`foreign_stop_hooks`）在 `--project` 模式下
  改查專案的 settings.json。

## protocol.md 讀取順序擴充

現況：session-start hook 只讀 `~/.claude/harness/protocol.md`，
fallback 到內嵌版。專案安裝會把 `protocol.md` 釋出到
`<專案>/.claude/harness/`，但引擎不讀它——使用者客製化不生效。

擴充為三層，最近的贏（與 config 分層精神一致）：

1. `<cwd>/.claude/harness/protocol.md`（專案層；hook 執行時 cwd 即
   專案目錄）
2. `~/.claude/harness/protocol.md`（全域層）
3. 內嵌版（編譯進 binary 的 fallback）

沿用現有規則：讀不到或非 UTF-8 即落到下一層，維持 fail-open。

## session state：維持全域不變

驗證關卡的 per-session 狀態檔一律寫在 `~/.claude/harness/state`，
與安裝範圍無關——它是暫存資料，session-start 的 prune 機制會清理
死 session。專案 uninstall 不處理它；全域 uninstall 照舊清理。

## 邊界情況

- **在家目錄執行 `--project`**：目標路徑與全域重合。安裝照做，但
  提示這等同於全域安裝（且不會建立 config.toml）。
- **專案無 settings.json**：`register_hooks` 現有邏輯會建立，不變。
- **hook 指令字串一致性**：全域與專案註冊的指令字串必須逐字元相同
  （皆為 `harness hook <event>`），否則 Claude Code 去重失效、hook
  觸發兩次。此不變量以測試鎖住。

## 測試策略

- **單元測試**：沿用「對 tempdir 呼叫參數化函式」模式。新增：
  - install 到專案目錄時不建立 `harness/config.toml`
  - doctor 共存提示（全域視角與專案視角各一）
  - `--project` 模式的 foreign Stop hook 檢查對象正確
  - protocol 三層讀取順序（專案 > 全域 > 內嵌，含非 UTF-8 落層）
  - 全域與專案 hook 指令字串逐字元相同（去重不變量）
- **E2E**：新增「專案 install → doctor → update → uninstall」完整
  生命週期，以及「全域＋專案共存」情境（互不干擾：各自的 manifest、
  各自的 settings.json、專案 uninstall 不動全域檔案）。
- **文件**：README.md 與 README.zh-TW.md 同步補 `--project` 用法與
  共存說明。

## 明確不做（YAGNI）

- `--target <dir>` 任意路徑安裝
- config 引擎新增第四層（專案 `.claude/harness/config.toml`）
- install 時自動執行 init
- 專案層 session state
