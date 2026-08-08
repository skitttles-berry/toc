# Task 3 report: strict JWT decoding

## Changes

- Added an internal `jwt::decode` compact-JWS decoder. It trims only ASCII space, tab, CR, and LF around the complete token; requires exactly three segments; rejects segment whitespace and non-canonical Base64URL; validates but does not verify the signature; and preserves the original signature segment.
- Added the safe `TransformError::InvalidJwtPart` rendering. JWT structure, header, payload, and signature failures map to this error without token contents.
- Added `json::format_object`, retaining the existing strict parser, duplicate-key/depth policy, and source token/key order while requiring an object at the top level for JWT header and payload.
- The JWT output is composed with limit checks at each append in `header`, `payload`, `signature`, `warning` order using two-space JSON indentation.
- `jwt-decode` remains unregistered as required. No verifier, key handling, or claim interpretation was added.

## Files

- `src/transforms/jwt.rs` (new): internal decoder and focused tests.
- `src/transforms/json.rs`: object-only formatting boundary.
- `src/error.rs`: safe JWT-part error and renderer test.
- `src/transforms/mod.rs`, `src/tui/views.rs`: internal module and exhaustive error rendering.

## TDD evidence

- RED: `rtk cargo test --lib transforms::jwt::tests::decodes_compact_jws_with_stable_two_space_json` failed with `cannot find function decode in this scope`.
- GREEN: the same focused test passed after the minimum decoder implementation.
- Focused suite: `rtk cargo test --lib transforms::jwt::tests` passed, 6 tests.

## Verification

- `rtk cargo test --lib` passed: 315 passed, 3 ignored.
- `rtk cargo clippy --lib -- -D warnings` passed with no issues.
- `rtk cargo fmt` and `rtk cargo fmt --check` passed.
- `rtk git diff --check` passed.

## Self-review

- Confirmed structure, canonical/non-canonical Base64URL, outer-only whitespace trimming, strict object JSON, duplicate/depth rejection, signature preservation including empty signature, exact formatting, safe errors, and final-composition output-limit behavior are covered.
- Confirmed no registry entry or cryptographic verification was added.

## Concerns

- The decoder is deliberately `dead_code`-allowed until Task 5 registers it; the object formatter has the same narrow allowance because its only caller is the deferred internal decoder.
