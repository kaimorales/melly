use serde::Serialize;
use std::collections::HashSet;
use std::fs::{self, File, Metadata};
use std::io::{self, Read};
use std::path::Path;

const FILE_LIMIT: usize = 12 * 1024;
const PACKET_LIMIT: usize = 32 * 1024;
const BUDGET_EXCEEDED: &str = "packet budget exceeded";

#[derive(Default, Serialize)]
struct Packet<'a> {
    files: Vec<Source<'a>>,
    omitted: Vec<Omission<'a>>,
}

#[derive(Serialize)]
struct Source<'a> {
    path: &'a str,
    content: String,
}

#[derive(Serialize)]
struct Omission<'a> {
    path: &'a str,
    reason: &'static str,
}

impl Packet<'_> {
    fn serialize(&self) -> String {
        serde_json::to_string(self).expect("source packet contains only strings and arrays")
    }
}

/// Build a packet from at most sixteen validated relative paths of at most 512 bytes each.
pub(crate) fn build<'a>(workspace: &Path, paths: impl Iterator<Item = &'a str>) -> String {
    let root = workspace.canonicalize();
    let mut requested = HashSet::new();
    let mut resolved = HashSet::new();
    let mut packet = Packet::default();
    let mut candidates = Vec::new();

    for path in paths {
        if !requested.insert(path) {
            continue;
        }
        let content = match &root {
            Ok(root) => {
                let requested = root.join(path);
                match requested.canonicalize() {
                    Ok(target) if !target.starts_with(root) => Err("outside workspace"),
                    Ok(target) => {
                        if !resolved.insert(target.clone()) {
                            continue;
                        }
                        read_source(&requested, &target)
                    }
                    Err(error) => Err(io_reason(&error)),
                }
            }
            Err(error) => Err(io_reason(error)),
        };
        let reason = match content {
            Ok(content) => {
                candidates.push((packet.omitted.len(), Source { path, content }));
                BUDGET_EXCEEDED
            }
            Err(reason) => reason,
        };
        packet.omitted.push(Omission { path, reason });
    }

    // Reserve room for every omission before promoting whole files in request order.
    // Even fully escaped paths fit as omissions under the validated count and length limits.
    let mut serialized = packet.serialize();
    for (index, source) in candidates {
        let index = index - packet.files.len();
        let omission = packet.omitted.remove(index);
        packet.files.push(source);
        let candidate = packet.serialize();
        if candidate.len() <= PACKET_LIMIT {
            serialized = candidate;
        } else {
            packet.files.pop();
            packet.omitted.insert(index, omission);
        }
    }
    serialized
}

fn read_source(requested: &Path, target: &Path) -> Result<String, &'static str> {
    let initial = fs::metadata(target).map_err(|error| io_reason(&error))?;
    if initial.is_dir() {
        return Err("directory");
    }
    if !initial.is_file() {
        return Err("not a regular file");
    }
    if initial.len() > FILE_LIMIT as u64 {
        return Err("too large");
    }

    let file = File::open(target).map_err(|error| io_reason(&error))?;
    let opened = file.metadata().map_err(|error| io_reason(&error))?;
    // This recheck catches ordinary replacements. Filesystem permissions remain the
    // security boundary against adversarial changes during path resolution and opening.
    if !same_source(&initial, &opened)
        || requested.canonicalize().as_deref().ok() != Some(target)
        || !fs::metadata(target).is_ok_and(|current| same_source(&opened, &current))
    {
        return Err("changed during capture");
    }
    let mut limited = file.take((FILE_LIMIT + 1) as u64);
    let mut bytes = Vec::new();
    limited
        .read_to_end(&mut bytes)
        .map_err(|error| io_reason(&error))?;
    if bytes.len() > FILE_LIMIT {
        return Err("too large");
    }
    let read = limited
        .get_ref()
        .metadata()
        .map_err(|error| io_reason(&error))?;
    if !same_source(&opened, &read)
        || bytes.len() as u64 != read.len()
        || requested.canonicalize().as_deref().ok() != Some(target)
    {
        return Err("changed during capture");
    }
    if bytes.contains(&0) {
        return Err("contains NUL");
    }
    String::from_utf8(bytes).map_err(|_| "invalid UTF-8")
}

fn same_source(before: &Metadata, after: &Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != after.dev() || before.ino() != after.ino() {
            return false;
        }
    }
    after.is_file()
        && before.len() == after.len()
        && before
            .modified()
            .is_ok_and(|modified| after.modified().is_ok_and(|other| modified == other))
}

fn io_reason(error: &io::Error) -> &'static str {
    match error.kind() {
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory => "missing",
        _ => "unreadable",
    }
}
