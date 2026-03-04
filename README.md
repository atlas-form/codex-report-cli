# codex-report-cli

一个最小化 Rust CLI：仅负责调用 Codex 生成项目分析报告。

## 功能

- 调用 `codex exec` 分析**当前目录项目**
- 每次运行按时间戳创建批次目录
- 每个项目单独子目录保存报告

输出结构示例：

```text
/srv/reports/
  index.json
  project_a/
    20260304_091500/
      report.json
      report.md
      meta.json
    20260304_102030/
      report.json
      report.md
      meta.json
```

说明：

- `index.json` 在 reports 根目录，记录所有项目和各项目的运行历史。
- 同一项目按时间戳子目录归档，不会把时间批次放在 reports 根目录。
- 每次运行会增量更新 `index.json`，保留已有项目记录。

## 使用

```bash
cargo run --
```

可选参数：

- `--output-root <DIR>`: 报告根目录（默认 `reports`）
- `--output-root <DIR>`: 报告根目录（默认 `/srv/reports`）
- `--codex-exe <BIN>`: Codex 可执行名或路径（默认 `codex`）
