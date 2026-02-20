//! git-size-fast - Fast git repository size-over-time analysis using commit sampling
//!
//! This tool creates size-over-time analysis of git repositories by:
//! 1. Determining the repository time span from first to last commit
//! 2. Sampling by year (repos > 6 years) or month (younger repos)
//! 3. For each sample: finding the nearest commit and measuring blob sizes
//! 4. Outputting CSV and optional PNG plot

use chrono::{DateTime, Duration, NaiveDate};
use clap::Parser;
use csv::Writer;
use git2::{Repository, Sort};
use indicatif::{ProgressBar, ProgressStyle};
use plotters::prelude::*;
use std::error::Error;
use std::fmt;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Analyze git repository size over time using commit sampling
#[derive(Parser, Debug)]
#[command(name = "git-size-fast")]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to the git repository
    #[arg(default_value = ".")]
    repo_path: PathBuf,

    /// Output CSV file path
    #[arg(short, long)]
    output: PathBuf,

    /// Generate a plot of cumulative size (PNG format)
    #[arg(long)]
    plot: Option<PathBuf>,

    /// Force monthly sampling (default: yearly for repos > 6 years)
    #[arg(long)]
    monthly: bool,

    /// Force yearly sampling
    #[arg(long)]
    yearly: bool,
}

#[derive(Debug)]
enum GitSizeError {
    Git(git2::Error),
    Io(io::Error),
    Csv(csv::Error),
    Chrono(chrono::OutOfRangeError),
    Plot(String),
    Command(String),
    Validation(String),
}

impl fmt::Display for GitSizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitSizeError::Git(e) => write!(f, "Git error: {}", e),
            GitSizeError::Io(e) => write!(f, "IO error: {}", e),
            GitSizeError::Csv(e) => write!(f, "CSV error: {}", e),
            GitSizeError::Chrono(e) => write!(f, "Date error: {}", e),
            GitSizeError::Plot(e) => write!(f, "Plot error: {}", e),
            GitSizeError::Command(e) => write!(f, "Command error: {}", e),
            GitSizeError::Validation(e) => write!(f, "Validation error: {}", e),
        }
    }
}

impl Error for GitSizeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            GitSizeError::Git(e) => Some(e),
            GitSizeError::Io(e) => Some(e),
            GitSizeError::Csv(e) => Some(e),
            GitSizeError::Chrono(e) => Some(e),
            _ => None,
        }
    }
}

impl From<git2::Error> for GitSizeError {
    fn from(e: git2::Error) -> Self {
        GitSizeError::Git(e)
    }
}

impl From<io::Error> for GitSizeError {
    fn from(e: io::Error) -> Self {
        GitSizeError::Io(e)
    }
}

impl From<csv::Error> for GitSizeError {
    fn from(e: csv::Error) -> Self {
        GitSizeError::Csv(e)
    }
}

impl From<chrono::OutOfRangeError> for GitSizeError {
    fn from(e: chrono::OutOfRangeError) -> Self {
        GitSizeError::Chrono(e)
    }
}

type Result<T> = std::result::Result<T, GitSizeError>;

/// Repository commit range information
struct CommitRange {
    first_commit: git2::Commit<'static>,
    last_commit: git2::Commit<'static>,
    total_commits: u32,
}

/// A sample point in repository history
struct SamplePoint {
    date: String,
    commit_hash: String,
    depth: u32,
}

/// Size measurement result
struct SizeMeasurement {
    date: String,
    cumulative_size: u64,
}

/// Get the first (oldest) and last (newest) commits from the repository
fn get_commit_range(repo: &Repository) -> Result<CommitRange> {
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    revwalk.set_sorting(Sort::TIME)?;

    let commits: Vec<git2::Oid> = revwalk.filter_map(|r| r.ok()).collect();

    if commits.is_empty() {
        return Err(GitSizeError::Validation(
            "No commits found in repository".to_string(),
        ));
    }

    let total_commits = commits.len() as u32;

    // Get first and last commits
    let first_oid = commits.last().unwrap();
    let last_oid = commits.first().unwrap();

    let first_commit = repo.find_commit(*first_oid)?;
    let last_commit = repo.find_commit(*last_oid)?;

    // Safety: We're extending the lifetime here, but the commits are tied to the repo
    // which outlives this function. This is safe in this specific context.
    let first_commit =
        unsafe { std::mem::transmute::<git2::Commit<'_>, git2::Commit<'static>>(first_commit) };
    let last_commit =
        unsafe { std::mem::transmute::<git2::Commit<'_>, git2::Commit<'static>>(last_commit) };

    Ok(CommitRange {
        first_commit,
        last_commit,
        total_commits,
    })
}

/// Generate sample points based on repository age
fn generate_sample_points(
    repo: &Repository,
    repo_path: &Path,
    range: &CommitRange,
    monthly: bool,
    yearly: bool,
) -> Result<Vec<SamplePoint>> {
    let first_time = range.first_commit.time().seconds();
    let last_time = range.last_commit.time().seconds();

    let first_dt = DateTime::from_timestamp(first_time, 0)
        .ok_or_else(|| GitSizeError::Validation("Invalid first commit timestamp".to_string()))?;
    let last_dt = DateTime::from_timestamp(last_time, 0)
        .ok_or_else(|| GitSizeError::Validation("Invalid last commit timestamp".to_string()))?;

    let duration = last_dt - first_dt;
    let years = duration.num_days() as f64 / 365.25;

    // Determine sampling strategy
    let use_yearly = yearly || (!monthly && years > 6.0);
    let interval_days = if use_yearly { 365 } else { 30 };

    let mut sample_points = Vec::new();
    let mut current_time = first_dt;

    while current_time <= last_dt {
        let target_timestamp = current_time.timestamp();

        if let Some(commit_info) = find_nearest_commit(repo, target_timestamp)? {
            let depth = calculate_commit_depth(repo_path, &commit_info.0)?;

            sample_points.push(SamplePoint {
                date: current_time.format("%Y-%m-%d").to_string(),
                commit_hash: commit_info.0,
                depth,
            });
        }

        current_time = current_time
            .checked_add_signed(Duration::days(interval_days))
            .unwrap_or(last_dt);
    }

    // Always include the latest commit if not already present
    let last_timestamp = range.last_commit.time().seconds();
    if !sample_points.iter().any(|p| {
        NaiveDate::parse_from_str(&p.date, "%Y-%m-%d")
            .ok()
            .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp())
            == Some(last_timestamp)
    }) {
        let depth = calculate_commit_depth(repo_path, &range.last_commit.id().to_string())?;
        sample_points.push(SamplePoint {
            date: last_dt.format("%Y-%m-%d").to_string(),
            commit_hash: range.last_commit.id().to_string(),
            depth,
        });
    }

    // Remove duplicates and sort by date
    sample_points.sort_by(|a, b| a.date.cmp(&b.date));
    sample_points.dedup_by(|a, b| a.date == b.date);

    Ok(sample_points)
}

/// Find the commit nearest to a target timestamp
fn find_nearest_commit(repo: &Repository, target_timestamp: i64) -> Result<Option<(String, i64)>> {
    let mut revwalk = repo.revwalk()?;
    revwalk.push_head()?;
    revwalk.set_sorting(Sort::TIME)?;

    let mut best: Option<(String, i64)> = None;
    let mut best_diff = i64::MAX;

    for oid_result in revwalk {
        let oid = oid_result?;
        if let Ok(commit) = repo.find_commit(oid) {
            let commit_time = commit.time().seconds();
            let diff = (commit_time - target_timestamp).abs();

            if diff < best_diff {
                best_diff = diff;
                best = Some((oid.to_string(), commit_time));
            }

            // Stop if we've gone past the target significantly
            if commit_time < target_timestamp && best.is_some() {
                break;
            }
        }
    }

    Ok(best)
}

/// Calculate commit depth using git rev-list
fn calculate_commit_depth(repo_path: &Path, commit_hash: &str) -> Result<u32> {
    let output = Command::new("git")
        .args(["rev-list", "--count", commit_hash, "..HEAD"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| GitSizeError::Command(format!("Failed to run git rev-list: {}", e)))?;

    if output.status.success() {
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u32>()
            .map(|n| n + 1)
            .map_err(|e| GitSizeError::Validation(format!("Invalid commit count: {}", e)))
    } else {
        Err(GitSizeError::Command(format!(
            "git rev-list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )))
    }
}

/// Calculate the size of objects reachable from a specific commit
fn measure_size_at_depth(source_repo: &Path, commit_hash: &str, _depth: u32) -> Result<u64> {
    // Get all objects reachable from the commit
    let objects_output = Command::new("git")
        .args([
            "-C",
            source_repo.to_str().unwrap(),
            "rev-list",
            "--objects",
            commit_hash,
        ])
        .output()
        .map_err(|e| GitSizeError::Command(format!("Failed to list objects: {}", e)))?;

    if !objects_output.status.success() {
        return Err(GitSizeError::Command("Failed to list objects".to_string()));
    }

    let mut total_size = 0u64;

    // For each object, get its size using git cat-file --batch-check
    let objects_list = String::from_utf8_lossy(&objects_output.stdout);
    let object_ids: Vec<&str> = objects_list
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .collect();

    if object_ids.is_empty() {
        return Ok(0);
    }

    // Use batch mode for efficiency
    let mut cat_file = Command::new("git")
        .args([
            "-C",
            source_repo.to_str().unwrap(),
            "cat-file",
            "--batch-check=%(objectname) %(objecttype) %(objectsize)",
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| GitSizeError::Command(format!("Failed to start cat-file: {}", e)))?;

    let stdin = cat_file.stdin.as_mut().unwrap();
    for oid in &object_ids {
        writeln!(stdin, "{}", oid).map_err(GitSizeError::Io)?;
    }
    drop(cat_file.stdin.take()); // Close stdin to signal end of input

    let output = cat_file
        .wait_with_output()
        .map_err(|e| GitSizeError::Command(format!("cat-file failed: {}", e)))?;

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 && parts[1] == "blob" {
            if let Ok(size) = parts[2].parse::<u64>() {
                total_size += size;
            }
        }
    }

    Ok(total_size)
}

/// Format size in human-readable form
fn format_size(size: u64) -> String {
    const KB: u64 = 1_000;
    const MB: u64 = 1_000_000;
    const GB: u64 = 1_000_000_000;

    if size >= GB {
        format!("{:.2} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.2} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.2} KB", size as f64 / KB as f64)
    } else {
        format!("{} B", size)
    }
}

/// Generate cumulative size plot
fn generate_plot(data: &[SizeMeasurement], output_path: &Path) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }

    let plot_data: Vec<(i64, u64)> = data
        .iter()
        .filter_map(|d| {
            NaiveDate::parse_from_str(&d.date, "%Y-%m-%d")
                .ok()
                .map(|dt| {
                    (
                        dt.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp(),
                        d.cumulative_size,
                    )
                })
        })
        .collect();

    if plot_data.is_empty() {
        return Ok(());
    }

    let min_ts = plot_data.iter().map(|(t, _)| *t).min().unwrap_or(0);
    let max_ts = plot_data.iter().map(|(t, _)| *t).max().unwrap_or(0);
    let max_size = plot_data.iter().map(|(_, s)| *s).max().unwrap_or(0);

    // Add margins
    let time_margin = ((max_ts - min_ts) / 20).max(86400 * 30);
    let size_margin = (max_size / 10).max(1000);

    let root = BitMapBackend::new(output_path, (1200, 600)).into_drawing_area();
    root.fill(&WHITE)
        .map_err(|e| GitSizeError::Plot(e.to_string()))?;

    let mut chart = ChartBuilder::on(&root)
        .caption(
            "Git Repository Size Over Time",
            ("sans-serif", 30).into_font(),
        )
        .margin(5)
        .x_label_area_size(60)
        .y_label_area_size(80)
        .build_cartesian_2d(
            (min_ts - time_margin)..(max_ts + time_margin),
            0u64..(max_size + size_margin),
        )
        .map_err(|e| GitSizeError::Plot(e.to_string()))?;

    chart
        .configure_mesh()
        .light_line_style(TRANSPARENT)
        .bold_line_style(BLACK.mix(0.3))
        .x_labels(10)
        .y_labels(10)
        .x_label_formatter(&|v| {
            DateTime::from_timestamp(*v, 0)
                .map(|dt| dt.format("%Y-%m").to_string())
                .unwrap_or_default()
        })
        .y_label_formatter(&|v| format_size(*v))
        .draw()
        .map_err(|e| GitSizeError::Plot(e.to_string()))?;

    chart
        .draw_series(LineSeries::new(
            plot_data.iter().map(|(t, s)| (*t, *s)),
            BLUE,
        ))
        .map_err(|e| GitSizeError::Plot(e.to_string()))?
        .label("Cumulative Size")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], BLUE));

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.8))
        .border_style(BLACK)
        .draw()
        .map_err(|e| GitSizeError::Plot(e.to_string()))?;

    root.present()
        .map_err(|e| GitSizeError::Plot(e.to_string()))?;

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    // Resolve and validate repo path
    let repo_path = if args.repo_path.is_absolute() {
        args.repo_path.clone()
    } else {
        std::env::current_dir()?.join(&args.repo_path)
    };

    if !repo_path.exists() {
        return Err(GitSizeError::Validation(format!(
            "Repository path does not exist: {:?}",
            repo_path
        )));
    }

    // Open repository
    let repo = Repository::open(&repo_path).map_err(|e| {
        GitSizeError::Validation(format!("Cannot open repository at {:?}: {}", repo_path, e))
    })?;

    println!("Analyzing repository: {}", repo_path.display());

    // Get commit range
    let range = get_commit_range(&repo)?;

    let first_ts = range.first_commit.time().seconds();
    let last_ts = range.last_commit.time().seconds();
    let first_dt = DateTime::from_timestamp(first_ts, 0).unwrap();
    let last_dt = DateTime::from_timestamp(last_ts, 0).unwrap();

    let duration = last_dt - first_dt;
    let years = duration.num_days() as f64 / 365.25;

    println!(
        "Repository spans {} to {} ({:.1} years, {} commits)",
        first_dt.format("%Y-%m-%d"),
        last_dt.format("%Y-%m-%d"),
        years,
        range.total_commits
    );

    // Determine sampling strategy
    let use_yearly = args.yearly || (!args.monthly && years > 6.0);
    println!(
        "Using {} sampling",
        if use_yearly { "yearly" } else { "monthly" }
    );

    // Generate sample points
    let samples = generate_sample_points(&repo, &repo_path, &range, args.monthly, args.yearly)?;
    println!("Will sample {} points", samples.len());

    // Progress bar
    let pb = ProgressBar::new(samples.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=>-"),
    );

    // Measure sizes
    let mut results: Vec<SizeMeasurement> = Vec::with_capacity(samples.len());

    for sample in &samples {
        pb.set_message(format!(
            "Sampling {} (depth: {})...",
            sample.date, sample.depth
        ));

        let size = measure_size_at_depth(&repo_path, &sample.commit_hash, sample.depth)?;

        results.push(SizeMeasurement {
            date: sample.date.clone(),
            cumulative_size: size,
        });

        pb.set_message(format!("Sampling {} ({})", sample.date, format_size(size)));
        pb.inc(1);
    }

    pb.finish_with_message("Sampling complete");

    // Write CSV
    println!("Writing CSV to {}", args.output.display());
    let mut wtr = Writer::from_path(&args.output)?;
    wtr.write_record(["date", "cumulative-size"])?;
    for data in &results {
        wtr.write_record([&data.date, &data.cumulative_size.to_string()])?;
    }
    wtr.flush()?;

    // Generate plot
    if let Some(plot_path) = &args.plot {
        println!("Generating plot: {}", plot_path.display());
        generate_plot(&results, plot_path)?;
        println!("Plot saved to {}", plot_path.display());
    }

    println!("Done! Output written to {}", args.output.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(999), "999 B");
    }

    #[test]
    fn test_format_size_kilobytes() {
        assert_eq!(format_size(1_000), "1.00 KB");
        assert_eq!(format_size(1_500), "1.50 KB");
        assert_eq!(format_size(999_999), "1000.00 KB");
    }

    #[test]
    fn test_format_size_megabytes() {
        assert_eq!(format_size(1_000_000), "1.00 MB");
        assert_eq!(format_size(2_500_000), "2.50 MB");
        assert_eq!(format_size(999_999_999), "1000.00 MB");
    }

    #[test]
    fn test_format_size_gigabytes() {
        assert_eq!(format_size(1_000_000_000), "1.00 GB");
        assert_eq!(format_size(5_500_000_000), "5.50 GB");
    }
}
