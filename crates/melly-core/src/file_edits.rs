//! A provider-independent proposal format. The model never gets a write capability.
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::path::{Component, Path};

pub const MAX_FILE_BYTES: usize = 24 * 1024;
pub const PROMPT: &str = r#"When the user asks to change or create a file in this linked project, reply normally and put one proposed file change in a fenced melly-file-edit block at the end of the reply. For an existing file use {"path":"relative/file","edits":[{"old":"exact original text that occurs once","new":"replacement"}]}. Use each nonoverlapping original substring exactly as supplied. For a new file use {"path":"relative/file","content":"complete contents"} and include all text. For an existing empty file use the content format. Use only paths relative to the linked project. If you do not have the current contents for an edit, ask the user to turn on project context or provide the relevant text. Say you proposed the edit. Melly shows the user the change before applying it. Do not emit a proposal for informational requests."#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    pub path: String,
    edits: Option<Vec<Replacement>>,
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replacement {
    old: String,
    new: String,
}

impl Proposal {
    pub fn parse(reply: &str) -> Result<Self> {
        let mut lines = reply.lines();
        lines
            .find(|line| *line == "```melly-file-edit")
            .context("The reply does not contain a file proposal")?;
        let mut block = String::new();
        loop {
            let line = lines.next().context("The file proposal is incomplete")?;
            if line.trim_end() == "```" {
                break;
            }
            ensure!(
                block.len() + line.len() < MAX_FILE_BYTES * 6,
                "The file proposal is too large"
            );
            block.push_str(line);
            block.push('\n');
        }
        ensure!(
            !lines.any(|line| line == "```melly-file-edit"),
            "The reply contains more than one file proposal"
        );
        let proposal: Self =
            serde_json::from_str(&block).context("The model returned an invalid file proposal")?;
        ensure!(
            proposal.edits.is_some() ^ proposal.content.is_some(),
            "The model must propose one change"
        );
        validate_path(&proposal.path)?;
        Ok(proposal)
    }

    pub fn edit(path: String, edits: Vec<Replacement>) -> Self {
        Self {
            path,
            edits: Some(edits),
            content: None,
        }
    }

    pub fn permits_new_file(&self) -> bool {
        self.content.is_some()
    }

    pub fn apply_to(&self, original: Option<&str>) -> Result<String> {
        let after = match (&self.edits, &self.content, original) {
            (None, Some(content), None | Some("")) => content.clone(),
            (Some(edits), None, Some(text)) => {
                ensure!(
                    !edits.is_empty() && edits.len() <= 32,
                    "A file proposal must contain 1 to 32 changes"
                );
                let mut ranges = Vec::with_capacity(edits.len());
                for replacement in edits {
                    let first = replacement
                        .old
                        .chars()
                        .next()
                        .context("The proposal contains an empty match")?;
                    let start = text
                        .find(&replacement.old)
                        .context("A proposed change no longer matches the file")?;
                    ensure!(
                        !text[start + first.len_utf8()..].contains(&replacement.old),
                        "A proposed change is ambiguous",
                    );
                    ranges.push((
                        start,
                        start + replacement.old.len(),
                        replacement.new.as_str(),
                    ));
                }
                ranges.sort_unstable_by_key(|range| range.0);
                ensure!(
                    ranges.windows(2).all(|pair| pair[0].1 <= pair[1].0),
                    "The proposed changes overlap"
                );
                let mut after = text.to_owned();
                for (start, end, replacement) in ranges.into_iter().rev() {
                    after.replace_range(start..end, replacement);
                }
                after
            }
            _ => {
                anyhow::bail!("The proposal does not match the current file. Ask for a fresh edit.")
            }
        };
        ensure!(
            after.len() <= MAX_FILE_BYTES && !after.contains('\0'),
            "The resulting file must be UTF-8 text of 24 KiB or less"
        );
        ensure!(
            original != Some(after.as_str()),
            "The proposal leaves the file unchanged"
        );
        Ok(after)
    }
}

pub fn validate_path(value: &str) -> Result<()> {
    let path = Path::new(value);
    ensure!(
        !value.is_empty()
            && value.len() <= 1024
            && !value.contains(['\\', ':'])
            && !value.chars().any(char::is_control)
            && value
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..")
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "The proposed file must be inside the linked project"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Result<Proposal> {
        Proposal::parse(&format!(
            "Here’s the change.\n\n```melly-file-edit\n{json}\n```"
        ))
    }

    #[test]
    fn only_applies_exact_unambiguous_matches() {
        let edit = parse(
            r#"{"path":"src/file.txt","edits":[{"old":"c","new":"X"},{"old":"b","new":""}]}"#,
        )
        .unwrap();
        assert_eq!(edit.apply_to(Some("abcd")).unwrap(), "aXd");
        for invalid_match in ["", "aba", "missing"] {
            let edit = parse(&format!(
                r#"{{"path":"file.txt","edits":[{{"old":"{invalid_match}","new":"x"}}]}}"#
            ))
            .unwrap();
            assert!(edit.apply_to(Some("ababa")).is_err());
        }
    }

    #[test]
    fn rejects_unsafe_or_incomplete_proposals() {
        for path in [
            "../file",
            "src/../file",
            "/tmp/file",
            "./file",
            "src//file",
            "src/",
        ] {
            assert!(parse(&format!(r#"{{"path":"{path}","content":"text"}}"#)).is_err());
        }
        let create = parse(r#"{"path":"file.txt","content":"text\n"}"#).unwrap();
        assert_eq!(create.apply_to(None).unwrap(), "text\n");
        assert!(create.apply_to(Some("existing")).is_err());
        assert!(Proposal::parse("```melly-file-edit\n{\"path\": \"foo\"}").is_err());
    }
}

#[derive(Clone, Debug, serde::Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: char,
    pub text: String,
}

pub fn diff_lines(before: Option<&str>, after: &str) -> Vec<DiffLine> {
    let before: Vec<_> = before.unwrap_or_default().split_inclusive('\n').collect();
    let after: Vec<_> = after.split_inclusive('\n').collect();
    let prefix = before
        .iter()
        .zip(&after)
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = before[prefix..]
        .iter()
        .rev()
        .zip(after[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let line = |kind, text: &str| DiffLine {
        kind,
        text: text.strip_suffix('\n').unwrap_or(text).to_owned(),
    };
    let mut diff: Vec<_> = before[prefix.saturating_sub(2)..prefix]
        .iter()
        .map(|text| line(' ', text))
        .collect();
    let old = &before[prefix..before.len() - suffix];
    let new = &after[prefix..after.len() - suffix];
    let width = new.len() + 1;
    if (old.len() + 1) * width <= 65_536 {
        let mut common = vec![0u16; (old.len() + 1) * width];
        for i in (0..old.len()).rev() {
            for j in (0..new.len()).rev() {
                let index = i * width + j;
                common[index] = if old[i] == new[j] {
                    1 + common[index + width + 1]
                } else {
                    common[index + width].max(common[index + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < old.len() || j < new.len() {
            if i < old.len() && j < new.len() && old[i] == new[j] {
                diff.push(line(' ', old[i]));
                i += 1;
                j += 1;
            } else if i < old.len()
                && (j == new.len() || common[(i + 1) * width + j] >= common[i * width + j + 1])
            {
                diff.push(line('-', old[i]));
                i += 1;
            } else {
                diff.push(line('+', new[j]));
                j += 1;
            }
        }
    } else {
        diff.extend(old.iter().map(|text| line('-', text)));
        diff.extend(new.iter().map(|text| line('+', text)));
    }
    let end = after.len() - suffix;
    diff.extend(
        after[end..end + suffix.min(2)]
            .iter()
            .map(|text| line(' ', text)),
    );
    // Keep two lines on either side of distant edits.
    let mut index = 0;
    while index < diff.len() {
        if diff[index].kind == ' ' {
            let end = index
                + diff[index..]
                    .iter()
                    .take_while(|row| row.kind == ' ')
                    .count();
            if end - index > 5 {
                diff.splice(index + 2..end - 2, [line('…', "")]);
                index += 5;
                continue;
            }
            index = end;
        } else {
            index += 1;
        }
    }
    diff
}

#[cfg(test)]
#[test]
fn diff_keeps_both_edits_and_folds_the_unchanged_gap() {
    let before = "one\ntwo\n3\n4\n5\n6\n7\n8\n9\nlast\n";
    let after = "ONE\ntwo\n3\n4\n5\n6\n7\n8\n9\nLAST\n";
    let diff = diff_lines(Some(before), after);
    assert_eq!(
        diff.iter().map(|row| row.kind).collect::<String>(),
        "-+  …  -+"
    );
    assert_eq!(diff.last().unwrap().text, "LAST");
}
