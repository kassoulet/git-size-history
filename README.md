# Git Size History

[![CI](https://github.com/kassoulet/git-size-history/actions/workflows/ci.yml/badge.svg)](https://github.com/kassoulet/git-size-history/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75+-blue.svg)](https://github.com/kassoulet/git-size-history)

**Git Size History** is an experimental fast CLI tool that analyzes how a git repository's size has grown over time by sampling commits at regular intervals and measuring packed object sizes.

## Features

- **Fast**: Efficient size measurement, multithreaded processing
- **Visual**: Generates PNG plots of size over time (and CSV data for further analysis)
- **Safe**: Read-only operations, never modifies your repository
- **Cross-platform**: Works on Linux, macOS, and Windows
- **Use Bitmap Index**: Leverages git's bitmap index for fast object counting and size estimation

> Use ```git repack -a -d --write-bitmap-index``` to create a bitmap index for faster analysis on large repositories.

```
$ time target/release/git-size-history ~/tmp/linux -o linux-bm.csv --plot linux-bm.png
  [00:00:37] Analysis complete                                                                                                                                                            [00:00:04] [========================================] 22/22 Sampling complete                                                                                                         Writing CSV to linux-bm.csv
Generating plot: linux-bm.png
Plot saved to linux-bm.png

=== Summary ===
Repository: /home/gautier/tmp/linux
Total commits analyzed: 1426552
Time span: 2005-04-16 to 2026-02-21 (20.8 years)
Sample points: 22
Sampling method: yearly
Initial size (2005-04-16): 53.14 MB
Final size (2026-02-21): 6.20 GB
Total growth: 6.15 GB

Output written to linux-bm.csv
Plot saved to linux-bm.png

real	0m43,268s
```

Scanning the size history of 1.4M commits in 43 seconds!

(Without bitmap index, it takes 3 minutes)

![](linux.png)

## Installation

### From Source

```bash
git clone https://github.com/example/git-size-history.git
cd git-size-history
cargo build --release
```

The binary will be at `target/release/git-size-history`.

## Quick Start

```bash
# Analyze current directory
git-size-history -o output.csv

# Analyze specific repository with plot
git-size-history /path/to/repo -o output.csv --plot size-over-time.png
```

## Usage

### Options

| Option | Description |
|--------|-------------|
| `<REPO_PATH>` | Path to git repository (default: `.`) |
| `-o, --output <FILE>` | Output CSV file path **(required)** |
| `--plot <FILE>` | Generate PNG plot of cumulative size |
| `--yearly` | Force yearly sampling |
| `--monthly` | Force monthly sampling (default for repos ≤6 years) |
| `-D, --debug` | Show debug output (object counts, sizes) |
| `-U, --uncompressed` | Calculate uncompressed blob sizes (slower) |
| `-h, --help` | Print help |
| `-V, --version` | Print version |

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

## Output

### CSV Format

The output CSV contains size measurements over time:

```csv
date,cumulative-size,uncompressed-size
2020-01-15,1048576,10485760
2021-01-15,2097152,20971520
2022-01-15,4194304,41943040
```

| Column | Description |
|--------|-------------|
| `date` | Sampling date in YYYY-MM-DD format |
| `cumulative-size` | Packed repository size in bytes (after `git gc`) |
| `uncompressed-size` | Total uncompressed blob size (only with `-U` flag) |

**Tip**: The ratio between uncompressed and packed size shows git's compression efficiency (typically 5-10x).

### Plot

The generated PNG plot displays:
- **X-axis**: Timeline with year-month labels
- **Y-axis**: Repository size with automatic unit scaling (B, KB, MB, GB)
- **Line**: Cumulative packed size over time

## How It Works

### Sampling Strategy

Git Size History uses an adaptive sampling approach:

| Repository Age | Sampling Interval |
|----------------|-------------------|
| > 6 years | Yearly (365 days) |
| ≤ 6 years | Monthly (30 days) |

The latest commit is always included as the final sample point.

### Size Measurement

For each sample point:

1. **Find Nearest Commit**: Binary search for commit closest to sample date
2. **Packed Size**: `git rev-list --objects --disk-usage` measures actual disk usage
3. **Uncompressed Size** (optional): `git cat-file --batch-check` sums all blob sizes

### Why This Approach?

| Benefit | Description |
|---------|-------------|
| **Accurate** | Measures actual disk usage after git compression |
| **Fast** | No cloning or temporary repositories needed |
| **Safe** | Read-only operations, never modifies the repository |
| **Efficient** | Uses git's batch mode for high-performance queries |

## Requirements

| Dependency | Version |
|------------|---------|
| Rust | 1.75 or later |
| Git | 2.0 or later |

## Troubleshooting

### "Cannot open repository"

Ensure the path points to a valid git repository:

```bash
cd /path/to/repo && git status
```

### "Failed to get disk usage"

Check that git is installed and accessible:

```bash
git --version
git rev-list --objects --disk-usage HEAD
```

### Plot generation fails

Ensure you have write permissions in the output directory:

```bash
ls -la /path/to/output/directory
```

### Analysis is slow

Try these optimizations:

1. Use `--yearly` flag to reduce sample points
2. Skip uncompressed calculation (don't use `-U`)
3. Ensure repository is on fast storage (SSD recommended)
4. Run `git gc` on the repository first

### High Memory Usage

Analyzing large repositories (e.g., Linux kernel with 1.4M+ commits) can consume significant memory due to parallel processing. We use [Rayon](https://github.com/rayon-rs/rayon) for parallelism, which by default uses all available CPU cores.

**To limit memory usage:**

1. **Reduce parallel threads** using `RAYON_NUM_THREADS`:
   ```bash
   # Limit to 2 threads (reduces memory pressure)
   RAYON_NUM_THREADS=2 git-size-history -o output.csv /path/to/repo
   ```

## Development

### Build from Source

```bash
git clone https://github.com/example/git-size-history.git
cd git-size-history
cargo build --release
```

### Run Tests

```bash
cargo test
```

### Check Code Quality

```bash
cargo fmt -- --check
cargo clippy -- -D warnings
```

## License

This project is licensed under the MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- Inspired by [How to Calculate Git Repository Growth Over Time](https://www.lullabot.com/articles/how-calculate-git-repository-growth-over-time) by Andrew Berry
- Built with:
  - [chrono](https://github.com/chronotope/chrono) - Date and time handling
  - [clap](https://github.com/clap-rs/clap) - Command-line argument parser
  - [csv](https://github.com/BurntSushi/rust-csv) - CSV parsing and writing
  - [git2](https://github.com/rust-lang/git2-rs) - Git bindings for Rust
  - [indicatif](https://github.com/console-rs/indicatif) - Progress bars
  - [plotters](https://github.com/plotters-rs/plotters-rs) - Plotting library
  - [rayon](https://github.com/rayon-rs/rayon) - Data parallelism library

## Related Projects

- [git-annex](https://git-annex.branchable.com/) - File synchronization with git
- [git-lfs](https://git-lfs.com/) - Git Large File Storage
- [git-sizer](https://github.com/github/git-sizer) - Compute various size metrics for a git repository, and spot problematic usages
