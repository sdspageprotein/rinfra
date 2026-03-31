# Contributing to rinfra

Thank you for your interest in contributing to rinfra! This document provides guidelines for contributing.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/<your-username>/rinfra.git`
3. Create a feature branch: `git checkout -b feature/my-feature`
4. Make your changes
5. Submit a pull request

## Development Setup

### Prerequisites

- Rust 1.88+ (stable)
- Node.js 22+ (for admin frontend)
- Docker (optional, for integration testing)

### Build

```bash
cargo build
```

### Test

```bash
cargo test
```

### Lint

```bash
cargo fmt --check
cargo clippy -- -D warnings
```

### Admin Frontend

```bash
cd rinfra-admin/frontend
npm ci
npm run build
```

## Code Guidelines

- **Language**: All code comments and commit messages must be in English.
- **Error handling**: Use `AppError` with descriptive `ErrorCode` values. Never use vague error messages.
- **Logging**: Use `tracing` with appropriate log levels. Include contextual fields. Never log secrets.
- **Testing**: Core logic must have unit tests. Use `test_<function>_<scenario>_<expected>` naming.
- **Documentation**: Written in Chinese (简体中文) for specs and design docs. Code docs in English.

## Pull Request Process

1. Ensure `cargo fmt`, `cargo clippy`, and `cargo test` all pass.
2. Update documentation if your change affects public APIs or behavior.
3. Keep PRs focused — one feature or fix per PR.
4. Write a clear PR description explaining **why** the change is needed.

## Reporting Issues

- Use GitHub Issues for bug reports and feature requests.
- Include reproduction steps, expected behavior, and actual behavior for bugs.
- Check existing issues before creating a new one.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
