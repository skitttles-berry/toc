use std::{sync::mpsc, thread};

use crate::{TUI_OUTPUT_LIMIT, error::TransformError, transforms::transform_by_id};

use super::output::Artifact;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CopyMode {
    Pretty,
    Raw,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CopyKind {
    Pretty,
    Raw,
    Hex,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ClipboardPayload {
    pub(super) text: String,
    pub(super) kind: CopyKind,
}

pub(super) struct CopyJob {
    pub(super) request_id: u64,
    pub(super) artifact: Artifact,
    pub(super) mode: CopyMode,
}

pub(super) struct PreparedCopy {
    pub(super) payload: ClipboardPayload,
    pub(super) requires_confirmation: bool,
}

pub(super) fn checked_hex_len(byte_len: usize) -> Option<usize> {
    byte_len.checked_mul(2)
}

fn binary_hex(bytes: &[u8]) -> Result<String, ()> {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let capacity = checked_hex_len(bytes.len()).ok_or(())?;
    let mut output = String::new();
    output.try_reserve_exact(capacity).map_err(|_| ())?;
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(output)
}

fn copy_exact_text(raw: &str) -> Result<String, ()> {
    let mut text = String::new();
    text.try_reserve_exact(raw.len()).map_err(|_| ())?;
    text.push_str(raw);
    Ok(text)
}

pub(super) fn format_text_for_copy(
    raw: &str,
    mode: CopyMode,
    output_limit: usize,
) -> Result<String, ()> {
    let transform_id = match mode {
        CopyMode::Pretty => "format-json",
        CopyMode::Raw => "minify-json",
    };
    let transform = transform_by_id(transform_id).expect("registered JSON copy transform");
    match (transform.apply)(raw.as_bytes(), output_limit) {
        Ok(bytes) => String::from_utf8(bytes).map_err(|_| ()),
        Err(TransformError::InvalidJson { .. }) => copy_exact_text(raw),
        Err(_) => Err(()),
    }
}

pub(super) fn clipboard_payload(
    artifact: &Artifact,
    mode: CopyMode,
) -> Result<ClipboardPayload, ()> {
    match std::str::from_utf8(artifact.bytes()) {
        Ok(raw) => Ok(ClipboardPayload {
            text: format_text_for_copy(raw, mode, TUI_OUTPUT_LIMIT)?,
            kind: match mode {
                CopyMode::Pretty => CopyKind::Pretty,
                CopyMode::Raw => CopyKind::Raw,
            },
        }),
        Err(_) => Ok(ClipboardPayload {
            text: binary_hex(artifact.bytes())?,
            kind: CopyKind::Hex,
        }),
    }
}

pub(super) fn prepare_copy(job: CopyJob) -> Result<PreparedCopy, ()> {
    let payload = clipboard_payload(&job.artifact, job.mode)?;
    let requires_confirmation =
        payload.kind != CopyKind::Hex && crate::error::contains_dangerous_control(&payload.text);
    Ok(PreparedCopy {
        payload,
        requires_confirmation,
    })
}

enum ClipboardCommand {
    Prepare(CopyJob),
    Write {
        request_id: u64,
        payload: ClipboardPayload,
    },
}

pub(super) enum ClipboardResult {
    Prepared {
        request_id: u64,
        result: Result<PreparedCopy, ()>,
    },
    Written {
        request_id: u64,
        kind: CopyKind,
        result: Result<(), ()>,
    },
}

pub(super) struct ClipboardWorker {
    commands: mpsc::Sender<ClipboardCommand>,
    results: mpsc::Receiver<ClipboardResult>,
}

impl ClipboardWorker {
    pub(super) fn new() -> Self {
        let mut clipboard = None;
        Self::new_with(move |text| {
            if clipboard.is_none() {
                clipboard = Some(arboard::Clipboard::new().map_err(|_| ())?);
            }
            clipboard.as_mut().ok_or(())?.set_text(text).map_err(|_| ())
        })
    }

    fn new_with<W>(mut write: W) -> Self
    where
        W: FnMut(String) -> Result<(), ()> + Send + 'static,
    {
        let (commands, receiver) = mpsc::channel();
        let (sender, results) = mpsc::channel();
        thread::spawn(move || {
            while let Ok(command) = receiver.recv() {
                let result = match command {
                    ClipboardCommand::Prepare(job) => {
                        let request_id = job.request_id;
                        let result = prepare_copy(job);
                        ClipboardResult::Prepared { request_id, result }
                    }
                    ClipboardCommand::Write {
                        request_id,
                        payload,
                    } => {
                        let kind = payload.kind;
                        let result = write(payload.text);
                        ClipboardResult::Written {
                            request_id,
                            kind,
                            result,
                        }
                    }
                };
                if sender.send(result).is_err() {
                    return;
                }
            }
        });
        Self { commands, results }
    }

    #[cfg(test)]
    fn new_with_writer<W>(write: W) -> Self
    where
        W: FnMut(String) -> Result<(), ()> + Send + 'static,
    {
        Self::new_with(write)
    }

    pub(super) fn prepare(&self, job: CopyJob) -> Result<(), ()> {
        self.commands
            .send(ClipboardCommand::Prepare(job))
            .map_err(|_| ())
    }

    pub(super) fn write(&self, request_id: u64, payload: ClipboardPayload) -> Result<(), ()> {
        self.commands
            .send(ClipboardCommand::Write {
                request_id,
                payload,
            })
            .map_err(|_| ())
    }

    pub(super) fn try_recv(&self) -> Result<ClipboardResult, mpsc::TryRecvError> {
        self.results.try_recv()
    }

    #[cfg(test)]
    fn recv_timeout(&self, timeout: std::time::Duration) -> Result<ClipboardResult, ()> {
        self.results.recv_timeout(timeout).map_err(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    #[test]
    fn preparation_formats_text_and_marks_only_dangerous_text_for_confirmation() {
        let safe = prepare_copy(CopyJob {
            request_id: 1,
            artifact: Artifact::new(br#"{"a":1}"#.to_vec()),
            mode: CopyMode::Pretty,
        })
        .unwrap();
        assert_eq!(safe.payload.text, "{\n  \"a\": 1\n}");
        assert!(!safe.requires_confirmation);

        let dangerous = prepare_copy(CopyJob {
            request_id: 2,
            artifact: Artifact::new(b"x\x1b[2J".to_vec()),
            mode: CopyMode::Raw,
        })
        .unwrap();
        assert!(dangerous.requires_confirmation);

        let binary = prepare_copy(CopyJob {
            request_id: 3,
            artifact: Artifact::new(vec![0x00, 0x1b, 0xff]),
            mode: CopyMode::Pretty,
        })
        .unwrap();
        assert_eq!(binary.payload.text, "001bff");
        assert_eq!(binary.payload.kind, CopyKind::Hex);
        assert!(!binary.requires_confirmation);
    }

    #[test]
    fn worker_never_writes_a_dangerous_payload_before_confirmation() {
        let writes = Arc::new(Mutex::new(Vec::new()));
        let observed = Arc::clone(&writes);
        let worker = ClipboardWorker::new_with_writer(move |text| {
            observed.lock().unwrap().push(text);
            Ok(())
        });
        worker
            .prepare(CopyJob {
                request_id: 7,
                artifact: Artifact::new(b"secret\x1b".to_vec()),
                mode: CopyMode::Pretty,
            })
            .unwrap();

        let ClipboardResult::Prepared {
            request_id,
            result: Ok(prepared),
        } = worker.recv_timeout(Duration::from_secs(1)).unwrap()
        else {
            panic!("worker did not prepare the payload");
        };
        assert_eq!(request_id, 7);
        assert!(prepared.requires_confirmation);
        assert!(writes.lock().unwrap().is_empty());

        worker.write(request_id, prepared.payload).unwrap();
        assert!(matches!(
            worker.recv_timeout(Duration::from_secs(1)).unwrap(),
            ClipboardResult::Written {
                request_id: 7,
                kind: CopyKind::Pretty,
                result: Ok(()),
            }
        ));
        assert_eq!(writes.lock().unwrap().as_slice(), ["secret\x1b"]);
    }

    #[test]
    fn worker_reports_clipboard_write_failure_without_stopping() {
        let worker = ClipboardWorker::new_with_writer(|_| Err(()));

        worker
            .write(
                11,
                ClipboardPayload {
                    text: "payload".to_string(),
                    kind: CopyKind::Raw,
                },
            )
            .unwrap();

        assert!(matches!(
            worker.recv_timeout(Duration::from_secs(1)).unwrap(),
            ClipboardResult::Written {
                request_id: 11,
                kind: CopyKind::Raw,
                result: Err(()),
            }
        ));
    }
}
