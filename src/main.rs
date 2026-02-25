//! Git Size History - Fast git repository size-over-time analysis using commit sampling
//!
//! This tool creates size-over-time analysis of git repositories by:
//! 1. Determining the repository time span from first to last commit
//! 2. Sampling by year (repos > 6 years) or month (younger repos)
//! 3. For each sample: finding the nearest commit and measuring blob sizes
//! 4. Outputting CSV and optional PNG plot

use chrono::{DateTime, Duration, NaiveDate, Utc};
use clap::Parser;
use csv::Writer;
use git2::Repository;
use indicatif::{ProgressBar, ProgressStyle};
use plotters::prelude::*;
use rayon::prelude::*;
use std::cmp::Reverse;
use std::error::Error;
use std::fmt;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Analyze git repository size over time using commit sampling
#[derive(Parser, Debug)]
#[command(name = "git-size-history")]
#[command(author = "Gautier Portet <gautier@soundconverter.org>", version, about, long_about = None)]
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

    /// Force yearly sampling
    #[arg(long)]
    yearly: bool,

    /// Force monthly sampling (default: yearly for repos > 6 years)
    #[arg(long)]
    monthly: bool,

    /// Enable debug output (show command outputs)
    #[arg(long, short = 'D')]
    debug: bool,

    /// Also calculate and output uncompressed blob sizes (slower)
    #[arg(long, short = 'U')]
    uncompressed: bool,
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
struct CommitRange<'repo> {
    /// The oldest (first) commit in the repository
    first_commit: git2::Commit<'repo>,
    /// The newest (last) commit in the repository
    last_commit: git2::Commit<'repo>,
    /// Total number of commits in the repository
    total_commits: u32,
}

/// A sample point in repository history
struct SamplePoint {
    /// Formatted date string (YYYY-MM-DD)
    date: String,
    /// Commit hash at this sample point
    commit_hash: String,
}

/// Size measurement result
struct SizeMeasurement {
    /// Formatted date string (YYYY-MM-DD)
    date: String,
    /// Cumulative packed size in bytes
    cumulative_size: u64,
    /// Uncompressed blob size in bytes (if calculated)
    uncompressed_size: Option<u64>,
}

/// Number of days in a year (accounting for leap years)
const DAYS_PER_YEAR: f64 = 365.25;
/// Repository age threshold in years for using yearly sampling
const YEARLY_THRESHOLD_YEARS: f64 = 6.0;
/// Sampling interval in days for yearly sampling
const YEARLY_INTERVAL_DAYS: i64 = 365;
/// Sampling interval in days for monthly sampling
const MONTHLY_INTERVAL_DAYS: i64 = 30;

/// Check if the repository has a bitmap index available.
///
/// Bitmap indexes are stored in .git/objects/pack/ directory as .bitmap files.
/// They significantly speed up git rev-list --disk-usage operations.
fn check_bitmap_index(repo_path: &Path) -> bool {
    let pack_dir = repo_path.join(".git/objects/pack");
    if let Ok(entries) = std::fs::read_dir(&pack_dir) {
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|ext| ext == "bitmap") {
                return true;
            }
        }
    }
    false
}

/// Get the first (oldest) and last (newest) commits from the repository.
///
/// This function walks the commit history using git2 and collects all commits
/// with their timestamps for efficient binary search during sampling.
/// Uses parallel processing for large repositories.
fn get_commit_range<'a>(
    repo: &'a Repository,
    repo_path: &Path,
    analysis_pb: &ProgressBar,
) -> Result<CommitRange<'a>> {
    // Check for bitmap index and warn if not present
    let has_bitmap = check_bitmap_index(repo_path);
    if !has_bitmap {
        eprintln!(
            "⚠️  Warning: No bitmap index found in repository.\n\
             Running 'git repack -ad --write-bitmap-index' can significantly speed up size measurements.\n\
             Example: cd {:?} && git repack -ad --write-bitmap-index",
            repo_path
        );
    }

    analysis_pb.set_message("Counting commits...");

    // Get total commit count using git rev-list --count (fast, especially with bitmaps)
    let count_output = Command::new("git")
        .arg("--no-replace-objects")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-list", "--count", "HEAD"])
        .output()?;
    let total_commits = String::from_utf8_lossy(&count_output.stdout)
        .trim()
        .parse::<u32>()
        .unwrap_or(0);

    if total_commits == 0 {
        return Err(GitSizeError::Validation(
            "No commits found in repository".to_string(),
        ));
    }

    analysis_pb.set_message("Finding first and last commits...");

    // Last commit is HEAD
    let last_commit = repo.head()?.peel_to_commit()?;

    // First commit: find all roots and pick the oldest
    let roots_output = Command::new("git")
        .arg("--no-replace-objects")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-list", "--max-parents=0", "HEAD"])
        .output()?;
    let stdout = String::from_utf8_lossy(&roots_output.stdout);

    let mut first_commit: Option<git2::Commit> = None;
    let mut earliest_time = i64::MAX;

    for line in stdout.lines() {
        if let Ok(oid) = git2::Oid::from_str(line.trim()) {
            if let Ok(commit) = repo.find_commit(oid) {
                let time = commit.time().seconds();
                if time < earliest_time {
                    earliest_time = time;
                    first_commit = Some(commit);
                }
            }
        }
    }

    let first_commit = first_commit
        .ok_or_else(|| GitSizeError::Validation("Failed to find initial commit".to_string()))?;

    Ok(CommitRange {
        first_commit,
        last_commit,
        total_commits,
    })
}

/// Generate sample points based on repository age.
///
/// This function determines a set of sampling dates between the first and last
/// commits of the repository. It uses an adaptive strategy (yearly or monthly)
/// unless forced by flags.
fn generate_sample_points(
    repo_path: &Path,
    range: &CommitRange<'_>,
    monthly: bool,
    yearly: bool,
) -> Result<Vec<SamplePoint>> {
    let first_time = range.first_commit.time().seconds();
    let last_time = range.last_commit.time().seconds();

    let first_dt = DateTime::from_timestamp(first_time, 0)
        .ok_or_else(|| GitSizeError::Validation("Invalid first commit timestamp".to_string()))?
        .with_timezone(&Utc);
    let last_dt = DateTime::from_timestamp(last_time, 0)
        .ok_or_else(|| GitSizeError::Validation("Invalid last commit timestamp".to_string()))?
        .with_timezone(&Utc);

    let duration = last_dt - first_dt;
    let years = duration.num_days() as f64 / DAYS_PER_YEAR;

    // Determine sampling strategy
    let use_yearly = yearly || (!monthly && years > YEARLY_THRESHOLD_YEARS);
    let interval_days = if use_yearly {
        YEARLY_INTERVAL_DAYS
    } else {
        MONTHLY_INTERVAL_DAYS
    };

    let mut target_times = Vec::new();
    let mut current_time = first_dt;

    while current_time <= last_dt {
        target_times.push(current_time);
        current_time = match current_time.checked_add_signed(Duration::days(interval_days)) {
            Some(new_time) => new_time,
            None => break,
        };
    }

    // Ensure the last commit's time is included as a target
    if target_times.last().map(|t| t.timestamp()) != Some(last_dt.timestamp()) {
        target_times.push(last_dt);
    }

    // Sort target times DESCENDING because git rev-list is descending
    target_times.sort_by_key(|t| Reverse(t.timestamp()));

    let mut sample_points = Vec::new();

    // Stream commits once to find all matches
    let mut child = Command::new("git")
        .arg("--no-replace-objects")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-list", "--timestamp", "HEAD"])
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| GitSizeError::Command(format!("Failed to spawn git rev-list: {}", e)))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| GitSizeError::Command("Failed to open git rev-list stdout".to_string()))?;
    let reader = BufReader::new(stdout);

    let mut target_idx = 0;

    for line in reader.lines() {
        let line = line?;
        let mut parts = line.split_whitespace();
        let ts = parts
            .next()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let hash = parts.next().unwrap_or("");

        // While the current commit is at or before our current target timestamp,
        // it's the latest commit for that target.
        while target_idx < target_times.len() && ts <= target_times[target_idx].timestamp() {
            if !hash.is_empty() {
                sample_points.push(SamplePoint {
                    date: target_times[target_idx].format("%Y-%m-%d").to_string(),
                    commit_hash: hash.to_string(),
                });
            }
            target_idx += 1;
        }

        if target_idx >= target_times.len() {
            // Found all sample points, can stop early
            let _ = child.kill();
            break;
        }
    }

    let _ = child.wait();

    // Sort by date ascending for the rest of the application
    sample_points.sort_by(|a, b| a.date.cmp(&b.date));
    sample_points.dedup_by(|a, b| a.date == b.date);

    Ok(sample_points)
}

/// Calculate the size of objects reachable from a specific commit.
///
/// This function uses git commands via `std::process::Command` to:
/// 1. Measure the packed disk usage using `git rev-list --objects --disk-usage`.
/// 2. (Optional) Measure the uncompressed size of all blobs using a pipeline
///    of `git rev-list` and `git cat-file`.
fn measure_size_at_commit(
    source_repo: &Path,
    commit_hash: &str,
    debug: bool,
    calculate_uncompressed: bool,
) -> Result<(u64, Option<u64>)> {
    // Basic validation
    if commit_hash.is_empty() {
        return Err(GitSizeError::Validation(
            "Commit hash cannot be empty".to_string(),
        ));
    }

    // Get packed disk usage using git rev-list --disk-usage
    let disk_usage_output = Command::new("git")
        .arg("--no-replace-objects")
        .arg("-C")
        .arg(source_repo)
        .args([
            "rev-list",
            "--objects",
            "--disk-usage",
            "--use-bitmap-index",
            commit_hash,
        ])
        .output()
        .map_err(|e| GitSizeError::Command(format!("Failed to get disk usage: {}", e)))?;

    if !disk_usage_output.status.success() {
        return Err(GitSizeError::Command(
            "Failed to get disk usage".to_string(),
        ));
    }

    // The last line contains the total disk usage in bytes
    let disk_usage_stdout = String::from_utf8_lossy(&disk_usage_output.stdout);
    let packed_size = disk_usage_stdout
        .lines()
        .last()
        .and_then(|line| line.trim().parse::<u64>().ok())
        .unwrap_or(0);

    // Calculate uncompressed size only if requested (it's slower)
    let uncompressed_size = if calculate_uncompressed {
        let mut rev_list = Command::new("git")
            .arg("--no-replace-objects")
            .arg("-C")
            .arg(source_repo)
            .args(["rev-list", "--objects", commit_hash])
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| GitSizeError::Command(format!("Failed to spawn git rev-list: {}", e)))?;

        let mut cat_file = Command::new("git")
            .arg("--no-replace-objects")
            .arg("-C")
            .arg(source_repo)
            .args(["cat-file", "--batch-check=%(objectname) %(objecttype) %(objectsize)"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| GitSizeError::Command(format!("Failed to spawn git cat-file: {}", e)))?;

        let mut stdin = cat_file.stdin.take().ok_or_else(|| {
            GitSizeError::Command("Failed to open git cat-file stdin".to_string())
        })?;

        let rev_list_stdout = rev_list.stdout.take().ok_or_else(|| {
            GitSizeError::Command("Failed to open git rev-list stdout".to_string())
        })?;

        let stdout = cat_file.stdout.take().ok_or_else(|| {
            GitSizeError::Command("Failed to open git cat-file stdout".to_string())
        })?;

        // Use a separate thread to write to cat-file's stdin while reading its stdout.
        // This prevents a deadlock when the pipe buffers fill up.
        let stdin_handle = std::thread::spawn(move || -> io::Result<()> {
            let mut reader = BufReader::new(rev_list_stdout);
            let mut line = String::new();

            while reader.read_line(&mut line)? > 0 {
                if let Some(oid) = line.split_whitespace().next() {
                    stdin.write_all(oid.as_bytes())?;
                    stdin.write_all(b"\n")?;
                }
                line.clear();
            }
            drop(stdin); // Close stdin to signal end of input
            Ok(())
        });

        let mut total = 0u64;
        let mut blob_count = 0u64;
        let mut object_count = 0u64;

        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let line = line?;
            object_count += 1;
            let mut parts = line.split_whitespace();
            let _oid = parts.next();
            let kind = parts.next();
            let size = parts.next();
            if kind == Some("blob") {
                if let Some(s) = size {
                    if let Ok(s_u64) = s.parse::<u64>() {
                        total += s_u64;
                        blob_count += 1;
                    }
                }
            }
        }

        // Ensure the stdin writing thread finished successfully
        stdin_handle
            .join()
            .map_err(|_| GitSizeError::Command("Stdin thread panicked".to_string()))?
            .map_err(|e| GitSizeError::Command(format!("Failed writing to stdin: {}", e)))?;

        // Clean up processes
        cat_file.wait().map_err(|e| {
            GitSizeError::Command(format!("Failed to wait for git cat-file: {}", e))
        })?;
        rev_list.wait().map_err(|e| {
            GitSizeError::Command(format!("Failed to wait for git rev-list: {}", e))
        })?;

        if debug {
            println!("  Objects: {}, Blobs: {}", object_count, blob_count);
            println!(
                "  Packed size: {}, Uncompressed size: {}",
                format_size(packed_size),
                format_size(total)
            );
        }

        Some(total)
    } else {
        if debug {
            println!("  Packed size: {}", format_size(packed_size));
        }
        None
    };

    Ok((packed_size, uncompressed_size))
}

/// Format a byte count into a human-readable string (B, KB, MB, GB).
///
/// This function converts a size in bytes to a human-readable format
/// using decimal prefixes (1 KB = 1000 bytes).
///
/// # Arguments
///
/// * `size` - The size in bytes to format
///
/// # Examples
///
/// ```
/// assert_eq!(format_size(0), "0 B");
/// assert_eq!(format_size(1500), "1.50 KB");
/// assert_eq!(format_size(2500000), "2.50 MB");
/// assert_eq!(format_size(5500000000), "5.50 GB");
/// ```
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

/// Generate a cumulative size over time plot using the `plotters` library.
///
/// This creates a PNG file at `output_path` displaying repository growth
/// based on the provided size measurement data.
fn generate_plot(data: &[SizeMeasurement], output_path: &Path) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }

    let plot_data: Vec<(i64, u64)> = data
        .iter()
        .filter_map(|d| {
            NaiveDate::parse_from_str(&d.date, "%Y-%m-%d")
                .ok()
                .and_then(|dt| {
                    dt.and_hms_opt(0, 0, 0)
                        .map(|naive| naive.and_utc().timestamp())
                        .map(|ts| (ts, d.cumulative_size))
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
        let git_dir = repo_path.join(".git");
        let is_git_dir = git_dir.exists();

        // Check if it might be a bare repository
        let config_file = repo_path.join("config");
        let is_bare = !is_git_dir && config_file.exists();

        let context = if is_bare {
            format!(
                "Cannot open repository at {:?}. The path appears to be a bare git repository. \
                git-size-history requires a non-bare repository with a working directory. \
                Either clone this repository to a working directory or use a regular git repository path. \
                Git error: {}",
                repo_path, e
            )
        } else if is_git_dir {
            format!(
                "Cannot open repository at {:?}. The .git directory exists but may be corrupted or inaccessible. \
                Try running 'git fsck' to check repository integrity. \
                Git error: {}",
                repo_path, e
            )
        } else {
            format!(
                "Cannot open repository at {:?}. Path is not a git repository (no .git directory found). \
                Make sure you're pointing to a valid git repository. \
                Git error: {}",
                repo_path, e
            )
        };
        GitSizeError::Validation(context)
    })?;

    // Progress bar for analysis phase - use indeterminate spinner during commit reading
    let analysis_pb = ProgressBar::new_spinner();
    analysis_pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} [{elapsed_precise}] {msg}")
            .map_err(|e| {
                GitSizeError::Validation(format!("Failed to set progress style: {}", e))
            })?,
    );
    analysis_pb.enable_steady_tick(std::time::Duration::from_millis(100));
    analysis_pb.set_message("Reading commit history...");

    // Get commit range
    let range = get_commit_range(&repo, &repo_path, &analysis_pb)?;
    let total_commits = range.total_commits;

    let first_ts = range.first_commit.time().seconds();
    let last_ts = range.last_commit.time().seconds();
    let first_dt = DateTime::from_timestamp(first_ts, 0).ok_or_else(|| {
        GitSizeError::Validation(format!("Invalid first commit timestamp: {}", first_ts))
    })?;
    let last_dt = DateTime::from_timestamp(last_ts, 0).ok_or_else(|| {
        GitSizeError::Validation(format!("Invalid last commit timestamp: {}", last_ts))
    })?;

    let duration = last_dt - first_dt;
    let years = duration.num_days() as f64 / DAYS_PER_YEAR;

    // Determine sampling strategy
    let use_yearly = args.yearly || (!args.monthly && years > YEARLY_THRESHOLD_YEARS);
    analysis_pb.set_message(format!(
        "Found {} commits ({} to {}, {:.1} years) - {} sampling",
        total_commits,
        first_dt.format("%Y-%m-%d"),
        last_dt.format("%Y-%m-%d"),
        years,
        if use_yearly { "yearly" } else { "monthly" }
    ));

    // Generate sample points
    let samples = generate_sample_points(&repo_path, &range, args.monthly, args.yearly)?;
    analysis_pb.set_message(format!("Generated {} sample points", samples.len()));
    analysis_pb.finish_with_message("Analysis complete");

    // Progress bar for sampling phase - shows complete commits count
    let pb = ProgressBar::new(samples.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
            )
            .map_err(|e| GitSizeError::Validation(format!("Failed to set progress style: {}", e)))?
            .progress_chars("=>-"),
    );
    pb.enable_steady_tick(std::time::Duration::from_millis(100));

    // Wrap progress bar in Arc for thread-safe updates
    // indicatif::ProgressBar is already thread-safe using atomics
    let pb = std::sync::Arc::new(pb);

    // Measure sizes in parallel for better performance
    // Using rayon to process multiple sample points concurrently
    let results: Vec<SizeMeasurement> = samples
        .par_iter()
        .map(|sample| {
            let (packed_size, uncompressed_size) = measure_size_at_commit(
                &repo_path,
                &sample.commit_hash,
                args.debug,
                args.uncompressed,
            )?;

            // Thread-safe progress bar increment (indicatif uses atomics internally)
            pb.inc(1);

            Ok(SizeMeasurement {
                date: sample.date.clone(),
                cumulative_size: packed_size,
                uncompressed_size,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    // Finish progress bar
    if let Ok(inner_pb) = std::sync::Arc::try_unwrap(pb) {
        inner_pb.finish_with_message("Sampling complete");
    }

    // Write CSV
    println!("Writing CSV to {}", args.output.display());
    let mut wtr = Writer::from_path(&args.output)?;
    if args.uncompressed {
        wtr.write_record(["date", "cumulative-size", "uncompressed-size"])?;
        for data in &results {
            wtr.write_record([
                &data.date,
                &data.cumulative_size.to_string(),
                &data.uncompressed_size.unwrap_or(0).to_string(),
            ])?;
        }
    } else {
        wtr.write_record(["date", "cumulative-size"])?;
        for data in &results {
            wtr.write_record([&data.date, &data.cumulative_size.to_string()])?;
        }
    }
    wtr.flush()?;

    // Generate plot
    if let Some(plot_path) = &args.plot {
        println!("Generating plot: {}", plot_path.display());
        generate_plot(&results, plot_path)?;
        println!("Plot saved to {}", plot_path.display());
    }

    // Print summary
    println!("\n=== Summary ===");
    println!("Repository: {}", repo_path.display());
    println!("Total commits analyzed: {}", range.total_commits);
    println!(
        "Time span: {} to {} ({:.1} years)",
        first_dt.format("%Y-%m-%d"),
        last_dt.format("%Y-%m-%d"),
        years
    );
    println!("Sample points: {}", results.len());
    println!(
        "Sampling method: {}",
        if use_yearly { "yearly" } else { "monthly" }
    );

    if let Some(first) = results.first() {
        println!(
            "Initial size ({}): {}",
            first.date,
            format_size(first.cumulative_size)
        );
    }
    if let Some(last) = results.last() {
        println!(
            "Final size ({}): {}",
            last.date,
            format_size(last.cumulative_size)
        );
    }

    if results.len() >= 2 {
        if let (Some(first), Some(last)) = (results.first(), results.last()) {
            let growth = last.cumulative_size.saturating_sub(first.cumulative_size);
            println!("Total growth: {}", format_size(growth));
        }
    }

    if args.uncompressed {
        if let Some(last) = results.last() {
            if let Some(uncompressed) = last.uncompressed_size {
                println!("Final uncompressed size: {}", format_size(uncompressed));
            }
        }
    }

    println!("\nOutput written to {}", args.output.display());
    if let Some(plot_path) = &args.plot {
        println!("Plot saved to {}", plot_path.display());
    }

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

    #[test]
    fn test_integration_minimal_repo() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("git-size-test-{}", timestamp));
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Initialize repo
        let repo = git2::Repository::init(&temp_dir).unwrap();
        let signature = git2::Signature::now("test", "test@example.com").unwrap();

        // Create initial commit
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let oid = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "initial commit",
                &tree,
                &[],
            )
            .unwrap();

        // Test get_commit_range
        let pb = ProgressBar::hidden();
        let range = get_commit_range(&repo, &temp_dir, &pb).unwrap();
        assert_eq!(range.total_commits, 1);

        // Test sampling
        let samples = generate_sample_points(&temp_dir, &range, false, false).unwrap();
        assert!(!samples.is_empty());

        // Test size measurement (at least check if it runs without error)
        let (packed, _) =
            measure_size_at_commit(&temp_dir, &oid.to_string(), false, false).unwrap();
        assert!(packed > 0);

        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_generate_sample_points_yearly_strategy() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("git-size-yearly-test-{}", timestamp));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let repo = git2::Repository::init(&temp_dir).unwrap();
        let signature = git2::Signature::now("test", "test@example.com").unwrap();

        // Create commits by appending content to the same file
        for i in 0..50 {
            let file_path = temp_dir.join("test.txt");
            std::fs::write(&file_path, format!("Content {}\n", i)).unwrap();

            let mut index = repo.index().unwrap();
            index.add_path(Path::new("test.txt")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();

            let head = repo.head().ok();
            let parent = head.as_ref().and_then(|h| h.peel_to_commit().ok());
            let parents: Vec<&git2::Commit> = parent.iter().collect();

            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                &format!("commit {}", i),
                &tree,
                parents.as_slice(),
            )
            .unwrap();
        }

        let pb = ProgressBar::hidden();
        let range = get_commit_range(&repo, &temp_dir, &pb).unwrap();

        // Force monthly sampling for this test
        let samples = generate_sample_points(&temp_dir, &range, true, false).unwrap();

        // Should have at least one sample (the final commit)
        // Note: Since all commits are created at nearly the same time,
        // the sampling may only produce one point
        assert!(!samples.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_generate_sample_points_forced_yearly() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir =
            std::env::temp_dir().join(format!("git-size-force-yearly-test-{}", timestamp));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let repo = git2::Repository::init(&temp_dir).unwrap();
        let signature = git2::Signature::now("test", "test@example.com").unwrap();

        // Create a few commits
        for i in 0..5 {
            let file_path = temp_dir.join("test.txt");
            std::fs::write(&file_path, format!("Content {}\n", i)).unwrap();

            let mut index = repo.index().unwrap();
            index.add_path(Path::new("test.txt")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();

            let head = repo.head().ok();
            let parent = head.as_ref().and_then(|h| h.peel_to_commit().ok());
            let parents: Vec<&git2::Commit> = parent.iter().collect();

            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                &format!("commit {}", i),
                &tree,
                parents.as_slice(),
            )
            .unwrap();
        }

        let pb = ProgressBar::hidden();
        let range = get_commit_range(&repo, &temp_dir, &pb).unwrap();

        // Force yearly sampling
        let samples = generate_sample_points(&temp_dir, &range, false, true).unwrap();

        // Should have at least start and end
        assert!(!samples.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_generate_sample_points_forced_monthly() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir =
            std::env::temp_dir().join(format!("git-size-force-monthly-test-{}", timestamp));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let repo = git2::Repository::init(&temp_dir).unwrap();
        let signature = git2::Signature::now("test", "test@example.com").unwrap();

        // Create commits
        for i in 0..10 {
            let file_path = temp_dir.join("test.txt");
            std::fs::write(&file_path, format!("Content {}\n", i)).unwrap();

            let mut index = repo.index().unwrap();
            index.add_path(Path::new("test.txt")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();

            let head = repo.head().ok();
            let parent = head.as_ref().and_then(|h| h.peel_to_commit().ok());
            let parents: Vec<&git2::Commit> = parent.iter().collect();

            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                &format!("commit {}", i),
                &tree,
                parents.as_slice(),
            )
            .unwrap();
        }

        let pb = ProgressBar::hidden();
        let range = get_commit_range(&repo, &temp_dir, &pb).unwrap();

        // Force monthly sampling
        let samples = generate_sample_points(&temp_dir, &range, true, false).unwrap();

        assert!(!samples.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_csv_output_format() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("git-size-csv-test-{}", timestamp));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let repo = git2::Repository::init(&temp_dir).unwrap();
        let signature = git2::Signature::now("test", "test@example.com").unwrap();
        let tree_id = repo.index().unwrap().write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let _oid = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "initial commit",
                &tree,
                &[],
            )
            .unwrap();

        let output_path = temp_dir.join("output.csv");

        // Test CSV writing directly
        let mut wtr = Writer::from_path(&output_path).unwrap();
        wtr.write_record(["date", "cumulative-size"]).unwrap();
        wtr.write_record(["2024-01-01", "1234"]).unwrap();
        wtr.flush().unwrap();

        // Verify CSV content
        let content = std::fs::read_to_string(&output_path).unwrap();
        assert!(content.contains("date,cumulative-size"));
        assert!(content.contains("2024-01-01,1234"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_measure_size_with_uncompressed() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("git-size-uncomp-test-{}", timestamp));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let repo = git2::Repository::init(&temp_dir).unwrap();
        let signature = git2::Signature::now("test", "test@example.com").unwrap();

        // Create a commit with some content
        let file_path = temp_dir.join("test.txt");
        std::fs::write(&file_path, "Hello, World! This is some test content.").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let oid = repo
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "commit with content",
                &tree,
                &[],
            )
            .unwrap();

        // Test with uncompressed calculation
        let (packed, uncompressed) =
            measure_size_at_commit(&temp_dir, &oid.to_string(), false, true).unwrap();

        assert!(packed > 0);
        assert!(uncompressed.is_some());
        assert!(uncompressed.unwrap() > 0);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_error_handling_invalid_path() {
        let invalid_path = PathBuf::from("/nonexistent/path/to/repo");
        let result = Repository::open(&invalid_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_error_handling_empty_repo() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("git-size-empty-test-{}", timestamp));
        std::fs::create_dir_all(&temp_dir).unwrap();

        // Initialize repo but don't create any commits
        let repo = git2::Repository::init(&temp_dir).unwrap();

        // get_commit_range should return an error for empty repo
        let pb = ProgressBar::hidden();
        let result = get_commit_range(&repo, &temp_dir, &pb);
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_multi_commit_repository() {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!("git-size-multi-test-{}", timestamp));
        std::fs::create_dir_all(&temp_dir).unwrap();

        let repo = git2::Repository::init(&temp_dir).unwrap();
        let signature = git2::Signature::now("test", "test@example.com").unwrap();

        // Create multiple commits by modifying the same file
        let mut commits = Vec::new();
        for i in 0..5 {
            let file_path = temp_dir.join("test.txt");
            std::fs::write(&file_path, format!("Content of file {}\n", i)).unwrap();

            let mut index = repo.index().unwrap();
            index.add_path(Path::new("test.txt")).unwrap();
            index.write().unwrap();
            let tree_id = index.write_tree().unwrap();
            let tree = repo.find_tree(tree_id).unwrap();

            let head = repo.head().ok();
            let parent = head.as_ref().and_then(|h| h.peel_to_commit().ok());
            let parents: Vec<&git2::Commit> = parent.iter().collect();

            let oid = repo
                .commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    &format!("commit {}", i),
                    &tree,
                    parents.as_slice(),
                )
                .unwrap();
            commits.push(oid);
        }

        // Test get_commit_range
        let pb = ProgressBar::hidden();
        let range = get_commit_range(&repo, &temp_dir, &pb).unwrap();
        assert_eq!(range.total_commits, 5);

        // Test sampling
        let samples = generate_sample_points(&temp_dir, &range, false, false).unwrap();
        assert!(!samples.is_empty());

        // Test size measurement at different commits
        for (i, commit_oid) in commits.iter().enumerate() {
            let (packed, _) =
                measure_size_at_commit(&temp_dir, &commit_oid.to_string(), false, false).unwrap();
            assert!(packed > 0, "Size measurement failed for commit {}", i);
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
