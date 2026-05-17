# 垃圾清理 · 路 2（扫描器）实现记录

对应契约：`docs/junk-cleanup-contract.md` §6.2

## 产出文件

| 文件 | 说明 |
|---|---|
| `src-tauri/src/junk/mod.rs` | 3 行 `pub mod`：rules / scanner / cleaner |
| `src-tauri/src/junk/scanner.rs` | `JunkItem` / `JunkScanResult` / `scan_junk_blocking` / `simple_glob` |

## 关键实现点

- **并行**：`applicable.par_iter().map(process_rule)` 按 rule 粒度并行
- **过滤顺序**：先按 `categories` 过滤 → 再按 `requires_admin` 过滤（不通过的 rule_id 写入 `skipped_rules`）
- **三种产出形态**：
  - `patterns` 非空：`walkdir` 递归 + `simple_glob` 文件名匹配，每个匹配文件作为独立 `JunkItem`
  - `patterns` 空 + `clean_subdirs_only`：`read_dir` 列直接子项，每个子项一项（目录子项递归累加 size）
  - `patterns` 空 + 整体：整路径作为一项，`walkdir` 递归累加 size 与 file_count
- **通配父目录**：`resolve_path_glob` 找到第一个 `*` 切 `prefix / glob_segment / suffix`，`read_dir(prefix)` + `simple_glob` 拼回完整路径；剩余还有 `*` 则递归展开（支持 `Profiles*\cache2`、`*\AppData\...\Profiles*\...` 多级通配）
- **错误处理**：所有 IO 错误 `tracing::debug!("[junk-scan] ...")` 后跳过，无 `unwrap` / `panic`
- **路径不存在**：rule 的所有 `paths` 都不存在时，`rule_id` 写入 `skipped_rules`
- **简单 glob**：`split('*')` 后按段位置（首段 `starts_with` / 末段 `ends_with` / 中段 `find`）线性匹配；4 个单元测试覆盖精确 / 前后缀 / 纯星号 / 中段

## 已实施的微优化

1. **少一次 String 分配**：`patterns` 分支文件名匹配从 `to_string_lossy().to_string()` 改为 `to_string_lossy()`（`Cow<str>`），合法 UTF-8 文件名走借用零分配
2. **glob 解析**：分隔符判断从字符串切片比较 `&normalized[i..i+1] == "\\"` 改为 `as_bytes().get(i) == Some(&b'\\')`，避免冗余越界 guard 和 UTF-8 边界检查

两处都已二次验证非反向优化：行为等价、最坏情况与原写法持平，最好情况更省。

## 没动的项（受契约约束，不在本路范围）

| 项 | 原因 |
|---|---|
| 给 `walkdir` 加 `max_depth` 限制 | 需要 `JunkRule` 增加字段，属路 1 契约 |
| 单 rule 内多 `path` 并行展开 | 外层已并行，嵌套 rayon 收益不明显 |
| 切到 `jwalk` | 契约 §6.2 明确"用 walkdir" |
| `category_name` 改 `Arc<str>` 复用 | 会破坏 `JunkItem` 的 serde 形态，影响前端 |

## 验证

```
cargo check                                  ✅ Exit 0
cargo test --lib junk::scanner::tests        ✅ 4 passed
```

`cargo test` 整体在 `rules.rs:506` 报 `push_some` 未定义——属路 1 范围，与本路无关。
