# toc 균형형 변환 확장 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 기존 24개 변환의 계약을 보존하면서 문자열·JSON 문자열·UTF-16·zlib·IP 정규화 변환 12개를 CLI와 TUI에 추가한다.

**Architecture:** 기존 `TransformFn`과 단일 정적 레지스트리를 유지하고, 각 변환을 책임별 내부 모듈에 구현한 뒤 마지막 통합 작업에서 새 ID 12개를 한 번에 공개한다. 표준 라이브러리와 이미 설치된 `serde_json`, `flate2`만 사용하며, zlib 해제는 저수준 스트림 상태와 소비 바이트 수를 직접 확인해 절단·후행 데이터를 엄격히 거부한다. CLI와 TUI는 기존처럼 같은 레지스트리를 사용하므로 별도 명령 계층이나 옵션·Recipe 추상화는 만들지 않는다.

**Tech Stack:** Rust 1.97.1, Rust 2024 Edition, `serde_json 1.0.151`, `flate2 1.1.9`의 `rust_backend`, 기존 Rust 단위·CLI·TUI·Shell Smoke 시험

## Global Constraints

- 기존 24개 공개 변환 ID, 메타데이터, 표시 순서를 그대로 유지하고 새 12개를 25번부터 36번까지 뒤에만 추가한다.
- `TransformFn = fn(&[u8], usize) -> Result<Vec<u8>, TransformError>`와 `TransformDefinition`은 변경하지 않는다.
- `trim`, `lowercase`, `uppercase`, JSON 문자열 인코더, UTF-16 인코더, `normalize-ip`는 유효한 UTF-8만 받는다.
- UTF-16 인코더는 BOM을 추가하지 않고, 디코더는 BOM을 제거하거나 엔디언을 자동 감지하지 않는다. UTF-16 오류 위치는 0부터 시작하는 바이트 오프셋이다.
- zlib 압축은 RFC 1950, 수준 6, 사전 없음으로 고정한다. 해제는 정확히 한 스트림만 받고 헤더·Adler-32·절단·사전 사용·후행 데이터를 검증한다.
- `normalize-ip`는 공백 없는 단일 IPv4 또는 IPv6 주소만 받고 CIDR·포트·대괄호·영역 식별자를 거부한다.
- 출력 초과는 기존 `OutputTooLarge`, UTF-8 입력 오류는 기존 `InvalidUtf8Input`을 사용하며 오류와 Trace에 원본 입력을 포함하지 않는다.
- CLI 입력 64 MiB·단계 출력 256 MiB, TUI 입력 1 MiB·단계 출력 64 MiB, Pipeline 최대 32단계 제한과 터미널 출력 안전 경계를 유지한다.
- 새 의존성, 단계별 옵션, Pipeline 저장·불러오기, 범용 Recipe 엔진, 자동 형식 감지, raw DEFLATE를 추가하지 않는다.
- 키 바인딩과 TUI 구조는 변경하지 않는다. 공개 등록과 같은 커밋에서 README를 `36 transforms`와 실제 변환 계약에 맞춘다.
- `ARCHITECTURE.md`, `DESIGN.md`, `docs/design/`은 현재 없으므로 새로 만들지 않는다.
- 기존의 추적되지 않은 `docs/superpowers/` 역사 문서는 수정하거나 스테이징하지 않고, 이 계획 파일과 승인 명세만 정확한 경로로 다룬다.

---

### Task 1: Unicode 문자열 기본 변환

**Files:**
- Create: `src/transforms/text.rs`
- Modify: `src/transforms/mod.rs:1-13`
- Test: `src/transforms/text.rs`

**Interfaces:**
- Consumes: `crate::error::TransformError`, 기존 `TransformFn`의 `(&[u8], usize)` 입력·출력 제한 계약
- Produces: `text::trim`, `text::lowercase`, `text::uppercase`, 모두 `pub(super) fn(&[u8], usize) -> Result<Vec<u8>, TransformError>`

- [ ] **Step 1: 새 모듈 선언과 실패 단위시험을 작성한다**

`src/transforms/mod.rs`의 모듈 목록에 `mod text;`를 추가한다. `src/transforms/text.rs`에는 먼저 다음 시험만 작성한다.

```rust
use crate::error::TransformError;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transforms::TransformFn;

    #[test]
    fn trims_only_unicode_whitespace_at_both_ends() {
        assert_eq!(
            trim("\u{2003}a \n b\u{3000}".as_bytes(), 5).unwrap(),
            b"a \n b"
        );
        assert_eq!(trim("\u{2003}\n".as_bytes(), 0).unwrap(), b"");
        assert_eq!(
            trim(b" x ", 0).unwrap_err(),
            TransformError::OutputTooLarge { limit: 0 }
        );
    }

    #[test]
    fn unicode_case_mapping_expands_only_within_the_limit() {
        assert_eq!(lowercase("İ".as_bytes(), 3).unwrap(), "i\u{307}".as_bytes());
        assert_eq!(uppercase("ß".as_bytes(), 2).unwrap(), b"SS");
        assert_eq!(
            lowercase("İ".as_bytes(), 2).unwrap_err(),
            TransformError::OutputTooLarge { limit: 2 }
        );
        assert_eq!(
            uppercase("ß".as_bytes(), 1).unwrap_err(),
            TransformError::OutputTooLarge { limit: 1 }
        );
    }

    #[test]
    fn text_transforms_reject_invalid_utf8() {
        let transforms: [TransformFn; 3] = [trim, lowercase, uppercase];
        for transform in transforms {
            assert_eq!(transform(b"", 0).unwrap(), b"");
            assert_eq!(
                transform(&[0xff], 8).unwrap_err(),
                TransformError::InvalidUtf8Input
            );
        }
    }
}
```

- [ ] **Step 2: 단위시험이 정의되지 않은 함수 때문에 실패하는지 확인한다**

Run:

```bash
cargo test --locked --lib --color never transforms::text::tests
```

Expected: FAIL with `cannot find function trim`, `lowercase`, or `uppercase`.

- [ ] **Step 3: 쓰기 전에 바이트 한도를 검사하는 최소 구현을 추가한다**

시험 모듈 위에 다음 구현을 추가한다.

```rust
fn utf8(input: &[u8]) -> Result<&str, TransformError> {
    std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)
}

pub(super) fn trim(
    input: &[u8],
    output_limit: usize,
) -> Result<Vec<u8>, TransformError> {
    let output = utf8(input)?.trim().as_bytes();
    if output.len() > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }
    Ok(output.to_vec())
}

fn map_case<I>(
    input: &[u8],
    output_limit: usize,
    map: impl Fn(char) -> I,
) -> Result<Vec<u8>, TransformError>
where
    I: IntoIterator<Item = char>,
{
    let input = utf8(input)?;
    let mut output = String::with_capacity(input.len().min(output_limit));
    for character in input.chars() {
        for mapped in map(character) {
            let new_len = output
                .len()
                .checked_add(mapped.len_utf8())
                .ok_or(TransformError::OutputTooLarge {
                    limit: output_limit,
                })?;
            if new_len > output_limit {
                return Err(TransformError::OutputTooLarge {
                    limit: output_limit,
                });
            }
            output.push(mapped);
        }
    }
    Ok(output.into_bytes())
}

pub(super) fn lowercase(
    input: &[u8],
    output_limit: usize,
) -> Result<Vec<u8>, TransformError> {
    map_case(input, output_limit, char::to_lowercase)
}

pub(super) fn uppercase(
    input: &[u8],
    output_limit: usize,
) -> Result<Vec<u8>, TransformError> {
    map_case(input, output_limit, char::to_uppercase)
}
```

- [ ] **Step 4: 문자열 단위시험과 라이브러리 회귀시험을 실행한다**

Run:

```bash
cargo test --locked --lib --color never transforms::text::tests
cargo test --locked --lib --color never
cargo fmt --all -- --check
cargo clippy --locked --lib -- -D warnings
git diff --check
```

Expected: 모든 명령이 종료 코드 0.

- [ ] **Step 5: 문자열 내부 구현을 커밋한다**

```bash
git add src/transforms/text.rs src/transforms/mod.rs
git diff --cached --check
git commit -m "feat(text): 문자열 변환 추가"
```

### Task 2: JSON 문자열 인코드·디코드

**Files:**
- Modify: `src/error.rs:2-36,85-90,188-195,336-359`
- Modify: `src/tui/views.rs:401-445,465-505`
- Modify: `src/transforms/json.rs:1,154-195,316-455`
- Test: `src/error.rs`
- Test: `src/tui/views.rs`
- Test: `src/transforms/json.rs`

**Interfaces:**
- Consumes: 기존 `json::validate(&[u8])`, `json::LimitedOutput`, `JsonErrorKind`, `TransformError::InvalidJson`
- Produces: `JsonErrorKind::ExpectedString`; `json::string_encode`와 `json::string_decode`, 모두 `pub(super) fn(&[u8], usize) -> Result<Vec<u8>, TransformError>`

- [ ] **Step 1: JSON 문자열과 안전한 오류 문구의 실패시험을 작성한다**

`src/transforms/json.rs` 시험 모듈에 다음 시험을 추가한다.

```rust
#[test]
fn encodes_a_complete_json_string_literal() {
    let input = "\"\\\0\n한";
    assert_eq!(
        string_encode(input.as_bytes(), 64).unwrap(),
        r#""\"\\\u0000\n한""#.as_bytes()
    );
    assert_eq!(
        string_encode(b"\"", 4).unwrap(),
        br#""\"""#
    );
}

#[test]
fn decodes_one_string_with_json_whitespace_and_surrogate_pairs() {
    assert_eq!(
        string_decode(" \t\"\\uD83D\\uDE00 한\"\r\n".as_bytes(), 8).unwrap(),
        "😀 한".as_bytes()
    );
}

#[test]
fn rejects_non_strings_bom_trailing_data_and_invalid_strings() {
    for input in [
        b"{}".as_slice(),
        b"[]",
        b"0",
        b"true",
        b"null",
    ] {
        assert!(matches!(
            string_decode(input, 8),
            Err(TransformError::InvalidJson {
                kind: JsonErrorKind::ExpectedString,
                ..
            })
        ));
    }

    for input in [
        br#""x" 0"#.as_slice(),
        br#""\q""#.as_slice(),
        br#""\uD800""#.as_slice(),
    ] {
        assert!(matches!(
            string_decode(input, 8),
            Err(TransformError::InvalidJson {
                kind: JsonErrorKind::Syntax,
                ..
            })
        ));
    }

    assert!(matches!(
        string_decode(b"\xef\xbb\xbf\"x\"", 8),
        Err(TransformError::InvalidJson {
            kind: JsonErrorKind::Bom,
            ..
        })
    ));
}

#[test]
fn json_string_transforms_enforce_utf8_and_byte_limits() {
    assert_eq!(
        string_encode(&[0xff], 8).unwrap_err(),
        TransformError::InvalidUtf8Input
    );
    assert_eq!(
        string_decode(&[0xff], 8).unwrap_err(),
        TransformError::InvalidUtf8Input
    );
    assert_eq!(
        string_encode(b"\"", 3).unwrap_err(),
        TransformError::OutputTooLarge { limit: 3 }
    );
    assert_eq!(
        string_decode("\"é\"".as_bytes(), 1).unwrap_err(),
        TransformError::OutputTooLarge { limit: 1 }
    );
}
```

`src/error.rs` 시험 모듈에 공개 Pipeline 오류 문구를 추가한다.

```rust
#[test]
fn renders_expected_json_string_errors_without_input_content() {
    assert_eq!(
        render_pipeline_error(&PipelineError::Step {
            step: 2,
            transform_id: "json-string-decode",
            source: TransformError::InvalidJson {
                line: 1,
                column: 3,
                kind: JsonErrorKind::ExpectedString,
            },
        }),
        "step 2 (json-string-decode) failed: expected a JSON string at line 1, column 3"
    );
}
```

`src/tui/views.rs` 시험 모듈에도 Trace 요약 문구를 고정한다.

```rust
#[test]
fn summarizes_expected_json_string_errors_without_input_content() {
    assert_eq!(
        render_transform_error_summary(&crate::error::TransformError::InvalidJson {
            line: 1,
            column: 3,
            kind: crate::error::JsonErrorKind::ExpectedString,
        }),
        "expected a JSON string at line 1, column 3"
    );
}
```

- [ ] **Step 2: 새 함수와 오류 종류가 없어 시험이 실패하는지 확인한다**

Run:

```bash
cargo test --locked --lib --color never transforms::json::tests
cargo test --locked --lib --color never error::tests
cargo test --locked --lib --color never tui::views::tests
```

Expected: FAIL with missing `string_encode`, `string_decode`, or `JsonErrorKind::ExpectedString`.

- [ ] **Step 3: JSON 오류 계약과 CLI·TUI 안전 문구를 구현한다**

`JsonErrorKind`에 새 종류를 추가한다.

```rust
pub enum JsonErrorKind {
    Syntax,
    DuplicateKey,
    Bom,
    DepthExceeded,
    ExpectedString,
}
```

`src/error.rs`의 JSON 오류 match에 다음 분기를 추가한다.

```rust
JsonErrorKind::ExpectedString => "expected a JSON string",
```

`src/tui/views.rs`의 JSON 오류 match에도 같은 분기를 추가한다.

```rust
JsonErrorKind::ExpectedString => "expected a JSON string",
```

- [ ] **Step 4: 기존 제한 작성기를 재사용해 JSON 문자열 변환을 구현한다**

`src/transforms/json.rs`의 표준 라이브러리 import에 `io`를 추가한다.

```rust
use std::{collections::HashSet, fmt, io};
```

기존 `impl LimitedOutput` 바로 뒤에 `io::Write` 구현을 추가한다.

```rust
impl io::Write for LimitedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.extend(bytes)
            .map(|()| bytes.len())
            .map_err(|_| io::Error::other("output limit"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
```

기존 `minify` 아래, 시험 모듈 위에 다음 두 함수를 추가한다.

```rust
pub(super) fn string_encode(
    input: &[u8],
    output_limit: usize,
) -> Result<Vec<u8>, TransformError> {
    let text = std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;
    let mut output = LimitedOutput::new(output_limit);
    serde_json::to_writer(&mut output, text).map_err(|_| TransformError::OutputTooLarge {
        limit: output_limit,
    })?;
    Ok(output.bytes)
}

pub(super) fn string_decode(
    input: &[u8],
    output_limit: usize,
) -> Result<Vec<u8>, TransformError> {
    std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;
    validate(input)?;
    let output = serde_json::from_slice::<String>(input).map_err(|error| {
        TransformError::InvalidJson {
            line: error.line(),
            column: error.column(),
            kind: JsonErrorKind::ExpectedString,
        }
    })?;
    if output.len() > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }
    Ok(output.into_bytes())
}
```

`validate(input)?`를 먼저 호출하므로 BOM·후행 데이터·잘못된 이스케이프·단독 서로게이트는 기존 `Bom` 또는 `Syntax`로 남고, 검증을 통과한 비문자 JSON만 `ExpectedString`이 된다.

- [ ] **Step 5: JSON·오류·라이브러리 시험을 실행한다**

Run:

```bash
cargo test --locked --lib --color never transforms::json::tests
cargo test --locked --lib --color never error::tests
cargo test --locked --lib --color never tui::views::tests
cargo test --locked --lib --color never
cargo fmt --all -- --check
cargo clippy --locked --lib -- -D warnings
git diff --check
```

Expected: 모든 명령이 종료 코드 0.

- [ ] **Step 6: JSON 문자열 내부 구현을 커밋한다**

```bash
git add src/error.rs src/tui/views.rs src/transforms/json.rs
git diff --cached --check
git commit -m "feat(json): JSON 문자열 변환 추가"
```

### Task 3: 명시적 엔디언 UTF-16 변환

**Files:**
- Create: `src/transforms/utf16.rs`
- Modify: `src/transforms/mod.rs:1-13`
- Modify: `src/error.rs:2-36,152-205`
- Modify: `src/tui/views.rs:401-445,465-505`
- Test: `src/transforms/utf16.rs`
- Test: `src/error.rs`
- Test: `src/tui/views.rs`

**Interfaces:**
- Consumes: `TransformError::InvalidUtf8Input`, `TransformError::OutputTooLarge`
- Produces: `TransformError::InvalidUtf16 { position: usize }`; `utf16::{encode_le, decode_le, encode_be, decode_be}`, 모두 `pub(super) fn(&[u8], usize) -> Result<Vec<u8>, TransformError>`

- [ ] **Step 1: 엔디언·서로게이트·BOM·한도 실패시험을 작성한다**

`src/transforms/mod.rs`에 `mod utf16;`을 추가하고, `src/transforms/utf16.rs`에 다음 시험을 먼저 작성한다.

```rust
use crate::error::TransformError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_decodes_both_endians_without_a_bom() {
        let text = "A한😀";
        let le = [0x41, 0x00, 0x5c, 0xd5, 0x3d, 0xd8, 0x00, 0xde];
        let be = [0x00, 0x41, 0xd5, 0x5c, 0xd8, 0x3d, 0xde, 0x00];

        assert_eq!(encode_le(text.as_bytes(), 8).unwrap(), le);
        assert_eq!(encode_be(text.as_bytes(), 8).unwrap(), be);
        assert_eq!(decode_le(&le, text.len()).unwrap(), text.as_bytes());
        assert_eq!(decode_be(&be, text.len()).unwrap(), text.as_bytes());
        assert_eq!(encode_le(b"", 0).unwrap(), b"");
        assert_eq!(decode_be(b"", 0).unwrap(), b"");
    }

    #[test]
    fn reports_exact_byte_offsets_for_invalid_utf16() {
        assert_eq!(
            decode_le(&[0x41], 8).unwrap_err(),
            TransformError::InvalidUtf16 { position: 0 }
        );
        assert_eq!(
            decode_le(&[0x00, 0xd8, 0x41, 0x00], 8).unwrap_err(),
            TransformError::InvalidUtf16 { position: 0 }
        );
        assert_eq!(
            decode_le(&[0x41, 0x00, 0x00, 0xd8], 8).unwrap_err(),
            TransformError::InvalidUtf16 { position: 2 }
        );
        assert_eq!(
            decode_le(&[0x3d, 0xd8, 0x00, 0xde, 0x00, 0xdc], 8).unwrap_err(),
            TransformError::InvalidUtf16 { position: 4 }
        );
    }

    #[test]
    fn preserves_bom_code_units_as_characters() {
        assert_eq!(
            decode_le(&[0xff, 0xfe, 0x41, 0x00], 4).unwrap(),
            "\u{feff}A".as_bytes()
        );
        assert_eq!(
            decode_be(&[0xfe, 0xff], 3).unwrap(),
            "\u{feff}".as_bytes()
        );
    }

    #[test]
    fn utf16_transforms_enforce_utf8_and_output_limits() {
        assert_eq!(
            encode_le(&[0xff], 8).unwrap_err(),
            TransformError::InvalidUtf8Input
        );
        assert_eq!(
            encode_be("😀".as_bytes(), 3).unwrap_err(),
            TransformError::OutputTooLarge { limit: 3 }
        );
        assert_eq!(
            decode_le(&[0x5c, 0xd5], 2).unwrap_err(),
            TransformError::OutputTooLarge { limit: 2 }
        );
        assert_eq!(decode_le(&[0x5c, 0xd5], 3).unwrap(), "한".as_bytes());
    }
}
```

`src/error.rs`와 `src/tui/views.rs` 시험 모듈에 고정 문구 시험을 각각 추가한다.

```rust
#[test]
fn renders_utf16_errors_without_input_content() {
    assert_eq!(
        render_pipeline_error(&PipelineError::Step {
            step: 1,
            transform_id: "utf16le-decode",
            source: TransformError::InvalidUtf16 { position: 4 },
        }),
        "step 1 (utf16le-decode) failed: invalid UTF-16 at byte 4"
    );
}
```

```rust
#[test]
fn summarizes_utf16_errors_without_input_content() {
    assert_eq!(
        render_transform_error_summary(&crate::error::TransformError::InvalidUtf16 {
            position: 4,
        }),
        "invalid UTF-16 at byte 4"
    );
}
```

- [ ] **Step 2: 새 모듈 함수와 오류 variant가 없어 시험이 실패하는지 확인한다**

Run:

```bash
cargo test --locked --lib --color never transforms::utf16::tests
cargo test --locked --lib --color never error::tests
cargo test --locked --lib --color never tui::views::tests
```

Expected: FAIL with missing UTF-16 함수 또는 `TransformError::InvalidUtf16`.

- [ ] **Step 3: UTF-16 오류와 안전한 CLI·TUI 문구를 추가한다**

`TransformError`에 다음 variant를 추가한다.

```rust
InvalidUtf16 {
    position: usize,
},
```

`src/error.rs`와 `src/tui/views.rs`의 `TransformError` match에 각각 다음 분기를 추가한다.

```rust
TransformError::InvalidUtf16 { position } => {
    format!("invalid UTF-16 at byte {position}")
}
```

- [ ] **Step 4: 바이트 위치를 직접 추적하는 UTF-16 변환을 구현한다**

`src/transforms/utf16.rs`의 시험 모듈 위에 다음 구현을 추가한다.

```rust
fn encode(
    input: &[u8],
    output_limit: usize,
    byte_order: fn(u16) -> [u8; 2],
) -> Result<Vec<u8>, TransformError> {
    let text = std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;
    let output_len = text
        .encode_utf16()
        .count()
        .checked_mul(2)
        .ok_or(TransformError::OutputTooLarge {
            limit: output_limit,
        })?;
    if output_len > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }

    let mut output = Vec::with_capacity(output_len);
    for code_unit in text.encode_utf16() {
        output.extend_from_slice(&byte_order(code_unit));
    }
    Ok(output)
}

fn decode(
    input: &[u8],
    output_limit: usize,
    byte_order: fn([u8; 2]) -> u16,
) -> Result<Vec<u8>, TransformError> {
    if input.len() % 2 != 0 {
        return Err(TransformError::InvalidUtf16 {
            position: input.len() - 1,
        });
    }

    let mut output = String::with_capacity(input.len().min(output_limit));
    let mut position = 0;
    while position < input.len() {
        let first = byte_order([input[position], input[position + 1]]);
        let scalar = match first {
            0xd800..=0xdbff => {
                if position + 3 >= input.len() {
                    return Err(TransformError::InvalidUtf16 { position });
                }
                let second = byte_order([input[position + 2], input[position + 3]]);
                if !(0xdc00..=0xdfff).contains(&second) {
                    return Err(TransformError::InvalidUtf16 { position });
                }
                position += 4;
                0x10000
                    + (((u32::from(first) - 0xd800) << 10)
                        | (u32::from(second) - 0xdc00))
            }
            0xdc00..=0xdfff => {
                return Err(TransformError::InvalidUtf16 { position });
            }
            _ => {
                position += 2;
                u32::from(first)
            }
        };

        let character = char::from_u32(scalar).expect("validated UTF-16 scalar");
        let new_len = output
            .len()
            .checked_add(character.len_utf8())
            .ok_or(TransformError::OutputTooLarge {
                limit: output_limit,
            })?;
        if new_len > output_limit {
            return Err(TransformError::OutputTooLarge {
                limit: output_limit,
            });
        }
        output.push(character);
    }
    Ok(output.into_bytes())
}

pub(super) fn encode_le(
    input: &[u8],
    output_limit: usize,
) -> Result<Vec<u8>, TransformError> {
    encode(input, output_limit, u16::to_le_bytes)
}

pub(super) fn decode_le(
    input: &[u8],
    output_limit: usize,
) -> Result<Vec<u8>, TransformError> {
    decode(input, output_limit, u16::from_le_bytes)
}

pub(super) fn encode_be(
    input: &[u8],
    output_limit: usize,
) -> Result<Vec<u8>, TransformError> {
    encode(input, output_limit, u16::to_be_bytes)
}

pub(super) fn decode_be(
    input: &[u8],
    output_limit: usize,
) -> Result<Vec<u8>, TransformError> {
    decode(input, output_limit, u16::from_be_bytes)
}
```

`char::decode_utf16(...).enumerate()`는 정상 서로게이트 쌍이 코드 단위 두 개를 출력 항목 하나로 줄여 이후 오류의 바이트 위치가 어긋나므로 사용하지 않는다.

- [ ] **Step 5: UTF-16·오류·라이브러리 시험을 실행한다**

Run:

```bash
cargo test --locked --lib --color never transforms::utf16::tests
cargo test --locked --lib --color never error::tests
cargo test --locked --lib --color never tui::views::tests
cargo test --locked --lib --color never
cargo fmt --all -- --check
cargo clippy --locked --lib -- -D warnings
git diff --check
```

Expected: 모든 명령이 종료 코드 0.

- [ ] **Step 6: UTF-16 내부 구현을 커밋한다**

```bash
git add src/error.rs src/tui/views.rs src/transforms/mod.rs src/transforms/utf16.rs
git diff --cached --check
git commit -m "feat(utf16): 엔디언별 UTF-16 변환 추가"
```

### Task 4: Gzip 모듈 일반화와 엄격한 zlib 변환

**Files:**
- Rename: `src/transforms/gzip.rs` → `src/transforms/compression.rs`
- Modify: `src/transforms/mod.rs:1-13,191-208`
- Modify: `src/error.rs:2-36,152-205`
- Modify: `src/tui/views.rs:401-445,465-505`
- Test: `src/transforms/compression.rs`
- Test: `src/error.rs`
- Test: `src/tui/views.rs`

**Interfaces:**
- Consumes: 기존 `compression::LimitedWriter`, 기존 Gzip `compress`·`decompress`, `flate2::{Decompress, FlushDecompress, Status}`
- Produces: 기존 Gzip 공개 동작을 그대로 유지하는 `compression::{compress, decompress}`; 새 `compression::{zlib_compress, zlib_decompress}`; `TransformError::InvalidZlib`

- [ ] **Step 1: 파일·모듈 이름만 일반화하고 Gzip 회귀시험을 실행한다**

`apply_patch`의 move를 사용해 기존 파일 내용을 바꾸지 않고 이동한다.

```text
*** Update File: src/transforms/gzip.rs
*** Move to: src/transforms/compression.rs
```

`src/transforms/mod.rs`의 모듈과 기존 Gzip 함수 참조만 다음처럼 바꾼다.

```rust
mod compression;
```

```rust
apply: compression::compress,
```

```rust
apply: compression::decompress,
```

Run:

```bash
cargo test --locked --lib --color never transforms::compression::tests
```

Expected: 기존 Gzip 시험 6개가 모두 PASS.

- [ ] **Step 2: 결정론·엄격 검증·출력 한도 실패시험을 추가한다**

`src/transforms/compression.rs` 시험 모듈에 다음 상수와 시험을 추가한다.

```rust
const ZLIB_EMPTY: &[u8] = &[0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01];
const ZLIB_HELLO: &[u8] = &[
    0x78, 0x9c, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00, 0x06, 0x2c, 0x02, 0x15,
];
const ZLIB_FDICT: &[u8] = &[
    0x78, 0xbb, 0x06, 0x2c, 0x02, 0x15, 0xcb, 0x00, 0x11, 0x3a, 0x0a, 0x60, 0x4a, 0x11, 0x00,
    0x21, 0x70, 0x04, 0x96,
];

#[test]
fn zlib_compress_uses_fixed_level_six_vectors_and_limits() {
    assert_eq!(zlib_compress(b"", usize::MAX).unwrap(), ZLIB_EMPTY);
    assert_eq!(zlib_compress(b"hello", usize::MAX).unwrap(), ZLIB_HELLO);
    assert_eq!(
        zlib_compress(b"hello", usize::MAX).unwrap(),
        zlib_compress(b"hello", usize::MAX).unwrap()
    );

    let full = zlib_compress(b"bounded zlib output", usize::MAX).unwrap();
    assert_eq!(
        zlib_compress(b"bounded zlib output", full.len()).unwrap(),
        full
    );
    assert_eq!(
        zlib_compress(b"bounded zlib output", full.len() - 1).unwrap_err(),
        TransformError::OutputTooLarge {
            limit: full.len() - 1,
        }
    );
}

#[test]
fn zlib_decompress_accepts_one_complete_stream() {
    assert_eq!(zlib_decompress(ZLIB_EMPTY, 0).unwrap(), b"");
    assert_eq!(zlib_decompress(ZLIB_HELLO, 5).unwrap(), b"hello");
}

#[test]
fn zlib_decompress_rejects_header_fdict_checksum_truncation_and_trailing_data() {
    let mut invalid_header = ZLIB_HELLO.to_vec();
    invalid_header[1] ^= 1;
    let mut invalid_checksum = ZLIB_HELLO.to_vec();
    *invalid_checksum.last_mut().unwrap() ^= 1;

    for input in [
        invalid_header,
        ZLIB_FDICT.to_vec(),
        invalid_checksum,
        ZLIB_HELLO[..ZLIB_HELLO.len() - 1].to_vec(),
        Vec::new(),
        [ZLIB_HELLO, b"junk"].concat(),
        [ZLIB_HELLO, ZLIB_EMPTY].concat(),
    ] {
        assert_eq!(
            zlib_decompress(&input, 1024).unwrap_err(),
            TransformError::InvalidZlib
        );
    }
}

#[test]
fn zlib_decompress_checks_one_extra_byte_across_the_buffer_boundary() {
    let payload = vec![b'x'; 8193];
    let compressed = zlib_compress(&payload, usize::MAX).unwrap();
    assert_eq!(zlib_decompress(&compressed, 8193).unwrap(), payload);
    assert_eq!(
        zlib_decompress(&compressed, 8192).unwrap_err(),
        TransformError::OutputTooLarge { limit: 8192 }
    );
}
```

`src/error.rs`와 `src/tui/views.rs` 시험 모듈에도 고정 문구를 추가한다.

```rust
#[test]
fn renders_zlib_errors_without_input_content() {
    assert_eq!(
        render_pipeline_error(&PipelineError::Step {
            step: 3,
            transform_id: "zlib-decompress",
            source: TransformError::InvalidZlib,
        }),
        "step 3 (zlib-decompress) failed: invalid zlib data"
    );
}
```

```rust
#[test]
fn summarizes_zlib_errors_without_input_content() {
    assert_eq!(
        render_transform_error_summary(&crate::error::TransformError::InvalidZlib),
        "invalid zlib data"
    );
}
```

- [ ] **Step 3: 새 zlib 함수와 오류 variant가 없어 시험이 실패하는지 확인한다**

Run:

```bash
cargo test --locked --lib --color never transforms::compression::tests
cargo test --locked --lib --color never error::tests
cargo test --locked --lib --color never tui::views::tests
```

Expected: FAIL with missing `zlib_compress`, `zlib_decompress`, or `TransformError::InvalidZlib`.

- [ ] **Step 4: zlib 오류와 안전한 CLI·TUI 문구를 추가한다**

`TransformError`에 다음 variant를 추가한다.

```rust
InvalidZlib,
```

`src/error.rs`와 `src/tui/views.rs`의 `TransformError` match에 각각 다음 분기를 추가한다.

```rust
TransformError::InvalidZlib => "invalid zlib data".to_string(),
```

- [ ] **Step 5: 기존 제한 작성기와 엄격한 스트림 상태 검사를 사용해 zlib을 구현한다**

`src/transforms/compression.rs`의 import를 다음처럼 확장한다.

```rust
use flate2::{
    Compression, Decompress, FlushDecompress, GzBuilder, Status,
    read::MultiGzDecoder,
    write::ZlibEncoder,
};
```

기존 Gzip 함수 아래에 다음 두 함수를 추가한다.

```rust
pub(super) fn zlib_compress(
    input: &[u8],
    output_limit: usize,
) -> Result<Vec<u8>, TransformError> {
    let writer = LimitedWriter::new(output_limit);
    let mut encoder = ZlibEncoder::new(writer, Compression::new(6));
    encoder
        .write_all(input)
        .map_err(|_| TransformError::OutputTooLarge {
            limit: output_limit,
        })?;
    encoder
        .finish()
        .map(|writer| writer.bytes)
        .map_err(|_| TransformError::OutputTooLarge {
            limit: output_limit,
        })
}

pub(super) fn zlib_decompress(
    input: &[u8],
    output_limit: usize,
) -> Result<Vec<u8>, TransformError> {
    let mut decoder = Decompress::new(true);
    let mut output = Vec::with_capacity(output_limit.min(8192));
    let mut buffer = [0; 8192];

    loop {
        let remaining = output_limit - output.len();
        let read_limit = remaining.saturating_add(1).min(buffer.len());
        let input_offset = decoder.total_in() as usize;
        let before_output = decoder.total_out();
        let flush = if input_offset == input.len() {
            FlushDecompress::Finish
        } else {
            FlushDecompress::None
        };
        let status = decoder
            .decompress(
                &input[input_offset..],
                &mut buffer[..read_limit],
                flush,
            )
            .map_err(|_| TransformError::InvalidZlib)?;
        let consumed = decoder.total_in() as usize - input_offset;
        let produced = (decoder.total_out() - before_output) as usize;

        if produced > remaining {
            return Err(TransformError::OutputTooLarge {
                limit: output_limit,
            });
        }
        output.extend_from_slice(&buffer[..produced]);

        if status == Status::StreamEnd {
            return if decoder.total_in() as usize == input.len() {
                Ok(output)
            } else {
                Err(TransformError::InvalidZlib)
            };
        }
        if consumed == 0 && produced == 0 {
            return Err(TransformError::InvalidZlib);
        }
    }
}
```

고수준 `bufread::ZlibDecoder`는 현재 잠긴 `flate2 1.1.9`에서 Adler-32의 마지막 바이트가 잘린 입력을 성공으로 처리할 수 있으므로 사용하지 않는다. `FlushDecompress::Finish`는 입력을 모두 소비한 뒤에만 사용하고, `Status::StreamEnd`와 `total_in() == input.len()`을 함께 요구한다.

- [ ] **Step 6: Gzip·zlib·오류·라이브러리 시험을 실행한다**

Run:

```bash
cargo test --locked --lib --color never transforms::compression::tests
cargo test --locked --lib --color never error::tests
cargo test --locked --lib --color never tui::views::tests
cargo test --locked --lib --color never
cargo fmt --all -- --check
cargo clippy --locked --lib -- -D warnings
git diff --check
```

Expected: 기존 Gzip 시험 6개와 새 zlib 시험을 포함해 모든 명령이 종료 코드 0.

- [ ] **Step 7: 압축 내부 구현을 커밋한다**

```bash
git add src/error.rs src/tui/views.rs src/transforms/mod.rs src/transforms/compression.rs
git add -u src/transforms/gzip.rs
git diff --cached --check
git commit -m "feat(compression): 엄격한 zlib 변환 추가"
```

### Task 5: IP 주소 정규화

**Files:**
- Create: `src/transforms/ip.rs`
- Modify: `src/transforms/mod.rs:1-13`
- Modify: `src/error.rs:2-36,152-205`
- Modify: `src/tui/views.rs:401-445,465-505`
- Test: `src/transforms/ip.rs`
- Test: `src/error.rs`
- Test: `src/tui/views.rs`

**Interfaces:**
- Consumes: `std::net::IpAddr::{from_str, to_string}`, `TransformError::InvalidUtf8Input`, `TransformError::OutputTooLarge`
- Produces: `TransformError::InvalidIpAddress`; `ip::normalize(&[u8], usize) -> Result<Vec<u8>, TransformError>`

- [ ] **Step 1: RFC 5952 출력·거부 경계·한도 실패시험을 작성한다**

`src/transforms/mod.rs`에 `mod ip;`를 추가하고, `src/transforms/ip.rs`에 다음 시험을 먼저 작성한다.

```rust
use crate::error::TransformError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_ipv4_and_rfc5952_ipv6() {
        assert_eq!(normalize(b"192.0.2.1", 16).unwrap(), b"192.0.2.1");
        assert_eq!(
            normalize(b"2001:0DB8:0000:0000:0000:ff00:0042:8329", 64).unwrap(),
            b"2001:db8::ff00:42:8329"
        );
        assert_eq!(
            normalize(b"2001:0:0:1:0:0:1:1", 64).unwrap(),
            b"2001::1:0:0:1:1"
        );
        assert_eq!(
            normalize(b"2001:db8:0:1:1:1:1:1", 64).unwrap(),
            b"2001:db8:0:1:1:1:1:1"
        );
        assert_eq!(
            normalize(b"::ffff:192.0.2.128", 64).unwrap(),
            b"::ffff:192.0.2.128"
        );
    }

    #[test]
    fn rejects_every_non_address_wrapper() {
        for input in [
            b" 127.0.0.1".as_slice(),
            b"127.0.0.1 ",
            b"127.0.0.1/24",
            b"127.0.0.1:80",
            b"[::1]",
            b"fe80::1%en0",
            b"01.2.3.4",
        ] {
            assert_eq!(
                normalize(input, 64).unwrap_err(),
                TransformError::InvalidIpAddress
            );
        }
    }

    #[test]
    fn normalize_ip_maps_utf8_and_output_errors() {
        assert_eq!(
            normalize(&[0xff], 8).unwrap_err(),
            TransformError::InvalidUtf8Input
        );
        assert_eq!(
            normalize(b"::1", 2).unwrap_err(),
            TransformError::OutputTooLarge { limit: 2 }
        );
        assert_eq!(normalize(b"::1", 3).unwrap(), b"::1");
    }
}
```

`src/error.rs`와 `src/tui/views.rs` 시험 모듈에도 원문 비노출 문구를 추가한다.

```rust
#[test]
fn renders_ip_errors_without_input_content() {
    assert_eq!(
        render_pipeline_error(&PipelineError::Step {
            step: 2,
            transform_id: "normalize-ip",
            source: TransformError::InvalidIpAddress,
        }),
        "step 2 (normalize-ip) failed: invalid IP address"
    );
}
```

```rust
#[test]
fn summarizes_ip_errors_without_input_content() {
    assert_eq!(
        render_transform_error_summary(&crate::error::TransformError::InvalidIpAddress),
        "invalid IP address"
    );
}
```

- [ ] **Step 2: 새 정규화 함수와 오류 variant가 없어 시험이 실패하는지 확인한다**

Run:

```bash
cargo test --locked --lib --color never transforms::ip::tests
cargo test --locked --lib --color never error::tests
cargo test --locked --lib --color never tui::views::tests
```

Expected: FAIL with missing `normalize` 또는 `TransformError::InvalidIpAddress`.

- [ ] **Step 3: IP 오류와 안전한 CLI·TUI 문구를 추가한다**

`TransformError`에 다음 variant를 추가한다.

```rust
InvalidIpAddress,
```

`src/error.rs`와 `src/tui/views.rs`의 `TransformError` match에 각각 다음 분기를 추가한다.

```rust
TransformError::InvalidIpAddress => "invalid IP address".to_string(),
```

- [ ] **Step 4: 표준 라이브러리 파서와 포매터로 최소 구현한다**

`src/transforms/ip.rs`의 시험 모듈 위에 다음 함수를 추가한다.

```rust
pub(super) fn normalize(
    input: &[u8],
    output_limit: usize,
) -> Result<Vec<u8>, TransformError> {
    let text = std::str::from_utf8(input).map_err(|_| TransformError::InvalidUtf8Input)?;
    let output = text
        .parse::<std::net::IpAddr>()
        .map_err(|_| TransformError::InvalidIpAddress)?
        .to_string();
    if output.len() > output_limit {
        return Err(TransformError::OutputTooLarge {
            limit: output_limit,
        });
    }
    Ok(output.into_bytes())
}
```

Rust 1.97.1의 `IpAddr` 파서는 입력 전체와 IPv4 선행 0을 검증하고, `Ipv6Addr::Display`는 가장 긴 왼쪽 0 구간만 축약하며 IPv4-mapped IPv6를 점 십진수로 출력한다. 별도 IP 파서나 포매터는 만들지 않는다.

- [ ] **Step 5: IP·오류·라이브러리 시험을 실행한다**

Run:

```bash
cargo test --locked --lib --color never transforms::ip::tests
cargo test --locked --lib --color never error::tests
cargo test --locked --lib --color never tui::views::tests
cargo test --locked --lib --color never
cargo fmt --all -- --check
cargo clippy --locked --lib -- -D warnings
git diff --check
```

Expected: 모든 명령이 종료 코드 0.

- [ ] **Step 6: IP 내부 구현을 커밋한다**

```bash
git add src/error.rs src/tui/views.rs src/transforms/ip.rs src/transforms/mod.rs
git diff --cached --check
git commit -m "feat(ip): IP 주소 정규화 추가"
```

### Task 6: 36개 공개 레지스트리·CLI·TUI·README 통합

**Files:**
- Modify: `src/transforms/mod.rs:29-303`
- Modify: `tests/cli.rs:18-43,64-129,276-300`
- Modify: `src/tui/state.rs:1845-1872`
- Modify: `src/tui/render.rs:1697-1714,2345-2367`
- Modify: `README.md:1-123`
- Test: `src/transforms/mod.rs`
- Test: `tests/cli.rs`
- Test: `src/tui/state.rs`
- Test: `src/tui/render.rs`

**Interfaces:**
- Consumes: Tasks 1–5의 내부 함수와 오류 계약, 기존 `transforms()`, `transform_by_id()`, CLI 동적 명령 생성, TUI Picker 동적 검색·상세 렌더링
- Produces: 기존 24개 뒤에 정확한 순서로 추가된 12개 공개 ID, 총 36개 CLI·TUI 변환, 실제 레지스트리와 일치하는 README

- [ ] **Step 1: 레지스트리·CLI·TUI의 36개 공개 계약 실패시험을 작성한다**

`src/transforms/mod.rs`의 `registry_has_the_exact_public_contract_once_in_display_order` 기대 배열 뒤에 다음 항목을 추가한다.

```rust
("trim", "Trim", false),
("lowercase", "Lowercase", false),
("uppercase", "Uppercase", false),
("json-string-encode", "JSON String Encode", false),
("json-string-decode", "JSON String Decode", false),
("utf16le-encode", "UTF-16LE Encode", false),
("utf16le-decode", "UTF-16LE Decode", true),
("utf16be-encode", "UTF-16BE Encode", false),
("utf16be-decode", "UTF-16BE Decode", true),
("zlib-compress", "Zlib Compress", true),
("zlib-decompress", "Zlib Decompress", true),
("normalize-ip", "Normalize IP", false),
```

`tests/cli.rs`의 `TRANSFORM_IDS` 끝에도 다음 ID를 같은 순서로 추가한다.

```rust
"trim",
"lowercase",
"uppercase",
"json-string-encode",
"json-string-decode",
"utf16le-encode",
"utf16le-decode",
"utf16be-encode",
"utf16be-decode",
"zlib-compress",
"zlib-decompress",
"normalize-ip",
```

목록 시험 이름을 다음처럼 바꾼다.

```rust
fn list_exposes_the_exact_thirty_six_public_transform_ids_in_order() {
```

`tests/cli.rs`에 새 변환별 실행·왕복·원자적 실패 시험을 추가한다.

```rust
#[test]
fn balanced_transform_expansion_commands_and_round_trips_execute() {
    const UTF16LE_A_GRIN: &[u8] = &[0x41, 0x00, 0x3d, 0xd8, 0x00, 0xde];
    const UTF16BE_A_GRIN: &[u8] = &[0x00, 0x41, 0xd8, 0x3d, 0xde, 0x00];
    const ZLIB_HELLO: &[u8] = &[
        0x78, 0x9c, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x07, 0x00, 0x06, 0x2c, 0x02, 0x15,
    ];
    let cases: &[(&str, &[u8], &[u8])] = &[
        ("trim", b" \tHello\n", b"Hello"),
        ("lowercase", b"TOC", b"toc"),
        ("uppercase", "Straße".as_bytes(), b"STRASSE"),
        ("json-string-encode", b"toc\n\"x\"", br#""toc\n\"x\"""#),
        ("json-string-decode", br#""toc\n\"x\"""#, b"toc\n\"x\""),
        ("utf16le-encode", "A😀".as_bytes(), UTF16LE_A_GRIN),
        ("utf16le-decode", UTF16LE_A_GRIN, "A😀".as_bytes()),
        ("utf16be-encode", "A😀".as_bytes(), UTF16BE_A_GRIN),
        ("utf16be-decode", UTF16BE_A_GRIN, "A😀".as_bytes()),
        ("zlib-compress", b"hello", ZLIB_HELLO),
        ("zlib-decompress", ZLIB_HELLO, b"hello"),
        (
            "normalize-ip",
            b"2001:0DB8:0:0:0:FF00:0042:8329",
            b"2001:db8::ff00:42:8329",
        ),
    ];

    for (id, input, expected) in cases {
        let output = run(&[id], input);
        assert_eq!(output.status.code(), Some(0), "{id}");
        assert_eq!(output.stdout, *expected, "{id}");
        assert!(output.stderr.is_empty(), "{id}");
    }

    for &(id, _, _) in cases {
        let output = run(&[id, "--help"], b"");
        assert_eq!(output.status.code(), Some(0), "{id} --help");
        assert!(output.stderr.is_empty(), "{id} --help");
        let help = std::str::from_utf8(&output.stdout).unwrap();
        assert!(
            help.contains(&format!("Usage: toc {id} [OPTIONS]")),
            "{id} --help: {help}"
        );
        assert!(help.contains("Behavior:"), "{id} --help: {help}");
    }

    for chain in [
        &["json-string-encode", "--then", "json-string-decode"][..],
        &["utf16le-encode", "--then", "utf16le-decode"][..],
        &["utf16be-encode", "--then", "utf16be-decode"][..],
        &["zlib-compress", "--then", "zlib-decompress"][..],
    ] {
        let output = run(chain, "A😀".as_bytes());
        assert_eq!(output.status.code(), Some(0), "{chain:?}");
        assert_eq!(output.stdout, "A😀".as_bytes(), "{chain:?}");
        assert!(output.stderr.is_empty(), "{chain:?}");
    }

    let output = run(&["trim", "--then", "normalize-ip"], b" not an ip ");
    assert_eq!(output.status.code(), Some(4));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"step 2 (normalize-ip) failed: invalid IP address\n"
    );
}
```

`src/tui/state.rs`의 `picker_search_exposes_new_transforms_from_the_shared_registry`를 총수·UTF-16 검색·IP 추가 계약으로 교체한다.

```rust
#[test]
fn picker_search_exposes_new_transforms_from_the_shared_registry() {
    let start = now();
    let mut app = App::new(start, true);
    app.open_picker();
    assert_eq!(app.filtered_transforms().len(), 36);
    for character in "UTF-16".chars() {
        app.picker_insert(character);
    }
    let ids: Vec<_> = app
        .filtered_transforms()
        .iter()
        .map(|transform| transform.id)
        .collect();
    assert_eq!(
        ids,
        [
            "utf16le-encode",
            "utf16le-decode",
            "utf16be-encode",
            "utf16be-decode",
        ]
    );

    app.open_picker();
    for character in "Normalize IP".chars() {
        app.picker_insert(character);
    }
    app.confirm_picker(start);
    assert_eq!(app.steps[0].definition.id, "normalize-ip");
}
```

`src/tui/render.rs`의 `picker_click_selects_then_explicit_add_and_cancel_regions_act`에서 다음 중복 개수 단정을 삭제한다. 바로 아래 행 범위 단정이 계산된 `last`를 이미 검증한다.

```rust
assert_eq!(last, 23);
```

`compact_add_transform_keeps_a_separate_selected_description`는 새 JSON 문자열 변환을 검색해 좁은 화면을 검증하도록 바꾼다.

```rust
#[test]
fn compact_add_transform_keeps_a_new_transform_name_and_description() {
    let start = now();
    let mut app = App::new(start, true);
    app.open_picker();
    for character in "json-string-encode".chars() {
        key(
            &mut app,
            KeyCode::Char(character),
            KeyModifiers::NONE,
            start,
        );
    }

    let screen = rendered_app(40, 10, &mut app);

    assert!(screen.contains("Search:"));
    let mut lines = screen.lines();
    let selected = lines
        .find(|line| line.contains("> JSON String Encode"))
        .unwrap();
    assert!(!selected.contains("[json-string-encode]"));
    assert!(!selected.contains("Encode UTF-8"));
    assert!(!lines.next().unwrap().contains("Encode UTF-8"));
    assert!(screen.contains("Encode UTF-8"));
    assert!(
        screen.contains("JSON String Encode — Encode UTF-8"),
        "missing compact name and description: {screen}"
    );
    assert!(screen.contains("Enter Add"));
    assert!(screen.contains("Esc Cancel"));
    assert!(!screen.contains("Backspace Search"));
}
```

- [ ] **Step 2: 공개 등록 전 통합시험이 현재 24개 계약에서 실패하는지 확인한다**

Run:

```bash
cargo test --locked --lib --color never registry_has_the_exact_public_contract_once_in_display_order
cargo test --locked --test cli --color never balanced_transform_expansion_commands_and_round_trips_execute
cargo test --locked --lib --color never picker_search_exposes_new_transforms_from_the_shared_registry
cargo test --locked --lib --color never compact_add_transform_keeps_a_new_transform_name_and_description
```

Expected: 레지스트리 길이·알 수 없는 CLI 명령·Picker 결과 때문에 모두 FAIL.

- [ ] **Step 3: 기존 24개 뒤에 새 정의 12개를 정확한 순서로 등록한다**

`src/transforms/mod.rs`의 기존 `remove-duplicate-lines` 정의 뒤에 다음 블록을 추가한다.

```rust
TransformDefinition {
    id: "trim",
    display_name: "Trim",
    description: "Trim Unicode whitespace from both ends of UTF-8 text",
    behavior: "removes Unicode whitespace only at both ends and preserves interior text",
    accepts_binary: false,
    apply: text::trim,
},
TransformDefinition {
    id: "lowercase",
    display_name: "Lowercase",
    description: "Convert UTF-8 text with Unicode default lowercase mapping",
    behavior: "uses locale-independent Unicode lowercase mapping without normalization",
    accepts_binary: false,
    apply: text::lowercase,
},
TransformDefinition {
    id: "uppercase",
    display_name: "Uppercase",
    description: "Convert UTF-8 text with Unicode default uppercase mapping",
    behavior: "uses locale-independent Unicode uppercase mapping without normalization",
    accepts_binary: false,
    apply: text::uppercase,
},
TransformDefinition {
    id: "json-string-encode",
    display_name: "JSON String Encode",
    description: "Encode UTF-8 text as one complete JSON string literal",
    behavior: "emits a quoted RFC 8259 string, escapes required characters, and adds no newline",
    accepts_binary: false,
    apply: json::string_encode,
},
TransformDefinition {
    id: "json-string-decode",
    display_name: "JSON String Decode",
    description: "Decode exactly one JSON string literal into UTF-8 text",
    behavior: "allows surrounding JSON whitespace and rejects BOM, non-strings, invalid escapes, and trailing data",
    accepts_binary: false,
    apply: json::string_decode,
},
TransformDefinition {
    id: "utf16le-encode",
    display_name: "UTF-16LE Encode",
    description: "Encode UTF-8 text as little-endian UTF-16",
    behavior: "writes little-endian UTF-16 code units without adding a BOM",
    accepts_binary: false,
    apply: utf16::encode_le,
},
TransformDefinition {
    id: "utf16le-decode",
    display_name: "UTF-16LE Decode",
    description: "Decode little-endian UTF-16 into UTF-8 text",
    behavior: "requires even bytes and valid surrogate pairs and preserves U+FEFF as text",
    accepts_binary: true,
    apply: utf16::decode_le,
},
TransformDefinition {
    id: "utf16be-encode",
    display_name: "UTF-16BE Encode",
    description: "Encode UTF-8 text as big-endian UTF-16",
    behavior: "writes big-endian UTF-16 code units without adding a BOM",
    accepts_binary: false,
    apply: utf16::encode_be,
},
TransformDefinition {
    id: "utf16be-decode",
    display_name: "UTF-16BE Decode",
    description: "Decode big-endian UTF-16 into UTF-8 text",
    behavior: "requires even bytes and valid surrogate pairs and preserves U+FEFF as text",
    accepts_binary: true,
    apply: utf16::decode_be,
},
TransformDefinition {
    id: "zlib-compress",
    display_name: "Zlib Compress",
    description: "Compress bytes as one deterministic zlib stream",
    behavior: "level 6 RFC 1950 with no preset dictionary and deterministic output",
    accepts_binary: true,
    apply: compression::zlib_compress,
},
TransformDefinition {
    id: "zlib-decompress",
    display_name: "Zlib Decompress",
    description: "Decompress and validate exactly one zlib stream",
    behavior: "rejects invalid headers, Adler-32, truncation, preset dictionaries, and trailing data",
    accepts_binary: true,
    apply: compression::zlib_decompress,
},
TransformDefinition {
    id: "normalize-ip",
    display_name: "Normalize IP",
    description: "Normalize one IPv4 or IPv6 address",
    behavior: "requires a bare address and emits canonical dotted decimal or RFC 5952 text",
    accepts_binary: false,
    apply: ip::normalize,
},
```

CLI 명령 생성, `--then`, `--list`, TUI 검색과 상세 렌더링은 모두 `transforms()`를 사용하므로 `src/cli.rs`, `src/main.rs`, TUI 프로덕션 코드는 수정하지 않는다.

- [ ] **Step 4: 레지스트리·CLI·TUI 통합시험을 실행한다**

Run:

```bash
cargo test --locked --lib --color never registry_has_the_exact_public_contract_once_in_display_order
cargo test --locked --test cli --color never balanced_transform_expansion_commands_and_round_trips_execute
cargo test --locked --test cli --color never list_exposes_the_exact_thirty_six_public_transform_ids_in_order
cargo test --locked --test cli --color never root_help_exposes_each_public_transform_command_once
cargo test --locked --lib --color never picker_search_exposes_new_transforms_from_the_shared_registry
cargo test --locked --lib --color never compact_add_transform_keeps_a_new_transform_name_and_description
```

Expected: 모든 명령이 종료 코드 0이며 새 CLI 결과에 임의 줄바꿈이 없음.

- [ ] **Step 5: README를 36개 실제 공개 계약에 맞춘다**

상단 제품 표기를 다음처럼 바꾼다.

```html
<p><code>TUI</code> · <code>CLI</code> · <code>Local-only</code> · <code>36 transforms</code></p>
```

CLI 예제에 새 Pipeline을 추가한다.

```console
# JSON 문자열 Decode → 양끝 공백 제거 → 소문자
$ printf '%s' '"  TOC  "' \
  | toc json-string-decode --then trim --then lowercase
toc
```

지원 변환 표를 다음 내용으로 현행화한다.

```markdown
| 기능군 | 변환 ID |
|---|---|
| 인코딩 | `base64-encode`<br>`base64-decode`<br>`base64url-encode`<br>`base64url-decode`<br>`base32-encode`<br>`base32-decode`<br>`url-encode`<br>`url-decode`<br>`hex-encode`<br>`hex-decode`<br>`html-encode`<br>`html-decode`<br>`json-string-encode`<br>`json-string-decode`<br>`utf16le-encode`<br>`utf16le-decode`<br>`utf16be-encode`<br>`utf16be-decode` |
| 문자열·데이터 | `trim`<br>`lowercase`<br>`uppercase`<br>`format-json`<br>`minify-json`<br>`rot13`<br>`sort-lines`<br>`remove-duplicate-lines` |
| 보안 분석 | `url-defang`<br>`url-refang`<br>`jwt-decode`<br>`normalize-ip` |
| 해시·압축 | `sha256`<br>`sha512`<br>`gzip-compress`<br>`gzip-decompress`<br>`zlib-compress`<br>`zlib-decompress` |
```

표 아래 주의사항을 다음 문장까지 포함하도록 갱신한다.

```markdown
- Base64URL 인코딩은 패딩을 붙이지 않으며 `url-decode`는 `+`를 그대로 둡니다.
- `json-string-decode`는 JSON 문자열 하나만 받습니다. UTF-16 인코더는 BOM을 추가하지 않으며 디코더는 U+FEFF를 일반 문자로 보존합니다.
- `jwt-decode`는 서명을 검증하지 않습니다. Gzip과 zlib 압축은 같은 입력에 항상 같은 결과를 만듭니다.
- `zlib-decompress`는 사전 없는 RFC 1950 스트림 하나만 받고 절단·후행 데이터를 거부합니다.
- `normalize-ip`는 주소 하나만 받으며 공백·CIDR·포트·대괄호·영역 식별자를 거부합니다.
```

- [ ] **Step 6: 실제 CLI 목록과 README 예제를 대조한다**

Run:

```bash
cargo run --locked -- --list
printf '%s' '"  TOC  "' | cargo run --quiet --locked -- json-string-decode --then trim --then lowercase
rg --color=never -n "36 transforms|json-string|utf16|zlib|normalize-ip" README.md
git diff --check
```

Expected: `--list`가 기존 24개 뒤에 새 12개를 출력하고, Pipeline 예제가 줄바꿈 없이 `toc`를 출력하며, README에 네 변환군의 계약이 모두 있음.

- [ ] **Step 7: 공개 통합과 README를 같은 커밋으로 커밋한다**

```bash
git add README.md src/transforms/mod.rs src/tui/state.rs src/tui/render.rs tests/cli.rs
git diff --cached --check
git commit -m "feat(transforms): 36개 공개 변환 통합"
```

## Final Verification

- [ ] **Step 1: 승인 명세의 전체 검증 명령을 새로 실행한다**

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features --locked --color never
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps --all-features
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
git diff --check
```

Expected: 실행한 모든 명령이 종료 코드 0. 환경 때문에 실행할 수 없는 항목은 성공으로 기록하지 않고 명령과 이유를 최종 보고에 남긴다.

- [ ] **Step 2: 범위와 Git 상태를 최종 감사한다**

```bash
git status --short --branch
git diff origin/main...HEAD --stat
git log --oneline --decorate -7
```

Expected: 소스·시험·README·승인 명세·이 계획만 의도한 커밋에 포함되고, 기존의 추적되지 않은 역사 문서는 스테이징되지 않음. `Cargo.toml`과 `Cargo.lock`은 변경되지 않음.
