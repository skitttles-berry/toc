mod base64;
mod url;

use crate::error::TransformError;

pub type TransformFn = fn(input: &[u8], output_limit: usize) -> Result<Vec<u8>, TransformError>;

#[derive(Clone, Copy)]
pub struct TransformDefinition {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub accepts_binary: bool,
    pub apply: TransformFn,
}

static TRANSFORMS: &[TransformDefinition] = &[
    TransformDefinition {
        id: "base64-encode",
        display_name: "Base64 Encode",
        description: "Encode bytes using padded RFC 4648 Base64",
        accepts_binary: true,
        apply: base64::encode,
    },
    TransformDefinition {
        id: "base64-decode",
        display_name: "Base64 Decode",
        description: "Decode padded RFC 4648 Base64 into UTF-8 text",
        accepts_binary: false,
        apply: base64::decode,
    },
    TransformDefinition {
        id: "url-encode",
        display_name: "URL Encode",
        description: "Encode UTF-8 text as an RFC 3986 URL component",
        accepts_binary: false,
        apply: url::encode,
    },
    TransformDefinition {
        id: "url-decode",
        display_name: "URL Decode",
        description: "Decode an RFC 3986 URL component into UTF-8 text",
        accepts_binary: false,
        apply: url::decode,
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
}
