## 2026-02-22 - Piping Deadlock in External Command Execution
**Vulnerability:** Denial of Service (DoS) via process deadlock when calculating uncompressed size on large repositories.
**Learning:** When piping the output of one `std::process::Command` to the input of another, a deadlock can occur if the second process's stdout is not being read while its stdin is being written to. If the second process's stdout buffer fills up, it will block on write, which in turn causes the first process (or the Rust program writing to stdin) to block on write when the stdin buffer also fills up.
**Prevention:** Use a separate thread to write to the child process's stdin while the main thread reads its stdout (e.g., via `wait_with_output()`), or use asynchronous IO.

## 2026-02-22 - OOM DoS via wait_with_output()
**Vulnerability:** Memory exhaustion (OOM) Denial of Service when processing large command output.
**Learning:** Using `wait_with_output()` collects the entire stdout/stderr of a child process into memory. For commands like `git cat-file --batch-check` that can produce millions of lines of output in large repositories, this can quickly lead to memory exhaustion, especially when multiple such processes are run in parallel (e.g., via Rayon).
**Prevention:** Process child process output as a stream (e.g., using `BufReader`) whenever possible instead of collecting it all into memory.

## 2026-02-25 - Integrity Deception and Path Handling Robustness
**Vulnerability:** Potential deception by Git's object replacement mechanism and panics on non-UTF-8 repository paths.
**Learning:** Git analysis tools should use `--no-replace-objects` to ensure they analyze the raw repository state. Relying on `.to_str().unwrap()` for paths in `std::process::Command` is a common source of panics and limits support to UTF-8 paths.
**Prevention:** Always include `--no-replace-objects` in git calls for security audits. Use `AsRef<OsStr>` (via direct `Path` passing) in `Command::arg` to handle all valid OS paths safely.

## 2026-02-26 - OOM DoS in Root Commit Collection
**Vulnerability:** Potential OOM Denial of Service when collecting root commits using `Command::output()`.
**Learning:** Although most repositories have few root commits, collecting all output from `git rev-list --max-parents=0` into memory can lead to OOM if a malicious repository contains millions of root commits.
**Prevention:** Always stream the output of `git rev-list` using `BufReader` and a reused `String` buffer, even for commands expected to have small output.

## 2026-02-26 - Argument Injection in Git Revision Handling
**Vulnerability:** Potential argument injection when passing revisions or hashes to `git rev-list`.
**Learning:** Failing to use the `--` separator between revisions and pathspecs allows malicious or malformed hashes to be interpreted as command-line options.
**Prevention:** Always include the `--` separator in `git rev-list` and similar commands to clearly disambiguate revisions from paths, e.g., `git rev-list <rev> --`.
