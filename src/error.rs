#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransformError {
    InvalidUtf8Input,
    InvalidBase64 {
        position: Option<usize>,
    },
    InvalidUrl {
        position: usize,
    },
    InvalidUtf8Output {
        preview_hex: String,
    },
    InvalidJson {
        line: usize,
        column: usize,
        kind: JsonErrorKind,
    },
    OutputTooLarge {
        limit: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PipelineError {
    TooManySteps {
        max: usize,
    },
    Step {
        step: usize,
        transform_id: &'static str,
        source: TransformError,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonErrorKind {
    Syntax,
    DuplicateKey,
    Bom,
    DepthExceeded,
}
