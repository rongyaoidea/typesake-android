# 跨语言分隔符协议（Rust ↔ Kotlin）

唯一真相（两端同值，禁止手写字面量）：

| 名称 | 字符 | 含义 | Rust | Kotlin |
|---|---|---|---|---|
| US | `U+001E` | 字段分隔 | `lib::US` / `ffi::FIELD` / `ffi::UNIT_SEP` | `TypesakeCore.US` |
| GS | `U+001F` | 列表分隔 | `lib::DELIM` / `lib::GS` / `ffi::ITEM`（别名） | `TypesakeCore.GS` |
| RS | `U+001D` | 子字段分隔 | `lib::RS` / `ffi::REC_SEP` | `TypesakeCore.RS_PUB` |

热路径（每键）：`analyzeInput` = `flag<US>matched<US>c1<GS>c2…`；
`blockedWords` / `userWords` / `englishCandidates` 内部用 `RS`（`拼音<RS>词`、`kind<RS>text`）。
冷路径（收藏/统计/初始化）：JSON（`serde_json` / `kotlinx.serialization`）。

规则：
- 新增协议一律走 US/GS/RS 三常量，不引入新分隔符；
- 用户输入侧过滤 US/GS/RS 与控制字符（`userdic::parse_names` /
  `remember_domain` 已做），防分隔符注入破坏 `splitDelim` 解析。
