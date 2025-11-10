# queensac

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Coverage Status](https://coveralls.io/repos/github/reddevilmidzy/queensac/badge.svg?branch=main)](https://coveralls.io/github/reddevilmidzy/queensac?branch=main)
[![ci](https://github.com/reddevilmidzy/queensac/actions/workflows/rust_ci.yml/badge.svg)](https://github.com/reddevilmidzy/queensac/actions/workflows/rust_ci.yml)
[![release](https://img.shields.io/github/v/release/reddevilmidzy/queensac?label=release)](https://github.com/reddevilmidzy/queensac/releases)

<br>

[English](README.md) | [한국어](README.ko.md)

## Introduction

queensac은 자동화된 링크 검증 및 수정 서비스입니다. GitHub 레포지토리 내의 깨진 링크를 자동으로 감지하고, 가능한 경우 올바른 링크로 수정하여 Pull Request를 생성합니다. 이로써 오픈소스 프로젝트의 문서 품질을 향상시키고, 사용자에게 더 나은 경험을 제공합니다.

지금까지 수많은 링크 체크 도구가 있었지만 이는 깨진 링크를 찾아서 보고하기만 할 뿐, 그 이상의 기능은 수행하지 않았습니다. 그래서 ci 워크플로우에도 링크 검증 로직이 있지만 해당 로직이 실패해도 그냥 무시하고 넘기는 경우가 허다했습니다. queensac은 이 문제를 해결하고자 등장했습니다.

## Features

- 링크 추출 및 유효성 검사: 레포지토리에서 모든 링크를 추출하고, 404 Not Found 등의 오류를 확인합니다.
- 대체 링크 탐색 및 수정: 깨진 링크에 대한 대체 가능한 올바른 링크를 찾아 변환합니다.
- Pull Request 생성: 변경 사항을 반영한 Pull Request를 생성합니다.

## Getting Started

[queensac[bot]](https://github.com/apps/queensac)을 사용하고자 하는 GitHub 레포지토리에 설치합니다.

GitHub Workflow에 아래의 내용을 추가합니다.

```yaml
- name: 👑 Run queensac
  uses: reddevilmidzy/queensac@v1
  with:
    github_token: ${{ secrets.GITHUB_TOKEN }}
```

## Contributing

여러분의 기여는 언제나 환영입니다. 버그 리포트, 새로운 기능 제안 등은 [Issue](https://github.com/reddevilmidzy/queensac/issues)를 통해 부탁드립니다.
자세한 기여 가이드 및 개발 환경 설정 방법은 [CONTRIBUTING.md](CONTRIBUTING.md) 파일을 참고해 주세요.

## License

이 프로젝트는 Apache-2.0 라이센스를 따릅니다. 자세한 내용은 [LICENSE](LICENSE) 파일을 참고해 주세요.
