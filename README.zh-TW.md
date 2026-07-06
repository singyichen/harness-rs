# harness — Claude Code 工程紀律引擎

一個 Rust 單一 binary,為 Claude Code 建立行為底線:證據先行、假設明示、
重大決策經對抗審查、程式碼變更必經測試驗證。安裝一次,所有專案生效;
每個專案可疊加自己的客製層。

[English README](README.md)

## 安裝

```bash
cargo install --path .
harness install    # 釋出資產 + 註冊 hooks + 安裝 agents/skills
harness doctor     # 體檢
```

## 指令

| 指令 | 作用 |
|---|---|
| `harness install` | 釋出資產到 ~/.claude/、註冊 hooks |
| `harness uninstall` | 乾淨移除(只刪自己的東西,保留你的客製與全域設定) |
| `harness init` | 在當前專案產生 harness.toml 客製層 |
| `harness doctor` | 體檢:hooks 註冊、版本一致、閘門衝突 |
| `harness update` | 升版後重釋資產(你改過的檔案保留,官方新版存 .new) |
| `harness config` | 顯示合併後設定(內建 → 全域 → 專案) |
| `harness hook <event>` | hook 引擎入口(Claude Code 呼叫,不需手動使用) |

## 驗證閘門

改了程式碼卻沒在其後跑測試?回合結束時依模式處理:

- `strict`:擋下,要求補測試(或向使用者說明原因後,第二次結束放行)
- `advisory`(預設):附警告放行
- `off`:不檢查

專案根目錄放 `harness.toml` 即可覆寫(`harness init` 產生範本):

```toml
[gates.verify]
mode = "strict"
test_commands = ["cargo test"]
```

## 對抗審查

五個 agent 鏡頭(skeptic / red-team / simplifier / evidence-auditor /
user-advocate)+ `adversarial-review` skill,重大結論過半存活才採信。
小組成員由 `[review].panel` 設定。

## 設計原則

- **Fail-open**:hook 引擎任何內部錯誤一律放行,絕不弄壞你的 session。
- **settings.json 安全**:時間戳備份、只增不覆、原子寫入、保留未知欄位。
- **你的客製優先**:install/update/uninstall 都不會覆蓋或刪除你改過的檔案。
