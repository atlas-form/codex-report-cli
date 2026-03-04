# codex-report-cli

一个最小化 Rust CLI：仅负责调用 Codex 生成项目分析报告。

## 功能

- 调用 `codex exec` 分析**当前目录项目**
- 每次运行按时间戳创建批次目录
- 每个项目单独子目录保存报告

输出结构示例：

```text
reports/
  batch_20260304_091500/
    project_a/
      report_20260304_091500.json
      report_20260304_091500.md
      meta_20260304_091500.json
    project_b/
      report_20260304_091500.json
      report_20260304_091500.md
      meta_20260304_091500.json
    summary.json
```

## 使用

```bash
cargo run --
```

可选参数：

- `--output-root <DIR>`: 报告根目录（默认 `reports`）
- `--output-root <DIR>`: 报告根目录（默认 `/srv/reports`）
- `--codex-exe <BIN>`: Codex 可执行名或路径（默认 `codex`）
