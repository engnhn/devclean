pub mod spinner;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::env;
use std::fmt::Write;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::artifact::recovery::recovery_for;
use crate::artifact::{ArtifactKind, Finding};
use crate::cleanup::CleanupPlan;
use crate::cleanup::execute::CleanupExecution;
use crate::fs::ScanResult;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CategorySummary {
    pub size_bytes: u64,
    pub count: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportOptions {
    pub details: bool,
    pub detail_limit: usize,
    pub home_dir: Option<PathBuf>,
    pub now: Option<SystemTime>,
    pub format: OutputFormat,
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];

    if bytes < 1024 {
        return format!("{bytes} B");
    }

    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= KB && unit < UNITS.len() - 1 {
        value /= KB;
        unit += 1;
    }

    let rendered = if value >= 10.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    };

    format!("{rendered} {}", UNITS[unit])
}

pub fn aggregate_by_kind(scan: &ScanResult) -> Vec<(ArtifactKind, CategorySummary)> {
    let mut summaries = BTreeMap::<ArtifactKind, CategorySummary>::new();

    for finding in &scan.findings {
        let summary = summaries.entry(finding.kind).or_default();
        summary.size_bytes += finding.size_bytes;
        summary.count += 1;
    }

    let mut summaries: Vec<_> = summaries.into_iter().collect();
    summaries.sort_by(|(left_kind, left), (right_kind, right)| {
        right
            .size_bytes
            .cmp(&left.size_bytes)
            .then_with(|| left_kind.label().cmp(right_kind.label()))
    });
    summaries
}

pub fn render_scan_report(scan: &ScanResult, options: ReportOptions) -> String {
    if options.format == OutputFormat::Json {
        return render_scan_json(scan, options.home_dir.as_deref());
    }

    let total: u64 = scan.findings.iter().map(|finding| finding.size_bytes).sum();
    let summaries = aggregate_by_kind(scan);
    let mut output = String::new();

    writeln!(output, "Found {} reclaimable", format_bytes(total)).unwrap();
    writeln!(output).unwrap();

    if summaries.is_empty() {
        writeln!(output, "No supported artifacts found.").unwrap();
    } else {
        writeln!(output, "{:<18} {:>12} {:>8}", "Artifact", "Size", "Items").unwrap();
        writeln!(output, "{:-<18} {:-<12} {:-<8}", "", "", "").unwrap();
        for (kind, summary) in summaries {
            writeln!(
                output,
                "{:<18} {:>12} {:>8}",
                kind.label(),
                format_bytes(summary.size_bytes),
                summary.count
            )
            .unwrap();
        }
    }

    if options.details && !scan.findings.is_empty() {
        write_details(
            &mut output,
            scan,
            &options,
            options.now.unwrap_or_else(SystemTime::now),
        );
    }

    if scan.stats.unreadable_entries > 0 {
        writeln!(output).unwrap();
        writeln!(
            output,
            "Skipped {} unreadable path(s).",
            scan.stats.unreadable_entries
        )
        .unwrap();
    }

    output
}

pub fn render_scan_json(scan: &ScanResult, home_dir: Option<&Path>) -> String {
    let total_bytes: u64 = scan.findings.iter().map(|f| f.size_bytes).sum();

    let mut json = String::new();
    json.push_str("{\n");
    json.push_str(&format!("  \"total_reclaimable_bytes\": {total_bytes},\n"));
    json.push_str(&format!(
        "  \"unreadable_entries\": {},\n",
        scan.stats.unreadable_entries
    ));
    json.push_str("  \"artifacts\": [\n");

    for (index, finding) in scan.findings.iter().enumerate() {
        let is_last = index == scan.findings.len() - 1;
        let recovery = recovery_for(finding.kind, &finding.path);
        let path_str = format_path_for_display(&finding.path, home_dir);
        let json_path = path_str.replace('\\', "\\\\").replace('"', "\\\"");

        json.push_str("    {\n");
        json.push_str(&format!("      \"kind\": \"{}\",\n", finding.kind.label()));
        json.push_str(&format!("      \"size_bytes\": {},\n", finding.size_bytes));
        json.push_str(&format!(
            "      \"recovery\": \"{}\",\n",
            recovery.display_text()
        ));
        json.push_str(&format!("      \"path\": \"{}\"\n", json_path));
        json.push_str(if is_last { "    }\n" } else { "    },\n" });
    }

    json.push_str("  ]\n");
    json.push_str("}\n");
    json
}

pub fn render_cleanup_plan(plan: &CleanupPlan, home_dir: Option<&Path>) -> String {
    let mut output = String::new();
    let mut entries: Vec<_> = plan.entries.iter().collect();
    entries.sort_by(|left, right| compare_findings(&left.finding, &right.finding));

    writeln!(output, "Cleanup plan").unwrap();
    writeln!(output).unwrap();

    if entries.is_empty() {
        writeln!(output, "No matching artifacts found.").unwrap();
        writeln!(output).unwrap();
        writeln!(output, "Would reclaim 0 B").unwrap();
        writeln!(output, "No files were removed.").unwrap();
        return output;
    }

    writeln!(
        output,
        "{:<10} {:<18} {:<14} Path",
        "Size", "Artifact", "Recovery"
    )
    .unwrap();
    writeln!(output, "{:-<10} {:-<18} {:-<14} {:-<42}", "", "", "", "").unwrap();

    for entry in &entries {
        writeln!(
            output,
            "{:<10} {:<18} {:<14} {}",
            format_bytes(entry.finding.size_bytes),
            cleanup_artifact_label(&entry.finding),
            entry.recovery.display_text(),
            format_path_for_display(&entry.finding.path, home_dir)
        )
        .unwrap();
    }

    writeln!(output).unwrap();
    writeln!(
        output,
        "Would reclaim {}",
        format_bytes(plan.planned_bytes())
    )
    .unwrap();
    if plan.blocked_count() > 0 {
        writeln!(
            output,
            "Blocked from execution: {} artifacts",
            plan.blocked_count()
        )
        .unwrap();
    }
    writeln!(output, "No files were removed.").unwrap();

    output
}

fn cleanup_artifact_label(finding: &Finding) -> Cow<'_, str> {
    match finding.kind {
        ArtifactKind::RustTarget => Cow::Borrowed("target"),
        ArtifactKind::PythonVenv => finding
            .path
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or(Cow::Borrowed("venv")),
        _ => Cow::Borrowed(finding.kind.label()),
    }
}

pub fn render_cleanup_execution(
    plan: &CleanupPlan,
    execution: &CleanupExecution,
    home_dir: Option<&Path>,
) -> String {
    let mut output = String::new();

    writeln!(
        output,
        "Removed {} artifact(s), planned {}",
        execution.removed.len(),
        format_bytes(execution.planned_removed_bytes())
    )
    .unwrap();

    if plan.blocked_count() > 0 {
        writeln!(
            output,
            "Blocked from execution: {} artifacts",
            plan.blocked_count()
        )
        .unwrap();
    }

    if !execution.failed.is_empty() {
        writeln!(
            output,
            "Failed to remove {} artifact(s):",
            execution.failed.len()
        )
        .unwrap();
        for failure in &execution.failed {
            writeln!(
                output,
                "{}: {}",
                format_path_for_display(&failure.finding.path, home_dir),
                failure.error
            )
            .unwrap();
        }
    }

    if execution.removed.is_empty() && execution.failed.is_empty() && plan.blocked_count() > 0 {
        writeln!(
            output,
            "No artifacts were removed because all selected artifacts were blocked."
        )
        .unwrap();
    }

    output
}

fn compare_findings(left: &Finding, right: &Finding) -> std::cmp::Ordering {
    right
        .size_bytes
        .cmp(&left.size_bytes)
        .then_with(|| left.kind.label().cmp(right.kind.label()))
        .then_with(|| left.path.cmp(&right.path))
}

fn write_details(output: &mut String, scan: &ScanResult, options: &ReportOptions, now: SystemTime) {
    let mut findings: Vec<&Finding> = scan.findings.iter().collect();
    findings.sort_by(|left, right| compare_findings(left, right));

    writeln!(output).unwrap();
    writeln!(output, "Largest artifacts").unwrap();
    writeln!(output).unwrap();
    writeln!(
        output,
        "{:<10} {:<18} {:<10} {:<14} Path",
        "Size", "Artifact", "Modified", "Recovery"
    )
    .unwrap();
    writeln!(
        output,
        "{:-<10} {:-<18} {:-<10} {:-<14} {:-<42}",
        "", "", "", "", ""
    )
    .unwrap();

    for finding in findings.into_iter().take(options.detail_limit) {
        let recovery = recovery_for(finding.kind, &finding.path);
        writeln!(
            output,
            "{:<10} {:<18} {:<10} {:<14} {}",
            format_bytes(finding.size_bytes),
            finding.kind.label(),
            format_modified_age(finding.modified_at, now),
            recovery.display_text(),
            format_path_for_display(&finding.path, options.home_dir.as_deref())
        )
        .unwrap();
    }
}

pub fn format_modified_age(modified_at: Option<SystemTime>, now: SystemTime) -> String {
    let Some(modified_at) = modified_at else {
        return "unknown".to_string();
    };

    let Ok(age) = now.duration_since(modified_at) else {
        return "unknown".to_string();
    };

    format_age_duration(age)
}

fn format_age_duration(age: Duration) -> String {
    const HOUR: u64 = 60 * 60;
    const DAY: u64 = 24 * HOUR;
    const YEAR: u64 = 365 * DAY;

    let seconds = age.as_secs();
    if seconds < HOUR {
        "<1h".to_string()
    } else if seconds < DAY {
        format!("{}h", seconds / HOUR)
    } else if seconds < YEAR {
        format!("{}d", seconds / DAY)
    } else {
        format!("{}y", seconds / YEAR)
    }
}

pub fn format_path_for_display(path: &Path, home_dir: Option<&Path>) -> String {
    let absolute = absolute_path(path);

    if let Some(home_dir) = home_dir
        && let Ok(stripped) = absolute.strip_prefix(home_dir)
    {
        if stripped.as_os_str().is_empty() {
            return "~".to_string();
        }

        return PathBuf::from("~").join(stripped).display().to_string();
    }

    absolute.display().to_string()
}

fn absolute_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }

    env::current_dir()
        .map(|current_dir| normalize_lexically(&current_dir.join(path)))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn normalize_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }

    normalized
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::SystemTime;

    use tempfile::tempdir;

    use crate::artifact::recovery::{RecoveryInfo, RecoveryStatus};
    use crate::artifact::{ArtifactKind, Finding};
    use crate::cleanup::{CleanupPlan, CleanupPlanEntry};

    use super::*;

    #[test]
    fn formats_bytes_naturally() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(999), "999 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(10 * 1024), "10 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn details_are_sorted_by_size_and_limited() {
        let tmp = tempdir().unwrap();
        let small = tmp
            .path()
            .join("project")
            .join("small")
            .join("node_modules");
        let large = tmp.path().join("project").join("large").join("target");
        let medium = tmp.path().join("project").join("medium").join(".next");
        let scan = ScanResult {
            findings: vec![
                finding(small.clone(), ArtifactKind::NodeModules, 10),
                finding(large.clone(), ArtifactKind::RustTarget, 30),
                finding(medium.clone(), ArtifactKind::NextBuild, 20),
            ],
            stats: Default::default(),
        };

        let output = render_scan_report(
            &scan,
            ReportOptions {
                details: true,
                detail_limit: 2,
                home_dir: None,
                now: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(100)),
                format: OutputFormat::Text,
            },
        );

        let target = output.find(&large.display().to_string()).unwrap();
        let next = output.find(&medium.display().to_string()).unwrap();

        assert!(target < next);
        assert!(!output.contains(&small.display().to_string()));
    }

    #[test]
    fn shortens_paths_inside_home_directory() {
        let tmp = tempdir().unwrap();
        let home = tmp.path().join("home");
        let path = home
            .join("projects")
            .join("ünicode app")
            .join("node_modules");

        let shortened = format_path_for_display(&path, Some(&home));
        assert_eq!(
            PathBuf::from(shortened),
            PathBuf::from("~")
                .join("projects")
                .join("ünicode app")
                .join("node_modules")
        );
    }

    #[test]
    fn falls_back_to_absolute_path_without_home_directory() {
        let output = format_path_for_display(Path::new("relative/path"), None);

        assert!(Path::new(&output).is_absolute());
        assert!(PathBuf::from(output).ends_with(Path::new("relative/path")));
    }

    #[test]
    fn formats_modified_age_compactly() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(400 * 24 * 60 * 60);

        assert_eq!(format_modified_age(Some(now), now), "<1h");
        assert_eq!(
            format_modified_age(Some(now - Duration::from_secs(4 * 60 * 60)), now),
            "4h"
        );
        assert_eq!(
            format_modified_age(Some(now - Duration::from_secs(3 * 24 * 60 * 60)), now),
            "3d"
        );
        assert_eq!(
            format_modified_age(Some(now - Duration::from_secs(365 * 24 * 60 * 60)), now),
            "1y"
        );
    }

    #[test]
    fn formats_unknown_modified_age_for_missing_or_future_timestamps() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(60);

        assert_eq!(format_modified_age(None, now), "unknown");
        assert_eq!(
            format_modified_age(Some(now + Duration::from_secs(60)), now),
            "unknown"
        );
    }

    #[test]
    fn cleanup_plan_uses_cli_labels_and_dry_run_wording() {
        let plan = CleanupPlan {
            entries: vec![CleanupPlanEntry {
                finding: finding(
                    PathBuf::from("/tmp/project/target"),
                    ArtifactKind::RustTarget,
                    1024,
                ),
                recovery: RecoveryInfo {
                    status: RecoveryStatus::Regenerable,
                    restore_hint: Some("cargo build"),
                },
                block_reason: None,
            }],
        };

        let output = render_cleanup_plan(&plan, None);

        assert!(output.contains("Cleanup plan"));
        assert!(output.contains("target"));
        assert!(!output.contains("Rust target"));
        assert!(output.contains("Would reclaim 1.0 KB"));
        assert!(output.contains("No files were removed."));
    }

    #[test]
    fn renders_json_report() {
        let scan = ScanResult {
            findings: vec![finding(
                PathBuf::from("/tmp/app/node_modules"),
                ArtifactKind::NodeModules,
                1024,
            )],
            stats: Default::default(),
        };

        let output = render_scan_report(
            &scan,
            ReportOptions {
                details: false,
                detail_limit: 0,
                home_dir: None,
                now: None,
                format: OutputFormat::Json,
            },
        );

        assert!(output.contains("\"total_reclaimable_bytes\": 1024"));
        assert!(output.contains("\"kind\": \"node_modules\""));
        assert!(output.contains("\"size_bytes\": 1024"));
    }

    fn finding(path: PathBuf, kind: ArtifactKind, size_bytes: u64) -> Finding {
        Finding {
            path,
            kind,
            size_bytes,
            modified_at: Some(SystemTime::UNIX_EPOCH),
            identity: None,
        }
    }
}
