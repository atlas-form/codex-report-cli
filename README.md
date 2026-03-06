# codex-report-cli

一个最小化 Rust CLI：调用 Codex 为**当前目录项目**生成 Markdown 状态报告。

## 新输出结构（按项目目录）

```text
/srv/reports/
  status.md
```

说明：

- 只保留 Markdown，不再生成任何 JSON 文件。
- `status.md` 为本次结果。

## 规则

- 运行时先检查项目根目录是否存在 `plan.md`。
- 如果缺少 `plan.md`：
  - 不做总结分析；
  - 直接生成一份“不合格状态”到 `status.md`。
- 如果存在 `plan.md`：
  - 调用 Codex 直接输出 Markdown 状态报告到 `status.md`；
  - 报告会要求对齐 `plan.md`，并尽量引用证据路径。

## 使用

```bash
cd /path/to/your/project
codex-report-cli --output-root /srv/reports
```

可选参数：

- `--output-root <DIR>`: 报告根目录（默认 `/srv/reports`）
- `--codex-exe <BIN>`: Codex 可执行名或路径（默认 `codex`）
