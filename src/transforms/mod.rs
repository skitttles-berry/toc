mod base64;
mod json;
mod url;

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
        description: "Decode canonical padded Base64 into UTF-8 text",
        behavior: "ignores ASCII whitespace, requires canonical padding, and returns only valid UTF-8",
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
        description: "Decode percent escapes into UTF-8 without changing plus signs",
        behavior: "decodes %HH to UTF-8 and leaves plus signs unchanged",
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
    fn base64_encode_is_registered_with_metadata() {
        let transform = transform_by_id("base64-encode").unwrap();
        assert_eq!(transform.display_name, "Base64 Encode");
        assert!(transform.accepts_binary);
        assert!(!transform.description.is_empty());
    }

    #[test]
    fn registry_has_exact_public_ids_once() {
        let ids: Vec<_> = transforms().iter().map(|transform| transform.id).collect();
        assert_eq!(
            ids,
            [
                "base64-encode",
                "base64-decode",
                "url-encode",
                "url-decode",
                "format-json",
                "minify-json",
            ]
        );
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), 6);
        assert!(!ids.contains(&"tui"));
        assert!(transforms().iter().all(|transform| {
            !transform.display_name.is_empty() && !transform.description.is_empty()
        }));
    }
}
