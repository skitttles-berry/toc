# toc 균형형 변환 확장 설계

**작성일:** 2026-08-16

**상태:** 사용자 승인·구현 전

**대상:** `toc` CLI와 `toc tui`

## 1. 목적

현재 24개 변환에 문자열, JSON 문자열, 문자 인코딩, 압축, IP 정규화 기능을 고르게
추가한다. 공개 표준에 근거한 결정론적 변환만 제공하고, 기존 로컬 전용 바이트 Pipeline과
출력 안전 경계를 유지한다.

Boop과 CyberChef는 기능 후보를 찾기 위한 참고 자료로만 사용한다. 두 프로젝트의 코드,
스크립트, 명령 문법, Recipe 형식을 복제하거나 호환 대상으로 삼지 않는다.

- [Boop 기본 스크립트](https://github.com/IvanMathy/Boop/tree/main/Boop/Boop/scripts)
- [CyberChef 기능 분류](https://github.com/gchq/CyberChef/blob/master/src/core/config/Categories.json)

## 2. 범위

기존 24개 공개 ID와 표시 순서를 그대로 두고 아래 12개를 뒤에 추가한다.

| 순서 | ID | 표시 이름 | 입력 | 구현 |
|---:|---|---|---|---|
| 25 | `trim` | Trim | 텍스트 | `text::trim` |
| 26 | `lowercase` | Lowercase | 텍스트 | `text::lowercase` |
| 27 | `uppercase` | Uppercase | 텍스트 | `text::uppercase` |
| 28 | `json-string-encode` | JSON String Encode | 텍스트 | `json::string_encode` |
| 29 | `json-string-decode` | JSON String Decode | 텍스트 | `json::string_decode` |
| 30 | `utf16le-encode` | UTF-16LE Encode | 텍스트 | `utf16::encode_le` |
| 31 | `utf16le-decode` | UTF-16LE Decode | 바이트 | `utf16::decode_le` |
| 32 | `utf16be-encode` | UTF-16BE Encode | 텍스트 | `utf16::encode_be` |
| 33 | `utf16be-decode` | UTF-16BE Decode | 바이트 | `utf16::decode_be` |
| 34 | `zlib-compress` | Zlib Compress | 바이트 | `compression::zlib_compress` |
| 35 | `zlib-decompress` | Zlib Decompress | 바이트 | `compression::zlib_decompress` |
| 36 | `normalize-ip` | Normalize IP | 텍스트 | `ip::normalize` |

이번 범위에서는 단계별 옵션, Pipeline 저장·불러오기, 범용 Recipe 엔진, 자동 형식 감지를
추가하지 않는다. raw DEFLATE, NDJSON, IDNA, Base45, 레거시 해시, 파일 컨테이너도 후속
요구가 확인될 때 별도로 설계한다.

## 3. 구조

`TransformFn`과 `TransformDefinition`을 변경하지 않는다. 각 변환은 기존
`fn(&[u8], output_limit) -> Result<Vec<u8>, TransformError>` 계약을 구현하고
`src/transforms/mod.rs`의 단일 레지스트리에 등록한다. CLI의 직접 명령과 `--then`, TUI
선택기, `--list`는 기존처럼 이 레지스트리를 공유한다.

내부 파일은 다음과 같이 구성한다.

- `text.rs`: Trim, Lowercase, Uppercase
- `json.rs`: 기존 JSON 포맷·축소와 새 JSON 문자열 변환
- `utf16.rs`: 명시적 리틀·빅 엔디언 UTF-16 변환
- `compression.rs`: 기존 Gzip과 새 zlib 변환, 출력 제한 작성기
- `ip.rs`: 단일 IP 주소 파싱과 정규 출력

기존 `gzip.rs`는 `compression.rs`로 이름을 일반화한다. 현재 `LimitedWriter`를 Gzip과
zlib 압축이 공유하여 같은 제한 작성기를 다시 구현하지 않는다. 기존 Gzip 공개 동작과 ID는
바꾸지 않는다.

새 의존성은 없다. Rust 표준 라이브러리와 현재 잠긴 `serde_json 1.0.151`, 순수 Rust
백엔드의 `flate2 1.1.9`만 사용한다.

## 4. 변환 계약

### 4.1 문자열

`trim`은 유효한 UTF-8만 받는다. Rust 표준 라이브러리가 Unicode 공백으로 판정하는 문자를
입력 양끝에서 제거하고 내부의 공백, 줄바꿈, 문자 순서는 그대로 둔다. 입력 전체가 공백이면
빈 결과를 반환한다.

`lowercase`와 `uppercase`는 유효한 UTF-8만 받는다. Rust 표준 라이브러리의 Unicode 기본
대소문자 매핑을 사용하고 로케일별 규칙이나 Unicode 정규화를 적용하지 않는다. 한 Unicode
스칼라 값이 여러 문자로 확장될 수 있으므로 쓰는 동안 출력 한도를 검사한다.

### 4.2 JSON 문자열

`json-string-encode`는 입력 전체를 하나의 문자열로 취급하고 따옴표를 포함한 완전한 JSON
문자열 리터럴을 출력한다. 따옴표, 역슬래시, U+0000부터 U+001F까지의 제어 문자는
[RFC 8259 7절](https://www.rfc-editor.org/rfc/rfc8259.html#section-7)에 맞게 이스케이프한다.
다른 비 ASCII 문자는 UTF-8로 유지하며 끝에 줄바꿈을 붙이지 않는다.

`json-string-decode`는 앞뒤 JSON 공백을 제외하고 정확히 하나의 JSON 문자열 리터럴만
허용한다. 객체, 배열, 숫자, 논리값, `null`, 후행 비공백 데이터, UTF-8 BOM, 잘못된
이스케이프와 짝이 맞지 않는 서로게이트를 거부한다. 결과는 디코딩된 UTF-8 문자열이며
줄바꿈을 추가하지 않는다.

### 4.3 UTF-16

인코더는 유효한 UTF-8을 지정된 엔디언의 UTF-16 코드 단위로 변환한다. BMP 밖의 문자는
서로게이트 쌍으로 기록한다. BOM은 자동으로 추가하지 않는다.

디코더는 바이트 수가 짝수인 입력만 허용하고 지정된 엔디언으로 코드 단위를 읽는다. 짝이
맞지 않는 상위·하위 서로게이트는 해당 코드 단위의 바이트 위치와 함께 거부한다. BOM을
감지하거나 제거하지 않으므로 입력의 U+FEFF 코드 단위는 일반 문자 U+FEFF로 보존한다.
명시적 엔디언 ID를 사용하므로 다른 엔디언으로 자동 재시도하지 않는다.

오류 위치는 0부터 시작하는 바이트 오프셋이다. 홀수 길이는 마지막 짝 없는 바이트를,
잘못된 서로게이트는 해당 코드 단위의 첫 바이트를 가리킨다.

이 계약은 [Unicode UTF와 BOM 안내](https://unicode.org/faq/utf_bom.html)를 기준으로 하되,
숨은 BOM 처리 없이 바이트 변환의 가역성을 우선한다.

### 4.4 zlib

`zlib-compress`는 임의 바이트를 [RFC 1950](https://www.rfc-editor.org/rfc/rfc1950.html)
컨테이너로 압축한다. 압축 수준은 6, 사전은 없음으로 고정한다. 같은 잠금 의존성과 입력에서는
같은 바이트를 출력한다.

`zlib-decompress`는 정확히 하나의 zlib 스트림만 허용한다. 헤더, DEFLATE 데이터,
Adler-32, 절단, 후행 바이트를 검증하고 사전 사용 스트림을 거부한다. Gzip이나 raw
DEFLATE로 자동 전환하지 않는다. 제한보다 한 바이트까지만 추가로 읽어 초과를 판정하며,
전체 압축 해제 결과를 먼저 할당하지 않는다.

### 4.5 IP 정규화

`normalize-ip`는 공백 없는 단일 IPv4 또는 IPv6 주소만 허용한다. CIDR, 포트, URL
대괄호, IPv6 영역 식별자와 앞뒤 공백을 거부한다.

IPv4는 각 옥텟을 0부터 255까지의 십진수로 받고, 한 자리 `0`을 제외한 선행 0을 거부한 뒤
표준 점 십진수로 출력한다. IPv6는 [RFC 5952](https://www.rfc-editor.org/rfc/rfc5952.html)에
따라 16진수를 소문자로 쓰고 선행 0을 제거한다. 0 필드가 둘 이상 이어진 가장 긴 구간만
`::`로 축약하고, 길이가 같으면 가장 왼쪽 구간을 선택한다. 단일 0 필드는 축약하지 않는다.
결과에 줄바꿈을 붙이지 않는다.

## 5. 오류와 안전 경계

다음 오류만 추가한다.

| 오류 | 공개 의미 |
|---|---|
| `JsonErrorKind::ExpectedString` | 입력 JSON 값이 문자열이 아님 |
| `InvalidUtf16 { position }` | 해당 바이트 위치에서 UTF-16이 잘못됨 |
| `InvalidZlib` | zlib 헤더·데이터·체크섬·종료가 잘못됨 |
| `InvalidIpAddress` | 허용 범위의 단일 IP 주소가 아님 |

출력 초과는 기존 `OutputTooLarge`, UTF-8 입력 오류는 기존 `InvalidUtf8Input`을 사용한다.
Pipeline 오류는 기존처럼 1부터 시작하는 단계 번호와 변환 ID를 포함한다. 오류 메시지와
Trace에는 원본 문자열, IP, 압축 데이터가 포함되지 않는다. 실패 시 부분 결과를 Artifact나
표준 출력에 기록하지 않는다.

CLI 입력 64 MiB·단계 출력 256 MiB, TUI 입력 1 MiB·단계 출력 64 MiB, Pipeline 최대
32단계 제한을 유지한다. `accepts_binary=false` 단계 앞의 UTF-8 검사, 실제 터미널의
비 UTF-8·위험 제어 문자 출력 거부, 리디렉션 시 원시 바이트 보존도 변경하지 않는다.

모든 가변 길이 변환은 필요한 길이를 사전 계산하거나 제한 작성기에 쓰면서 상한을 지킨다.
zlib 해제는 스트리밍 상한으로 압축 폭탄의 메모리 확대를 제한한다. 네트워크 접근, 외부 명령,
스크립트 평가를 추가하지 않는다.

## 6. 검증 계획

### 6.1 단위시험

- 문자열: 빈 입력, 전부 공백, 내부 공백 보존, Unicode 공백, 대소문자 다중 확장,
  잘못된 UTF-8, 정확한 출력 한도
- JSON 문자열: 따옴표·역슬래시·제어 문자·한글·서로게이트 쌍 왕복, 비문자 JSON,
  BOM, 후행 데이터, 잘못된 이스케이프, 출력 한도
- UTF-16: 빈 입력, ASCII·한글·BMP 밖 문자 왕복, 엔디언 차이, 홀수 바이트, 단독
  상위·하위 서로게이트, BOM 문자 보존, 출력 한도
- zlib: 알려진 벡터, 같은 입력의 동일 출력, 빈 데이터, 잘못된 헤더와 체크섬, 절단,
  후행 데이터, 사전 사용, 압축·해제 출력 한도
- IP: IPv4, 대문자·확장형 IPv6, 0 구간 축약 동률, IPv4 포함 IPv6, 공백·CIDR·포트·
  대괄호·영역 식별자 거부

### 6.2 통합시험

- 레지스트리가 정확히 36개이며 기존 24개 순서와 메타데이터가 유지되고 새 ID가 중복되지 않음
- 12개 새 변환의 대표 입력을 CLI에서 각각 실행하고 성공 결과 끝에 임의 줄바꿈이 없음
- JSON 문자열과 UTF-16, zlib의 인코드·디코드 `--then` 왕복
- UTF-16·zlib의 이진 중간 결과가 바이트 허용 다음 단계에 전달됨
- 실패한 단계가 표준 출력을 비우고 종료 코드 4와 단계 번호·ID를 보고함
- `--list`, 변환 도움말, TUI Picker 검색과 상세 설명이 36개 레지스트리를 반영함
- 새 표시 이름이 좁은 TUI 선택기와 상세 영역을 깨뜨리지 않음

### 6.3 완료 검증

구현 완료 전 다음을 새로 실행한다.

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features --locked --color never
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps --all-features
bash tests/shell-smoke.sh
zsh tests/shell-smoke.sh
git diff --check
```

환경 의존 경로는 실행 가능한 환경만 검증하고 미실행 항목을 성공으로 표현하지 않는다.

## 7. 문서 동기화

구현과 같은 커밋에서 README를 다음과 같이 현행화한다.

- 상단 배지를 `36 transforms`로 변경
- 지원 변환 표에 문자열, JSON 문자열, UTF-16, zlib, IP 변환 추가
- UTF-16이 BOM을 자동 처리하지 않는다는 점 명시
- zlib이 한 스트림과 후행 데이터까지 엄격히 검증한다는 점 명시
- IP 입력이 주소 하나로 제한된다는 점 명시
- 새 변환을 포함한 실제 CLI Pipeline 예제 최소 하나 추가

현재 저장소에는 `ARCHITECTURE.md`, `DESIGN.md`, `docs/design/`이 없으므로 새 파일을 만들지
않는다. 이 명세와 README를 관련 문서로 사용한다.

## 8. 완료 기준

- 12개 공개 ID가 CLI와 TUI에서 동일한 동작으로 노출된다.
- 기존 24개 ID, 순서, 키 바인딩, Pipeline 문법, 출력 안전 경계에 회귀가 없다.
- 모든 새 변환이 입력·출력 한도와 오류 비노출 계약을 지킨다.
- 새 의존성, 옵션 체계, Recipe 엔진 없이 구현된다.
- 전체 검증이 통과하고 README가 실제 `toc --list`와 일치한다.
