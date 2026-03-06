use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Local;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "codex-report-cli")]
#[command(about = "Call Codex to generate per-project Markdown status reports")]
struct Cli {
    #[arg(long, default_value = "./reports")]
    output_root: PathBuf,
    #[arg(long, default_value = "codex")]
    codex_exe: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let project = std::env::current_dir().context("failed to detect current working directory")?;

    fs::create_dir_all(&cli.output_root).with_context(|| {
        format!(
            "failed to create report root directory {}",
            cli.output_root.display()
        )
    })?;

    let latest_status = generate_project_status(&project, &cli.output_root, &cli.codex_exe)?;
    println!("Done: {}", latest_status.display());
    Ok(())
}

fn generate_project_status(project: &Path, output_root: &Path, codex_exe: &str) -> Result<PathBuf> {
    if !project.exists() || !project.is_dir() {
        bail!(
            "project not found or not a directory: {}",
            project.display()
        );
    }

    let project_name = project_name(project);
    let latest_status = output_root.join("status.md");
    let source_plan = project.join("plan.md");

    if !source_plan.exists() || !source_plan.is_file() {
        let markdown = render_missing_plan_status(&project_name, project);
        fs::write(&latest_status, markdown)
            .with_context(|| format!("failed to write {}", latest_status.display()))?;
        return Ok(latest_status);
    }

    let prompt = build_status_prompt(project, &project_name);
    if let Err(err) = run_codex(codex_exe, project, &latest_status, &prompt) {
        let markdown = render_codex_failed_status(&project_name, project, &err.to_string());
        fs::write(&latest_status, markdown)
            .with_context(|| format!("failed to write {}", latest_status.display()))?;
    }

    Ok(latest_status)
}

fn run_codex(
    codex_exe: &str,
    project_path: &Path,
    output_md: &Path,
    prompt: &str,
) -> Result<std::process::Output> {
    let mut child = Command::new(codex_exe)
        .arg("exec")
        .arg("-s")
        .arg("workspace-write")
        .arg("--skip-git-repo-check")
        .arg("-C")
        .arg(project_path)
        .arg("-o")
        .arg(output_md)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn codex executable: {codex_exe}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(prompt.as_bytes())
            .context("failed to write prompt to codex stdin")?;
    } else {
        return Err(anyhow!("failed to acquire codex stdin"));
    }

    let output = child
        .wait_with_output()
        .context("failed waiting for codex process")?;
    if !output.status.success() {
        return Err(anyhow!(
            "codex failed: exit={:?}, stderr={}",
            output.status.code(),
            tail_chars(&String::from_utf8_lossy(&output.stderr), 2600)
        ));
    }

    if !output_md.exists() {
        return Err(anyhow!(
            "codex finished but report file was not created: {}",
            output_md.display()
        ));
    }

    Ok(output)
}

fn build_status_prompt(project_path: &Path, project_name: &str) -> String {
    let p = project_path.display();

    format!(
        r#"你是项目状态报告 agent。请读取本机项目目录，输出一份结构化 Markdown 状态报告。

项目名称: {project_name}
项目根路径: {p}
计划文件: {p}/plan.md

规则:
1) 只输出 Markdown，不要输出 JSON。
2) 必须对照 plan.md 的关键目标进行完成度判断。
3) 尽量给出证据路径（文件路径或目录路径）。
4) 如果信息不足，明确写“信息不足”，不要编造。
5) 如果没有发现有效实现进展，要明确写“未发现有效进展”。

Markdown 结构必须包含：
- 标题：# {project_name} Status
- 概览（项目是否健康、一句话结论）
- 与计划对齐度（按 plan.md 的关键条目逐条对照）
- 本次进展
- 风险与阻塞
- 下一步（最多 5 条，按优先级）
- 附录：关键证据路径列表
"#
    )
}

fn render_missing_plan_status(project_name: &str, project_path: &Path) -> String {
    let generated_at = Local::now().format("%Y-%m-%d %H:%M:%S %:z");
    format!(
        "# {project_name} Status\n\n- 生成时间: {generated_at}\n- 项目路径: {}\n- 结论: 项目不合格（缺少根目录 plan.md）\n\n## 问题说明\n\n未在项目根目录发现 `plan.md`，本次不执行总结分析。\n\n## 需要补齐\n\n1. 在项目根目录新增 `plan.md`\n2. 明确目标、范围、里程碑、验收标准\n3. 下次运行后再生成完整状态报告\n",
        project_path.display()
    )
}

fn render_codex_failed_status(project_name: &str, project_path: &Path, error: &str) -> String {
    let generated_at = Local::now().format("%Y-%m-%d %H:%M:%S %:z");
    format!(
        "# {project_name} Status\n\n- 生成时间: {generated_at}\n- 项目路径: {}\n- 结论: 状态生成失败\n\n## 错误信息\n\n```text\n{error}\n```\n\n## 建议\n\n1. 检查 codex 命令是否可用\n2. 确认项目目录可读\n3. 重新执行 `codex-report-cli`\n",
        project_path.display()
    )
}

fn project_name(path: &Path) -> String {
    path.file_name()
        .and_then(OsStr::to_str)
        .map(std::string::ToString::to_string)
        .unwrap_or_else(|| "project".to_string())
}

fn tail_chars(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    chars[chars.len() - max_chars..].iter().collect()
}
