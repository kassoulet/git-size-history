# Git Size History

[![CI](https://github.com/kassoulet/git-size-history/actions/workflows/ci.yml/badge.svg)](https://github.com/kassoulet/git-size-history/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.75+-blue.svg)](https://github.com/kassoulet/git-size-history)

**Git Size History** is a fast CLI tool that analyzes how a git repository's size has grown over time by sampling commits at regular intervals and measuring packed object sizes.

## Features

- **Fast**: Efficient size measurement
- **Visual**: Generates PNG plots of size over time
- **Safe**: Read-only operations, never modifies your repository
- **Cross-platform**: Works on Linux, macOS, and Windows

```
$ time target/release/git-size-fast ~/tmp/linux -o linux.csv --plot linux.png
  Repository spans 2005-04-16 to 2026-02-17 (20.8 years, 1425993 commits)
  [00:00:58] Analysis
  [00:14:34] [========================================] 22/22 Sampling
Writing CSV to linux.csv
Generating plot: linux.png

real	15m33.510s
```

Scanning the size of nearly 1.5M commits in 15 minutes!

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

![Example Plot](.github/plot-example.png)

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
| **Insightful** | Compression ratio reveals repository health |

## Performance

Typical performance characteristics:

| Repository Size | Commits | Time |
|-----------------|---------|------|
| Small | <100 | <1 second |
| Medium | 100-1,000 | 5-30 seconds |
| Large | 1,000-10,000 | 1-5 minutes |
| Very Large | >10,000 | 5-15 minutes |

**Factors affecting performance:**
- Number of sample points (yearly vs monthly)
- Total commit count
- Disk I/O speed
- Object database size

## Requirements

| Dependency | Version |
|------------|---------|
| Rust | 1.75 or later |
| Git | 2.0 or later |

### Optional Dependencies

- `awk`: Used for efficient object ID extraction (pre-installed on most Unix systems)

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

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Areas for Contribution

- 📝 Additional documentation
- 🧪 More test coverage
- 🐛 Bug fixes
- ✨ New features (see [Issues](https://github.com/example/git-size-history/issues))
- 🌍 Internationalization

## License

This project is licensed under the MIT License - see [LICENSE](LICENSE) for details.

## Acknowledgments

- Inspired by [How to Calculate Git Repository Growth Over Time](https://www.lullabot.com/articles/how-calculate-git-repository-growth-over-time) by Andrew Berry
- Built with:
  - [clap](https://github.com/clap-rs/clap) - Command-line argument parser
  - [git2](https://github.com/rust-lang/git2-rs) - Git bindings for Rust
  - [plotters](https://github.com/plotters-rs/plotters-rs) - Plotting library
  - [indicatif](https://github.com/console-rs/indicatif) - Progress bars

## Related Projects

- [git-annex](https://git-annex.branchable.com/) - File synchronization with git
- [git-lfs](https://git-lfs.com/) - Git Large File Storage
- [git-sizer](https://github.com/github/git-sizer) - Compute various size metrics for a git repository
