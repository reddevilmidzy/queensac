# queensac

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Coverage Status](https://coveralls.io/repos/github/reddevilmidzy/queensac/badge.svg?branch=main)](https://coveralls.io/github/reddevilmidzy/queensac?branch=main)
[![ci](https://github.com/reddevilmidzy/queensac/actions/workflows/rust_ci.yml/badge.svg)](https://github.com/reddevilmidzy/queensac/actions/workflows/rust_ci.yml)
[![release](https://img.shields.io/github/v/release/reddevilmidzy/queensac?label=release)](https://github.com/reddevilmidzy/queensac/releases)

<br>

[English](README.md) | [한국어](README.ko.md)

## Introduction

queensac is an automated link validation and correction service. It automatically detects broken links within a GitHub repository and, when possible, corrects them to valid links and creates a Pull Request. This improves the documentation quality of open-source projects and provides a better experience for users.

Until now, there have been many link-checking tools, but they only reported broken links and did nothing more. Even though link validation logic exists in CI workflows, it was common to simply ignore failures and move on. queensac was created to solve this problem.

## Features

- Link extraction and validation: Extracts all links from the repository and checks for errors such as 404 Not Found.
- Alternative link search and correction: Finds and replaces broken links with valid alternatives.
- Pull Request creation: Creates a Pull Request reflecting the changes.

## Getting Started

Install [queensac[bot]](https://github.com/apps/queensac) on the GitHub repository you want to use.

Add the following to your GitHub Workflow:

```yaml
- name: 👑 Run queensac
  uses: reddevilmidzy/queensac@v1
  with:
    github_app_id: ${{ secrets.QUEENSAC_APP_ID }}
    github_app_private_key: ${{ secrets.QUEENSAC_APP_PRIVATE_KEY }}
    repo: https://github.com/${{ github.repository }}
```

## Contributing

Your contributions are always welcome. Please report bugs, suggest new features, etc. via [Issue](https://github.com/reddevilmidzy/queensac/issues).
For detailed contribution guidelines and development environment setup, please refer to the [CONTRIBUTING.md](CONTRIBUTING.md) file.

## License

This project follows the Apache-2.0 license. For more details, please refer to the [LICENSE](LICENSE) file.
