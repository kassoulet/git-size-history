# Changelog

All notable changes to git-size-fast will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial release
- Yearly/monthly sampling based on repository age
- CSV output with date and cumulative size
- PNG plot generation
- Progress bar for long-running analyses
- Efficient blob size measurement using `git rev-list --objects` and `git cat-file`

### Changed
- Replaced shallow clone approach with direct object enumeration (much faster)
- Use `git gc` instead of `git gc --aggressive` for better performance
- No temporary repository clones needed

### Deprecated
- N/A

### Removed
- N/A

### Fixed
- N/A

### Security
- N/A

## [0.1.0] - 2024-02-19

### Added
- Initial release with core functionality
