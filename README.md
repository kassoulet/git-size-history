# Git Size History

[![CI](https://github.com/example/git-size-history/actions/workflows/ci.yml/badge.svg)](https://github.com/example/git-size-history/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Crates.io](https://img.shields.io/crates/v/git-size-history.svg)](https://crates.io/crates/git-size-history)

Fast git repository size-over-time analysis using commit sampling.

## Overview

`git-size-fast` analyzes how a git repository's size has grown over time by:

1. Determining the repository's time span from first to last commit
2. Sampling at regular intervals (yearly for repos >6 years, monthly otherwise)
3. For each sample point:
   - Finding the nearest commit to the sample date
   - Using `git rev-list --objects` to enumerate all blobs reachable from that commit
   - Summing blob sizes using `git cat-file --batch-check`
4. Outputting results as CSV and optionally generating a PNG plot

## Installation

### From Source

```bash
git clone https://github.com/example/git-size-history.git
cd git-size-history
cargo build --release
```

The binary will be at `target/release/git-size-history`.

### From Crates.io (coming soon)

```bash
cargo install git-size-history
```

## Usage

### Basic Usage

```bash
# Analyze current directory
git-size-history -o output.csv

# Analyze specific repository
git-size-history /path/to/repo -o output.csv

# Generate plot
git-size-history -o output.csv --plot size-over-time.png /path/to/repo
```

### Options

| Option | Description |
|--------|-------------|
| `<REPO_PATH>` | Path to git repository (default: current directory) |
| `-o, --output <FILE>` | Output CSV file path (required) |
| `--plot <FILE>` | Generate PNG plot of cumulative size |
| `--monthly` | Force monthly sampling |
| `--yearly` | Force yearly sampling |
| `-D, --debug` | Enable debug output (show object counts and sizes) |
| `-U, --uncompressed` | Also calculate uncompressed blob sizes (slower) |
| `-h, --help` | Print help information |
| `-V, --version` | Print version information |

### Examples

```bash
# Analyze a large repository with yearly sampling
git-size-history --yearly -o linux-size.csv --plot linux-size.png /path/to/linux

# Analyze current project with monthly sampling
git-size-history --monthly -o project-size.csv .

# Quick analysis with default settings
git-size-history -o output.csv /path/to/repo

# Show debug information during analysis
git-size-history -D -o output.csv /path/to/repo

# Include uncompressed sizes for compression ratio analysis
git-size-history -U -o output.csv /path/to/repo
```

## Output Format

### CSV

The output CSV contains size measurements over time:

```csv
date,cumulative-size,uncompressed-size
2020-01-15,1048576,10485760
2021-01-15,2097152,20971520
2022-01-15,4194304,41943040
```

- `date`: Sampling date in YYYY-MM-DD format
- `cumulative-size`: Packed repository size in bytes (after `git gc`)
- `uncompressed-size`: Total uncompressed blob size in bytes (only with `-U` flag)

The ratio between uncompressed and packed size shows the compression efficiency.

### Plot

The generated PNG plot shows:
- X-axis: Timeline with year-month labels
- Y-axis: Repository size with automatic unit scaling (B, KB, MB, GB)
- Blue line: Cumulative size over time

## Algorithm Details

### Sampling Strategy

- **Repositories > 6 years**: Yearly sampling (one sample per year)
- **Repositories ≤ 6 years**: Monthly sampling (one sample per 30 days)
- Always includes the latest commit

### Size Measurement

For each sample point:

1. **Packed Size**: Uses `git rev-list --objects --disk-usage` to get the actual disk space used by objects
2. **Uncompressed Size** (optional): Uses `git cat-file --batch-check` to sum individual blob sizes

### Why This Approach?

- **Accurate**: Measures actual disk usage after git compression
- **Fast**: No cloning or temporary repositories needed
- **Safe**: Read-only operations, never modifies the repository
- **Efficient**: Uses git's batch mode for high-performance object queries
- **Insightful**: Optional uncompressed size shows compression efficiency

## Performance

Typical performance characteristics:

- **Small repos** (<100 commits): <1 second
- **Medium repos** (100-1000 commits): 5-30 seconds
- **Large repos** (>1000 commits): 1-5 minutes

Factors affecting performance:
- Number of sample points (yearly vs monthly)
- Repository size and commit count
- Disk I/O speed
- Network speed (for remote repositories)

## Requirements

- **Rust**: 1.75 or later
- **Git**: 2.0 or later

## Troubleshooting

### "Cannot open repository"

Ensure the path points to a valid git repository:
```bash
cd /path/to/repo && git status
```

### "Failed to clone repository"

Check that git is installed and the repository is accessible:
```bash
git --version
git ls-remote /path/to/repo
```

### Plot generation fails

Ensure you have write permissions in the output directory.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.

## License

This project is licensed under the MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- Inspired by https://www.lullabot.com/articles/how-calculate-git-repository-growth-over-time, thanks Andrew.
- Built with [clap](https://github.com/clap-rs/clap), [git2](https://github.com/rust-lang/git2-rs), and [plotters](https://github.com/plotters-rs/plotters-rs)
