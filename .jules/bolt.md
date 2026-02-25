## 2026-02-22 - [Descending Binary Search]
**Learning:** Rust's `binary_search_by` requires the closure to return `target.cmp(element)` for descending lists to correctly partition the search space (Less for left, Greater for right).
**Action:** Use `target.cmp(element)` when searching descending lists and document it with a comment.

## 2026-02-22 - [Git Bitmap Optimization]
**Learning:** Using `--use-bitmap-index` with `git rev-list --objects --disk-usage` provides massive speedups for reachability analysis in large repositories.
**Action:** Always include this flag when calculating disk usage from a commit hash.

## 2026-02-24 - [Git CLI for History Walking]
**Learning:** Spawning `git rev-list --format="%H %ct"` is significantly faster than using `git2`'s `revwalk` + `find_commit` because it avoids the overhead of full commit object parsing in Rust/libgit2.
**Action:** Prefer Git CLI for bulk history traversal when only OIDs and timestamps are needed.

## 2026-02-24 - [Streaming Process Output]
**Learning:** `wait_with_output()` buffers the entire stdout/stderr into memory, which can cause OOM for large repositories. Using `BufReader` to stream output line-by-line is much more memory-efficient.
**Action:** Always stream large process outputs in parallel or high-volume contexts.

## 2026-02-25 - [Allocation Reuse in IO Loops]
**Learning:** Reusing a `String` buffer with `reader.read_line(&mut line)` in history-walking or object-listing loops significantly reduces heap allocation pressure compared to `reader.lines()`.
**Action:** Use `read_line` with buffer reuse in all performance-critical parsing loops.

## 2026-02-25 - [Pre-formatting and Caching in Loops]
**Learning:** Redundant calls to `format()` or `timestamp()` inside tight loops (like walking 1M+ commits) add measurable overhead.
**Action:** Pre-format static data like target dates and cache timestamps outside of the loop.
