mod base32;
mod base64;
mod base64url;
mod compression;
mod hash;
mod hex;
mod html;
mod ioc;
#[allow(dead_code)] // Registered by the later transform-expansion task.
mod ip;
mod json;
mod jwt;
mod lines;
mod rot13;
#[allow(dead_code)] // Registered by the later transform-expansion task.
mod text;
mod url;
#[allow(dead_code)] // Registered by the later transform-expansion task.
mod utf16;

use crate::error::TransformError;

pub type TransformFn = fn(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError>;

#[derive(Clone, Copy)]
pub struct TransformDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub behavior: &'static str,
    pub accepts_binary: bool,
    pub apply: TransformFn,
}

static TRANSFORMS: &[TransformDefinition] = &[
    TransformDefinition {
        id: "base64-encode",
        display_name: "Base64 Encode",
        description: "Encode bytes using padded RFC 4648 Base64",
        behavior: "padded RFC 4648 Base64 with canonical = padding and no trailing newline",
        accepts_binary: true,
        apply: base64::encode,
    },
    TransformDefinition {
        id: "base64-decode",
        display_name: "Base64 Decode",
        description: "Decode canonical padded Base64 into bytes",
        behavior: "ignores ASCII space, tab, CR, and LF and requires canonical padding and trailing bits",
        accepts_binary: false,
        apply: base64::decode,
    },
    TransformDefinition {
        id: "url-encode",
        display_name: "URL Encode",
        description: "Percent-encode UTF-8 as an RFC 3986 component",
        behavior: "RFC 3986 component encoding with uppercase %HH; space becomes %20",
        accepts_binary: false,
        apply: url::encode,
    },
    TransformDefinition {
        id: "url-decode",
        display_name: "URL Decode",
        description: "Decode percent escapes into bytes without changing plus signs",
        behavior: "decodes %HH to bytes and leaves plus signs unchanged",
        accepts_binary: false,
        apply: url::decode,
    },
    TransformDefinition {
        id: "format-json",
        display_name: "JSON Prettify",
        description: "Indent strict JSON while preserving keys and value tokens",
        behavior: "two-space indentation while preserving key and value token spelling and order",
        accepts_binary: false,
        apply: json::format,
    },
    TransformDefinition {
        id: "minify-json",
        display_name: "JSON Minify",
        description: "Remove structural JSON whitespace while preserving tokens",
        behavior: "removes whitespace outside strings while preserving token spelling and order",
        accepts_binary: false,
        apply: json::minify,
    },
    TransformDefinition {
        id: "hex-encode",
        display_name: "Hex Encode",
        description: "Encode bytes as lowercase hexadecimal",
        behavior: "lowercase hexadecimal with two digits per byte and no prefix, separators, or trailing newline",
        accepts_binary: true,
        apply: hex::encode,
    },
    TransformDefinition {
        id: "hex-decode",
        display_name: "Hex Decode",
        description: "Decode hexadecimal text into bytes",
        behavior: "ignores ASCII space, tab, CR, and LF and accepts mixed-case digits",
        accepts_binary: false,
        apply: hex::decode,
    },
    TransformDefinition {
        id: "base64url-encode",
        display_name: "Base64URL Encode",
        description: "Encode bytes using unpadded RFC 4648 Base64URL",
        behavior: "unpadded RFC 4648 URL-safe Base64 with no trailing newline",
        accepts_binary: true,
        apply: base64url::encode,
    },
    TransformDefinition {
        id: "base64url-decode",
        display_name: "Base64URL Decode",
        description: "Decode canonical unpadded Base64URL into bytes",
        behavior: "ignores ASCII space, tab, CR, and LF and rejects padding and noncanonical trailing bits",
        accepts_binary: false,
        apply: base64url::decode,
    },
    TransformDefinition {
        id: "base32-encode",
        display_name: "Base32 Encode",
        description: "Encode bytes using padded RFC 4648 Base32",
        behavior: "uppercase RFC 4648 Base32 with canonical = padding and no trailing newline",
        accepts_binary: true,
        apply: base32::encode,
    },
    TransformDefinition {
        id: "base32-decode",
        display_name: "Base32 Decode",
        description: "Decode canonical padded Base32 into bytes",
        behavior: "ignores ASCII space, tab, CR, and LF, accepts mixed-case letters, and requires canonical padding and trailing bits",
        accepts_binary: false,
        apply: base32::decode,
    },
    TransformDefinition {
        id: "html-encode",
        display_name: "HTML Encode",
        description: "Escape HTML text-context ampersands and angle brackets",
        behavior: "escapes only &, <, and > as named entities",
        accepts_binary: false,
        apply: html::encode,
    },
    TransformDefinition {
        id: "html-decode",
        display_name: "HTML Decode",
        description: "Decode valid semicolon-terminated HTML entities",
        behavior: "decodes exact named, decimal, and hexadecimal entities and leaves invalid references unchanged",
        accepts_binary: false,
        apply: html::decode,
    },
    TransformDefinition {
        id: "rot13",
        display_name: "ROT13",
        description: "Rotate ASCII letters by 13 positions",
        behavior: "changes only ASCII A-Z and a-z and preserves all other UTF-8",
        accepts_binary: false,
        apply: rot13::apply,
    },
    TransformDefinition {
        id: "url-defang",
        display_name: "URL Defang",
        description: "Defang lowercase URL protocols and dots for IOC text",
        behavior: "replaces lowercase http:// and https:// with hxxp[://] and hxxps[://], then replaces . with [.]",
        accepts_binary: false,
        apply: ioc::defang,
    },
    TransformDefinition {
        id: "url-refang",
        display_name: "URL Refang",
        description: "Restore exact defanged URL protocols and dots",
        behavior: "replaces exact hxxp[://], hxxps[://], and [.] markers with lowercase URL text",
        accepts_binary: false,
        apply: ioc::refang,
    },
    TransformDefinition {
        id: "jwt-decode",
        display_name: "JWT Decode",
        description: "Decode a compact JWS without verifying its signature",
        behavior: "requires three canonical Base64URL parts and strict JSON object header and payload, preserves the signature, and warns Signature not verified",
        accepts_binary: false,
        apply: jwt::decode,
    },
    TransformDefinition {
        id: "sha256",
        display_name: "SHA-256",
        description: "Hash bytes with SHA-256 as lowercase hexadecimal",
        behavior: "SHA-256 digest as 64 lowercase hexadecimal digits with no trailing newline",
        accepts_binary: true,
        apply: hash::sha256,
    },
    TransformDefinition {
        id: "sha512",
        display_name: "SHA-512",
        description: "Hash bytes with SHA-512 as lowercase hexadecimal",
        behavior: "SHA-512 digest as 128 lowercase hexadecimal digits with no trailing newline",
        accepts_binary: true,
        apply: hash::sha512,
    },
    TransformDefinition {
        id: "gzip-compress",
        display_name: "Gzip Compress",
        description: "Compress bytes as a deterministic Gzip member",
        behavior: "level 6 with mtime=0, OS=255, one member, and no optional header fields",
        accepts_binary: true,
        apply: compression::compress,
    },
    TransformDefinition {
        id: "gzip-decompress",
        display_name: "Gzip Decompress",
        description: "Decompress and validate Gzip members into bytes",
        behavior: "consumes all concatenated members and validates headers, CRC and size, truncation, and trailing garbage",
        accepts_binary: true,
        apply: compression::decompress,
    },
    TransformDefinition {
        id: "sort-lines",
        display_name: "Sort Lines",
        description: "Sort UTF-8 lines by Unicode scalar order",
        behavior: "recognizes LF and CRLF, normalizes separators to LF, preserves the terminal newline, and limits input to 1000000 logical lines",
        accepts_binary: false,
        apply: lines::sort,
    },
    TransformDefinition {
        id: "remove-duplicate-lines",
        display_name: "Remove Duplicate Lines",
        description: "Remove duplicate UTF-8 lines while keeping first occurrences",
        behavior: "matches exact LF-normalized lines in original order, preserves the terminal newline, and limits input to 1000000 logical lines",
        accepts_binary: false,
        apply: lines::remove_duplicates,
    },
];

pub fn transforms() -> &'static [TransformDefinition] {
    TRANSFORMS
}

pub fn transform_by_id(id: &str) -> Option<&'static TransformDefinition> {
    TRANSFORMS.iter().find(|transform| transform.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_metadata_matches_the_four_whitespace_byte_contract() {
        let encode = transform_by_id("base64-encode").unwrap();
        assert_eq!(encode.display_name, "Base64 Encode");
        assert!(encode.accepts_binary);
        assert!(!encode.description.is_empty());

        let decode = transform_by_id("base64-decode").unwrap();
        assert_eq!(
            decode.behavior,
            "ignores ASCII space, tab, CR, and LF and requires canonical padding and trailing bits"
        );
        for input in [b"Zm9v\x0b".as_slice(), b"Zm9v\x0c"] {
            assert!(matches!(
                (decode.apply)(input, 16),
                Err(TransformError::InvalidBase64 { .. })
            ));
        }
    }

    #[test]
    fn registry_has_the_exact_public_contract_once_in_display_order() {
        let metadata: Vec<_> = transforms()
            .iter()
            .map(|transform| {
                (
                    transform.id,
                    transform.display_name,
                    transform.accepts_binary,
                )
            })
            .collect();
        let unique: std::collections::HashSet<_> = metadata.iter().map(|(id, _, _)| *id).collect();
        assert_eq!(
            metadata,
            [
                ("base64-encode", "Base64 Encode", true),
                ("base64-decode", "Base64 Decode", false),
                ("url-encode", "URL Encode", false),
                ("url-decode", "URL Decode", false),
                ("format-json", "JSON Prettify", false),
                ("minify-json", "JSON Minify", false),
                ("hex-encode", "Hex Encode", true),
                ("hex-decode", "Hex Decode", false),
                ("base64url-encode", "Base64URL Encode", true),
                ("base64url-decode", "Base64URL Decode", false),
                ("base32-encode", "Base32 Encode", true),
                ("base32-decode", "Base32 Decode", false),
                ("html-encode", "HTML Encode", false),
                ("html-decode", "HTML Decode", false),
                ("rot13", "ROT13", false),
                ("url-defang", "URL Defang", false),
                ("url-refang", "URL Refang", false),
                ("jwt-decode", "JWT Decode", false),
                ("sha256", "SHA-256", true),
                ("sha512", "SHA-512", true),
                ("gzip-compress", "Gzip Compress", true),
                ("gzip-decompress", "Gzip Decompress", true),
                ("sort-lines", "Sort Lines", false),
                ("remove-duplicate-lines", "Remove Duplicate Lines", false,),
            ]
        );
        assert_eq!(unique.len(), metadata.len());
        assert!(!unique.contains("tui"));
        assert!(transforms().iter().all(|transform| {
            !transform.display_name.is_empty() && !transform.description.is_empty()
        }));
    }
}
