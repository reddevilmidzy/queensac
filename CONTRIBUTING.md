# Contributing to queensac

Thank you for your interest in contributing to queensac!! We welcome all kinds of contributions, from bug reports to feature requests and code contributions.

## Getting Started

1. Fork the repository
2. Clone your fork locally:
   ```bash
   git clone https://github.com/your-username/queensac.git
   cd queensac
   ```
3. Install Rust if you haven't already: https://rustup.rs/
4. Build the project:
   ```bash
   cargo build
   ```

## Development Workflow

### Making Changes

1. Create a new branch for your changes (any branch naming convention is fine)
2. Make your changes
3. Test your changes thoroughly

### Commit Convention

This project follows the [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/) specification for commit messages.

### Code Quality Checks

Before submitting your pull request, please ensure that your code passes all the following checks:

#### 1. Linting with Clippy
Run Clippy to catch common mistakes and improve code quality:
```bash
cargo clippy -- -D warnings
```
This command will treat all warnings as errors. Please fix any issues that arise.

#### 2. Code Formatting
Ensure your code follows the standard Rust formatting:
```bash
cargo fmt -- --check
```
If this command fails, run `cargo fmt` to automatically format your code.

#### 3. Tests
Make sure all tests pass:
```bash
cargo test
```

## Submitting Changes

1. Push your changes to your fork
2. Create a Pull Request with:
   - A clear title describing your changes
   - A detailed description of what you've changed and why
   - Any relevant issue numbers (e.g., "Fixes #123")

## Types of Contributions

- **Bug Reports**: Use GitHub Issues to report bugs
- **Feature Requests**: Suggest new features through GitHub Issues
- **Documentation**: Improve README, code comments, or other documentation
- **Code**: Bug fixes, feature implementations, performance improvements

## Code Guidelines

- Write clear, readable code with appropriate comments
- Follow Rust best practices and idioms
- Add tests for new functionality
- Update documentation as needed

## Getting Help

If you have questions about contributing, feel free to:
- Open an issue for discussion
- Look at existing issues and PRs for examples
- Reach out to the maintainers

Thank you for contributing to queensac!!
