# Changelog

All notable changes to git-size-fast will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Comprehensive documentation for public functions
- Unit and integration tests for core logic
- Constants for magic numbers in sampling strategy

### Changed
- Refactored `measure_size_at_commit` to remove `awk` dependency and use pure Rust pipe processing
- Improved progress bar accuracy and messages during analysis
- Replaced `unwrap()` calls with proper error handling in `get_commit_range`

### Fixed
- Fixed shell injection vulnerability in git command execution

### Security
- Eliminated shell injection risk by avoiding `bash -c` and using direct argument passing for `Command`

## [0.1.0] - 2024-02-19

### Added
- Initial release with core functionality
