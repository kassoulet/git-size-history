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
