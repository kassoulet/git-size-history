## 2026-02-22 - Piping Deadlock in External Command Execution
**Vulnerability:** Denial of Service (DoS) via process deadlock when calculating uncompressed size on large repositories.
**Learning:** When piping the output of one `std::process::Command` to the input of another, a deadlock can occur if the second process's stdout is not being read while its stdin is being written to. If the second process's stdout buffer fills up, it will block on write, which in turn causes the first process (or the Rust program writing to stdin) to block on write when the stdin buffer also fills up.
**Prevention:** Use a separate thread to write to the child process's stdin while the main thread reads its stdout (e.g., via `wait_with_output()`), or use asynchronous IO.
