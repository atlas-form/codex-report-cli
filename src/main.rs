use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use chrono::Local;
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Parser, Debug)]
#[command(name = "codex-report-cli")]
#[command(about = "Call Codex to generate per-project analysis reports")]
struct Cli {
    #[arg(long, default_value = "/srv/reports")]
    output_root: PathBuf,
    #[arg(long, default_value = "codex")]
    codex_exe: String,
}

#[derive(Debug, Serialize)]
struct SummaryItem {
    project_key: String,
    project_name: String,
    project_path: String,
    run_timestamp: String,
    status: String,
    report_json: String,
    report_md: String,
    meta_json: String,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ReportsIndex {
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    projects: Vec<ProjectIndexEntry>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ProjectIndexEntry {
    project_key: String,
    project_name: String,
    project_path: String,
    project_dir: String,
    latest_run_timestamp: String,
    latest_status: String,
    latest_report_json: String,
    latest_report_md: String,
    latest_meta_json: String,
    #[serde(default)]
    runs: Vec<RunIndexEntry>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct RunIndexEntry {
    run_timestamp: String,
    generated_at: String,
    status: String,
    report_json: String,
    report_md: String,
    meta_json: String,
    error: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let project = std::env::current_dir().context("failed to detect current working directory")?;
    let now = Local::now();
    let timestamp = now.format("%Y%m%d_%H%M%S").to_string();
    fs::create_dir_all(&cli.output_root).with_context(|| {
        format!(
            "failed to create report root directory {}",
            cli.output_root.display()
        )
    })?;

    let item = generate_project_report(&project, &cli.output_root, &timestamp, &cli.codex_exe);
    let summary_item = match item {
        Ok(ok) => {
            println!("OK   {}", ok.project_path);
            ok
        }
        Err(err) => {
            let project_name = project_name(&project);
            let project_key = sanitize_name(&project_name);
            let project_dir = cli.output_root.join(&project_key);
            let run_dir = project_dir.join(&timestamp);
            fs::create_dir_all(&run_dir).ok();

            let report_json = run_dir.join("report.json");
            let report_md = run_dir.join("report.md");
            let meta_json = run_dir.join("meta.json");

            let _ = write_pretty_json(
                &report_json,
                &json!({
                    "generated_at": Local::now().to_rfc3339(),
                    "project_path": project.to_string_lossy(),
                    "error": err.to_string(),
                    "has_plan": false
                }),
            );
            let _ = fs::write(
                &report_md,
                format!(
                    "# 报告生成失败\n\n- 项目: {}\n- 时间: {}\n- 错误: {}\n",
                    project.to_string_lossy(),
                    Local::now().to_rfc3339(),
                    err
                ),
            );
            let _ = write_pretty_json(
                &meta_json,
                &json!({
                    "status": "failed",
                    "error": err.to_string(),
                    "generated_at": Local::now().to_rfc3339()
                }),
            );

            SummaryItem {
                project_key,
                project_name,
                project_path: project.to_string_lossy().to_string(),
                run_timestamp: timestamp.clone(),
                status: "failed".to_string(),
                report_json: report_json.to_string_lossy().to_string(),
                report_md: report_md.to_string_lossy().to_string(),
                meta_json: meta_json.to_string_lossy().to_string(),
                error: Some(err.to_string()),
            }
        }
    };
    let index_path = update_root_index(&cli.output_root, &summary_item, &now.to_rfc3339())?;
    println!("Done. Root index: {}", index_path.display());
    Ok(())
}

fn generate_project_report(
    project: &Path,
    output_root: &Path,
    timestamp: &str,
    codex_exe: &str,
) -> Result<SummaryItem> {
    if !project.exists() || !project.is_dir() {
        bail!("project not found or not a directory: {}", project.display());
    }

    let project_name = project_name(project);
    let project_key = sanitize_name(&project_name);
    let project_dir = output_root.join(&project_key);
    let run_dir = project_dir.join(timestamp);
    fs::create_dir_all(&run_dir)
        .with_context(|| format!("failed to create run dir {}", run_dir.display()))?;

    let report_json = run_dir.join("report.json");
    let report_md = run_dir.join("report.md");
    let meta_json = run_dir.join("meta.json");
    let prompt = build_prompt(project, &project_name);

    let output = run_codex(codex_exe, project, &report_json, &prompt)?;
    let report_text = fs::read_to_string(&report_json)
        .with_context(|| format!("failed to read report {}", report_json.display()))?;
    let report_value: Value = serde_json::from_str(&report_text)
        .with_context(|| format!("codex output is not valid JSON: {}", report_json.display()))?;
    if !report_value.is_object() {
        return Err(anyhow!("codex output is not a JSON object"));
    }

    let markdown = render_markdown(&project_name, project, &report_value);
    fs::write(&report_md, markdown)
        .with_context(|| format!("failed to write markdown {}", report_md.display()))?;

    write_pretty_json(
        &meta_json,
        &json!({
            "generated_at": Local::now().to_rfc3339(),
            "project_name": project_name,
            "project_path": project.to_string_lossy(),
            "status": "ok",
            "engine": "codex",
            "command": format!(
                "{} exec -s workspace-write --skip-git-repo-check -C {} -o {} -",
                codex_exe,
                project.display(),
                report_json.display()
            ),
            "exit_code": output.status.code(),
            "stdout_tail": tail_chars(&String::from_utf8_lossy(&output.stdout), 1800),
            "stderr_tail": tail_chars(&String::from_utf8_lossy(&output.stderr), 2600)
        }),
    )?;

    Ok(SummaryItem {
        project_key,
        project_name,
        project_path: project.to_string_lossy().to_string(),
        run_timestamp: timestamp.to_string(),
        status: "ok".to_string(),
        report_json: report_json.to_string_lossy().to_string(),
        report_md: report_md.to_string_lossy().to_string(),
        meta_json: meta_json.to_string_lossy().to_string(),
        error: None,
    })
}

fn update_root_index(output_root: &Path, item: &SummaryItem, generated_at: &str) -> Result<PathBuf> {
    let index_path = output_root.join("index.json");
    let mut index: ReportsIndex = if index_path.exists() {
        let raw = fs::read_to_string(&index_path)
            .with_context(|| format!("failed to read {}", index_path.display()))?;
        serde_json::from_str(&raw).with_context(|| {
            format!(
                "failed to parse existing {} (not overwriting it)",
                index_path.display()
            )
        })?
    } else {
        ReportsIndex::default()
    };

    let run_entry = RunIndexEntry {
        run_timestamp: item.run_timestamp.clone(),
        generated_at: generated_at.to_string(),
        status: item.status.clone(),
        report_json: item.report_json.clone(),
        report_md: item.report_md.clone(),
        meta_json: item.meta_json.clone(),
        error: item.error.clone(),
    };

    if let Some(existing) = index
        .projects
        .iter_mut()
        .find(|x| x.project_key == item.project_key)
    {
        existing.project_name = item.project_name.clone();
        existing.project_path = item.project_path.clone();
        existing.project_dir = output_root
            .join(&item.project_key)
            .to_string_lossy()
            .to_string();

        if let Some(run) = existing
            .runs
            .iter_mut()
            .find(|x| x.run_timestamp == item.run_timestamp)
        {
            *run = run_entry;
        } else {
            existing.runs.push(run_entry);
        }

        existing
            .runs
            .sort_by(|a, b| b.run_timestamp.cmp(&a.run_timestamp));
        if let Some(latest) = existing.runs.first() {
            existing.latest_run_timestamp = latest.run_timestamp.clone();
            existing.latest_status = latest.status.clone();
            existing.latest_report_json = latest.report_json.clone();
            existing.latest_report_md = latest.report_md.clone();
            existing.latest_meta_json = latest.meta_json.clone();
        }
    } else {
        index.projects.push(ProjectIndexEntry {
            project_key: item.project_key.clone(),
            project_name: item.project_name.clone(),
            project_path: item.project_path.clone(),
            project_dir: output_root
                .join(&item.project_key)
                .to_string_lossy()
                .to_string(),
            latest_run_timestamp: item.run_timestamp.clone(),
            latest_status: item.status.clone(),
            latest_report_json: item.report_json.clone(),
            latest_report_md: item.report_md.clone(),
            latest_meta_json: item.meta_json.clone(),
            runs: vec![run_entry],
        });
    }

    index.projects.sort_by(|a, b| a.project_key.cmp(&b.project_key));
    index.updated_at = generated_at.to_string();
    write_pretty_json(&index_path, &serde_json::to_value(index).context("serialize index failed")?)?;
    Ok(index_path)
}

fn run_codex(codex_exe: &str, project_path: &Path, output_json: &Path, prompt: &str) -> Result<std::process::Output> {
    let mut child = Command::new(codex_exe)
        .arg("exec")
        .arg("-s")
        .arg("workspace-write")
        .arg("--skip-git-repo-check")
        .arg("-C")
        .arg(project_path)
        .arg("-o")
        .arg(output_json)
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

    let output = child.wait_with_output().context("failed waiting for codex process")?;
    if !output.status.success() {
        return Err(anyhow!(
            "codex failed: exit={:?}, stderr={}",
            output.status.code(),
            tail_chars(&String::from_utf8_lossy(&output.stderr), 2600)
        ));
    }
    if !output_json.exists() {
        return Err(anyhow!(
            "codex finished but report file was not created: {}",
            output_json.display()
        ));
    }
    Ok(output)
}

fn build_prompt(project_path: &Path, project_name: &str) -> String {
    let p = project_path.display();
    format!(
        r#"你是项目进度评估 agent。请读取本机项目目录并生成分析报告，只输出一个 JSON 对象。

项目名称: {project_name}
项目根路径: {p}
开发计划目录: {p}/project_plan

输出 JSON 必须包含字段:
{{
  "project_path": "...",
  "has_plan": true/false,
  "warning": "...",
  "plan_summary": "...",
  "total_progress": 0-100 或 null,
  "implemented_parts": [{{"part":"...","evidence":["..."]}}],
  "quality_scores": [{{"part":"...","score":0-100,"difficulty_value_critical_weight":0-5}}],
  "pending_parts": ["..."],
  "extra_parts": ["..."],
  "overall_comment": "..."
}}

规则:
1) 只输出 JSON，不能输出 Markdown 或解释文字。
2) 如果找不到 project_plan，has_plan=false 且 warning 写清楚。
3) evidence 尽量包含文件名或路径片段。
"#
    )
}

fn render_markdown(project_name: &str, project_path: &Path, payload: &Value) -> String {
    let generated_at = Local::now().to_rfc3339();
    let pretty = serde_json::to_string_pretty(payload).unwrap_or_else(|_| "{}".to_string());
    format!(
        "# {project_name} 分析报告\n\n- 生成时间: {generated_at}\n- 项目路径: {}\n\n## JSON 结果\n\n```json\n{pretty}\n```\n",
        project_path.display()
    )
}

fn project_name(path: &Path) -> String {
    path.file_name()
        .and_then(OsStr::to_str)
        .map(std::string::ToString::to_string)
        .unwrap_or_else(|| "project".to_string())
}

fn sanitize_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let compact = out.trim_matches('_');
    if compact.is_empty() {
        "project".to_string()
    } else {
        compact.to_string()
    }
}

fn write_pretty_json(path: &Path, value: &Value) -> Result<()> {
    let text = serde_json::to_string_pretty(value).context("failed to serialize json")?;
    fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn tail_chars(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars {
        return text.to_string();
    }
    chars[chars.len() - max_chars..].iter().collect()
}
