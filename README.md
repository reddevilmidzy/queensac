# queensac
![GitHub Stars](https://img.shields.io/github/stars/reddevilmidzy/queensac)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
![Security](https://img.shields.io/badge/security-audited-brightgreen.svg)
[![Coverage Status](https://coveralls.io/repos/github/reddevilmidzy/queensac/badge.svg?branch=main)](https://coveralls.io/github/reddevilmidzy/queensac?branch=main)
[![Rust](https://github.com/reddevilmidzy/queensac/actions/workflows/rust.yml/badge.svg)](https://github.com/reddevilmidzy/queensac/actions/workflows/rust.yml)

> 자동화된 링크 검증 및 수정 서비스

queensac은 GitHub 저장소 내의 깨진 링크를 자동으로 감지하고, 가능한 경우 올바른 링크로 수정하여 Pull Request를 생성하는 서비스입니다. 이로써 오픈소스 프로젝트의 문서 품질을 향상시키고, 사용자에게 더 나은 경험을 제공합니다.


## 주요 기능

* 링크 추출: 저장소 내의 파일에서 모든 링크를 추출합니다.
* 링크 검증: 추출된 링크의 유효성을 검사합니다.
* 대체 링크 탐색: 깨진 링크에 대한 대체 가능한 올바른 링크를 검색합니다.
* 자동 수정 및 PR 생성: 깨진 링크를 수정하고, 변경 사항을 반영한 Pull Request를 자동으로 생성합니다.
