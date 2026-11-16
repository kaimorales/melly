//! Display-only reasoning channels. Protocol signatures and encrypted data never enter these events.
use crate::models::{ProviderEvent, ProviderSink};
use std::sync::{Arc, Mutex};

#[derive(Default)]
enum Channel {
    #[default]
    Prefix,
    Thinking(&'static str),
    Answer,
}
#[derive(Default)]
struct Splitter {
    channel: Channel,
    pending: String,
}
impl Splitter {
    fn push(&mut self, text: &str, sink: &ProviderSink) {
        if matches!(self.channel, Channel::Answer) {
            if !text.is_empty() {
                sink(ProviderEvent::Delta(text.into()));
            }
            return;
        }
        self.pending.push_str(text);
        loop {
            match self.channel {
                Channel::Prefix => {
                    let candidate = self.pending.trim_start();
                    let whitespace = self.pending.len() - candidate.len();
                    let tags = [("<think>", "</think>"), ("<thinking>", "</thinking>")];
                    if let Some((open, close)) =
                        tags.iter().find(|(open, _)| candidate.starts_with(open))
                    {
                        self.pending.drain(..whitespace + open.len());
                        self.channel = Channel::Thinking(close);
                    } else if whitespace <= 64
                        && (candidate.is_empty()
                            || tags.iter().any(|(open, _)| open.starts_with(candidate)))
                    {
                        return;
                    } else {
                        self.channel = Channel::Answer;
                    }
                }
                Channel::Thinking(close) => {
                    if let Some(end) = self.pending.find(close) {
                        let reasoning: String = self.pending.drain(..end).collect();
                        if !reasoning.is_empty() {
                            sink(ProviderEvent::Reasoning(reasoning));
                        }
                        self.pending.drain(..close.len());
                        self.channel = Channel::Prefix;
                    } else {
                        // Retain only a possible partial closing tag between chunks.
                        let keep = (1..close.len())
                            .rev()
                            .find(|&n| self.pending.ends_with(&close[..n]))
                            .unwrap_or(0);
                        let end = self.pending.len() - keep;
                        if end > 0 {
                            sink(ProviderEvent::Reasoning(
                                self.pending.drain(..end).collect(),
                            ));
                        }
                        return;
                    }
                }
                Channel::Answer => {
                    if !self.pending.is_empty() {
                        sink(ProviderEvent::Delta(std::mem::take(&mut self.pending)));
                    }
                    return;
                }
            }
        }
    }
    fn finish(&mut self, sink: &ProviderSink) {
        if !self.pending.is_empty() {
            let text = std::mem::take(&mut self.pending);
            sink(if matches!(self.channel, Channel::Thinking(_)) {
                ProviderEvent::Reasoning(text)
            } else {
                ProviderEvent::Delta(text)
            });
        }
    }
}

pub(super) fn display_sink(sink: ProviderSink) -> ProviderSink {
    let splitter = Mutex::new(Splitter::default());
    Arc::new(move |event| match event {
        ProviderEvent::Delta(text) => splitter.lock().unwrap().push(&text, &sink),
        ProviderEvent::Done => {
            splitter.lock().unwrap().finish(&sink);
            sink(ProviderEvent::Done);
        }
        other => sink(other),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn split(chunks: &[&str]) -> (String, String) {
        let result = Arc::new(Mutex::new((String::new(), String::new())));
        let captured = result.clone();
        let sink = display_sink(Arc::new(move |event| {
            let mut pair = captured.lock().unwrap();
            match event {
                ProviderEvent::Delta(s) => pair.0.push_str(&s),
                ProviderEvent::Reasoning(s) => pair.1.push_str(&s),
                _ => {}
            }
        }));
        for chunk in chunks {
            sink(ProviderEvent::Delta((*chunk).into()));
        }
        sink(ProviderEvent::Done);
        result.lock().unwrap().clone()
    }
    #[test]
    fn every_tag_boundary_is_split_without_leaking_into_answer() {
        let text = "  <think>Compare café options.</think>\nThe answer.";
        for at in 0..=text.len() {
            if text.is_char_boundary(at) {
                assert_eq!(
                    split(&[&text[..at], &text[at..]]),
                    ("\nThe answer.".into(), "Compare café options.".into())
                );
            }
        }
        assert_eq!(
            split(&["<thi", "nking>one</thin", "king><think>two</think>answer"]),
            ("answer".into(), "onetwo".into())
        );
    }
    #[test]
    fn literal_examples_and_normal_answers_stay_unchanged() {
        for text in [
            "Hello",
            "<",
            "```xml\n<think>example</think>\n```",
            "Use <think> as an example",
            "<thought>literal</thought>",
        ] {
            assert_eq!(split(&[text]), (text.into(), String::new()));
        }
        assert_eq!(
            split(&["<think>unfinished"]),
            (String::new(), "unfinished".into())
        );
    }
}
