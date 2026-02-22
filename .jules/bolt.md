## 2026-02-22 - [Descending Binary Search]
**Learning:** Rust's `binary_search_by` requires the closure to return `target.cmp(element)` for descending lists to correctly partition the search space (Less for left, Greater for right).
**Action:** Use `target.cmp(element)` when searching descending lists and document it with a comment.

## 2026-02-22 - [Git Bitmap Optimization]
**Learning:** Using `--use-bitmap-index` with `git rev-list --objects --disk-usage` provides massive speedups for reachability analysis in large repositories.
**Action:** Always include this flag when calculating disk usage from a commit hash.
