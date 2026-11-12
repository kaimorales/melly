//! Local session navigation; opening and searching this view makes no model calls.
use std::time::SystemTime;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};

use crate::visual::{BORDER, INK, MUTED, SELECTED, SIDEBAR};
use crate::{state::SessionSummary, ui::ellipsis};

#[derive(Debug, PartialEq, Eq)]
pub enum SessionAction {
    History,
    Resume(String),
    New,
}

pub struct SessionPicker {
    sessions: Vec<SessionSummary>,
    cwd: String,
    current: Option<String>,
    query: String,
    all_projects: bool,
    selection: usize,
    pub error: Option<String>,
}

impl SessionPicker {
    pub fn new(sessions: Vec<SessionSummary>, cwd: String, current: Option<String>) -> Self {
        Self {
            sessions,
            cwd,
            current,
            query: String::new(),
            all_projects: false,
            selection: 0,
            error: None,
        }
    }

    fn filtered(&self) -> Vec<&SessionSummary> {
        let words: Vec<String> = self
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        self.sessions
            .iter()
            .filter(|s| {
                (self.all_projects || s.cwd == self.cwd) && {
                    let haystack = format!("{} {} {}", s.title, s.cwd, s.id).to_lowercase();
                    words.iter().all(|word| haystack.contains(word))
                }
            })
            .collect()
    }

    pub fn insert(&mut self, text: &str) {
        let left = 120usize.saturating_sub(self.query.chars().count());
        self.query
            .extend(text.chars().filter(|c| !c.is_control()).take(left));
        self.selection = 0;
        self.error = None;
    }

    pub fn key(&mut self, key: KeyEvent) -> Option<SessionAction> {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('n') => return Some(SessionAction::New),
                KeyCode::Char('u') => {
                    self.query.clear();
                    self.selection = 0;
                }
                _ => {}
            }
            return None;
        }
        let count = self.filtered().len();
        match key.code {
            KeyCode::Down => self.selection = (self.selection + 1) % count.max(1),
            KeyCode::Up => {
                self.selection = self
                    .selection
                    .checked_sub(1)
                    .unwrap_or(count.saturating_sub(1))
            }
            KeyCode::Home => self.selection = 0,
            KeyCode::End => self.selection = count.saturating_sub(1),
            KeyCode::Tab | KeyCode::BackTab => {
                self.all_projects = !self.all_projects;
                self.selection = 0;
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.selection = 0;
                self.error = None;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::ALT) => {
                self.insert(&c.to_string())
            }
            KeyCode::Enter => {
                return self
                    .filtered()
                    .get(self.selection)
                    .map(|s| SessionAction::Resume(s.id.clone()));
            }
            _ => {}
        }
        None
    }

    pub fn draw(&self, frame: &mut Frame, accent: Color, busy: bool) {
        let area = frame.area();
        if area.width < 24 || area.height < 8 {
            return;
        }
        let width = area.width.saturating_sub(4).min(84);
        let rows = self.filtered();
        let height = area
            .height
            .saturating_sub(2)
            .min(self.sessions.len().clamp(3, 8) as u16 * 2 + 7);
        let popup = Rect::new(
            (area.width - width) / 2,
            (area.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup);
        let (bg, ink, muted) = (SIDEBAR, INK, MUTED);
        let block = Block::bordered()
            .title(Line::styled(
                " Sessions ",
                Style::default().fg(ink).add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(bg).fg(ink))
            .border_style(Style::default().fg(BORDER));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let content = Rect::new(
            inner.x + 1,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height,
        );
        let scope = if self.all_projects {
            "All projects"
        } else {
            "This project"
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(scope, Style::default().fg(accent)),
                Span::styled(
                    format!("  ·  {}  ·  Tab change", rows.len()),
                    Style::default().fg(muted),
                ),
            ])),
            Rect::new(content.x, content.y, content.width, 1),
        );
        let query = if self.query.is_empty() {
            "Search sessions…"
        } else {
            &self.query
        };
        frame.render_widget(
            Paragraph::new(format!(
                "› {}",
                ellipsis(query, content.width.saturating_sub(2) as usize)
            ))
            .style(Style::default().fg(if self.query.is_empty() {
                muted
            } else {
                ink
            })),
            Rect::new(content.x, content.y + 1, content.width, 1),
        );
        let (offset, row_height, bottom_gap) = if content.height < 7 {
            (2, 1, 1)
        } else {
            (3, 2, 2)
        };
        let capacity = content
            .height
            .saturating_sub(offset + bottom_gap)
            .div_euclid(row_height) as usize;
        if capacity > 0 && rows.is_empty() {
            let message = if self.query.is_empty() {
                "No saved sessions here. Ctrl+N starts one."
            } else {
                "No matches. Tab searches other projects."
            };
            frame.render_widget(
                Paragraph::new(message).style(Style::default().fg(muted)),
                Rect::new(content.x, content.y + offset, content.width, 1),
            );
        }
        let first = self.selection.saturating_sub(capacity.saturating_sub(1));
        let now = SystemTime::now();
        for (row, (index, session)) in rows
            .iter()
            .enumerate()
            .skip(first)
            .take(capacity)
            .enumerate()
        {
            let selected = index == self.selection;
            let title = if session.title.is_empty() {
                "Untitled session".into()
            } else {
                session_title(&session.title)
            };
            let current = self.current.as_deref() == Some(session.id.as_str());
            let suffix = if current { " · current" } else { "" };
            let title = ellipsis(
                &title,
                content.width.saturating_sub(2 + suffix.len() as u16) as usize,
            );
            let project = std::path::Path::new(&session.cwd)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or(&session.cwd);
            let detail = format!(
                "  {} · {} · {} Luna · {}",
                session_title(project),
                age(session.modified, now),
                session.workers,
                ellipsis(&session.id, 8)
            );
            let lines = vec![
                Line::styled(
                    format!("{} {title}{suffix}", if selected { "›" } else { " " }),
                    Style::default()
                        .fg(if selected { accent } else { ink })
                        .add_modifier(Modifier::BOLD),
                ),
                Line::styled(
                    ellipsis(&detail, content.width as usize),
                    Style::default().fg(muted),
                ),
            ];
            frame.render_widget(
                Paragraph::new(lines).style(Style::default().bg(if selected {
                    SELECTED
                } else {
                    bg
                })),
                Rect::new(
                    content.x,
                    content.y + offset + row as u16 * row_height,
                    content.width,
                    row_height,
                ),
            );
        }
        let hint = self.error.as_deref().unwrap_or(if busy {
            "Team working · browse now, switch when idle"
        } else {
            "↑↓ select  Enter resume  Ctrl+N new  Esc back"
        });
        frame.render_widget(
            Paragraph::new(hint).style(Style::default().fg(if self.error.is_some() {
                accent
            } else {
                muted
            })),
            Rect::new(
                content.x,
                content.bottom().saturating_sub(1),
                content.width,
                1,
            ),
        );
    }
}

pub fn session_title(text: &str) -> String {
    let clean: String = text
        .chars()
        .filter(|c| !c.is_control() || c.is_whitespace())
        .collect();
    ellipsis(&clean.split_whitespace().collect::<Vec<_>>().join(" "), 100)
}

fn age(modified: SystemTime, now: SystemTime) -> String {
    let seconds = now.duration_since(modified).unwrap_or_default().as_secs();
    match seconds {
        0..60 => "just now".into(),
        60..3600 => format!("{}m ago", seconds / 60),
        3600..86400 => format!("{}h ago", seconds / 3600),
        _ => format!("{}d ago", seconds / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests::{capture, screen};
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn history_searches_titles_projects_and_ids_and_handles_empty_results() {
        let rows = vec![
            SessionSummary {
                id: "portfolio-1".into(),
                title: "Fix the home transition".into(),
                cwd: "/work/portfolio".into(),
                workers: 2,
                modified: SystemTime::now(),
            },
            SessionSummary {
                id: "melly-2".into(),
                title: "Restore the team on resume".into(),
                cwd: "/work/melly".into(),
                workers: 1,
                modified: SystemTime::now(),
            },
        ];
        let mut p = SessionPicker::new(rows, "/work/melly".into(), Some("melly-2".into()));
        assert_eq!(p.filtered().len(), 1);
        let key = |c| KeyEvent::new(c, KeyModifiers::NONE);
        p.key(key(KeyCode::Tab));
        assert_eq!(p.filtered().len(), 2);
        let mut t = Terminal::new(TestBackend::new(130, 38)).unwrap();
        t.draw(|f| {
            f.render_widget(
                Block::default().style(Style::default().bg(Color::Rgb(11, 11, 11))),
                f.area(),
            );
            p.draw(f, Color::Rgb(234, 188, 110), false);
        })
        .unwrap();
        capture(&t, "sessions");
        assert!(screen(&t).contains("2 Luna"));
        p.insert("PORTFOLIO transition");
        assert_eq!(
            p.key(key(KeyCode::Enter)),
            Some(SessionAction::Resume("portfolio-1".into()))
        );
        p.insert(" no-match");
        p.key(key(KeyCode::Down));
        assert_eq!(p.key(key(KeyCode::Enter)), None);
        for (w, h) in [(80, 24), (40, 16), (24, 8), (20, 5)] {
            let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
            t.draw(|f| p.draw(f, Color::Cyan, true)).unwrap();
        }
    }
}
