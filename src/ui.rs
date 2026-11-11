use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use melly::session::{Session, Speaker};
use serde_json::Value;

const BG: Color = Color::Rgb(20, 23, 30);
const PANEL: Color = Color::Rgb(27, 30, 39);
const INK: Color = Color::Rgb(235, 232, 228);
const MUTED: Color = Color::Rgb(162, 166, 177);
const BORDER: Color = Color::Rgb(62, 67, 81);
const ACCENT: Color = Color::Rgb(210, 183, 240);
const MINT: Color = Color::Rgb(164, 233, 193);

#[derive(Default)]
pub struct Workspace {
    draft: String,
    cursor: usize,
    chat_scroll: usize,
    activity_scroll: usize,
    approval_scroll: u16,
    shown_approval: Option<(Value, bool)>,
    armed_approval: Option<Value>,
    activity_focused: bool,
}

impl Workspace {
    pub fn key(&mut self, key: KeyEvent, session: &mut Session) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('q') => return true,
                KeyCode::Char('c') => return !session.interrupt(),
                _ => {}
            }
        }
        if let Some(approval) = session.approvals.front() {
            if key.modifiers != KeyModifiers::NONE || key.kind != KeyEventKind::Press {
                return false;
            }
            let Some((shown_id, confirm_visible)) = &self.shown_approval else {
                return false;
            };
            if *shown_id != approval.id {
                return false;
            }
            let accept = match key.code {
                KeyCode::Char('y') if approval.can_accept => {
                    self.armed_approval = Some(approval.id.clone());
                    return false;
                }
                KeyCode::Enter if *confirm_visible => Some(true),
                KeyCode::Char('n') | KeyCode::Esc => Some(false),
                KeyCode::Up => {
                    self.approval_scroll = self.approval_scroll.saturating_sub(1);
                    None
                }
                KeyCode::Down => {
                    self.approval_scroll = self.approval_scroll.saturating_add(1);
                    None
                }
                _ => None,
            };
            if let Some(accept) = accept {
                session.approve(accept);
                self.shown_approval = None;
                self.armed_approval = None;
                self.approval_scroll = 0;
            }
            return false;
        }

        let scroll = if self.activity_focused {
            &mut self.activity_scroll
        } else {
            &mut self.chat_scroll
        };
        match key.code {
            KeyCode::Tab => self.activity_focused = !self.activity_focused,
            KeyCode::Esc => self.activity_focused = false,
            KeyCode::PageUp => {
                *scroll = scroll.saturating_add(10);
                session.load_older();
            }
            KeyCode::PageDown => *scroll = scroll.saturating_sub(10),
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => self.insert("\n"),
            KeyCode::Enter if session.submit(&self.draft) => {
                self.draft.clear();
                self.cursor = 0;
                self.chat_scroll = 0;
                self.activity_focused = false;
            }
            KeyCode::Left => {
                if let Some((index, _)) = self.draft[..self.cursor].char_indices().next_back() {
                    self.cursor = index;
                }
            }
            KeyCode::Right => {
                if let Some(ch) = self.draft[self.cursor..].chars().next() {
                    self.cursor += ch.len_utf8();
                }
            }
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.draft.len(),
            KeyCode::Backspace => {
                if let Some((index, _)) = self.draft[..self.cursor].char_indices().next_back() {
                    self.draft.drain(index..self.cursor);
                    self.cursor = index;
                }
            }
            KeyCode::Delete => {
                if let Some(ch) = self.draft[self.cursor..].chars().next() {
                    self.draft.drain(self.cursor..self.cursor + ch.len_utf8());
                }
            }
            KeyCode::Char(ch)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                let mut bytes = [0; 4];
                self.insert(ch.encode_utf8(&mut bytes));
            }
            _ => {}
        }
        false
    }

    pub fn insert(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let text = printable(&text);
        let max = 48 * 1024 - self.draft.len();
        let text = &text[..text.floor_char_boundary(max.min(text.len()))];
        self.draft.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    pub fn draw(&mut self, frame: &mut Frame, session: &Session) {
        self.shown_approval = None;
        let area = frame.area();
        frame.render_widget(
            Block::default().style(Style::default().bg(BG).fg(INK)),
            area,
        );
        if area.height < 8 || area.width < 24 {
            frame.render_widget(Paragraph::new("Resize terminal · Ctrl+Q to quit"), area);
            return;
        }

        let [header, content, footer] = Layout::vertical([
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .areas(area);
        let cwd = std::path::Path::new(&session.cwd)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&session.cwd);
        let model = session.model.as_deref().unwrap_or("Codex");
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  melly  ", accent()),
                Span::styled(printable(cwd), dim()),
                Span::styled("  /  ", dim()),
                Span::raw(printable(model)),
                Span::styled(
                    format!("   ● {}", session.status),
                    Style::default().fg(MINT),
                ),
            ]))
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(border()),
            ),
            header,
        );

        let wide = area.width >= 94;
        if wide {
            let [chat, activity] =
                Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
                    .areas(content);
            self.chat(frame, chat, session);
            self.activity(frame, activity, session);
        } else if self.activity_focused {
            self.activity(frame, content, session);
        } else {
            self.chat(frame, content, session);
        }
        frame.render_widget(
            Paragraph::new(
                "  Enter Send · Tab Activity · PgUp/PgDn Scroll · Ctrl+C Interrupt · Ctrl+Q Quit",
            )
            .style(dim()),
            footer,
        );

        if let Some(approval) = session.approvals.front() {
            let confirm_visible =
                approval.can_accept && self.armed_approval.as_ref() == Some(&approval.id);
            self.shown_approval = Some((approval.id.clone(), confirm_visible));
            let width = (area.width * 4 / 5).max(20);
            let height = (area.height / 2).max(6);
            let popup = Rect::new(
                (area.width - width) / 2,
                (area.height - height) / 2,
                width,
                height,
            );
            frame.render_widget(Clear, popup);
            let block = Block::bordered()
                .title(" Approval required ")
                .title_bottom(if confirm_visible {
                    " Enter Confirm · n Decline "
                } else if approval.can_accept {
                    " y Select allow · n Decline "
                } else {
                    " n Decline "
                })
                .border_style(accent())
                .style(Style::default().bg(PANEL).fg(INK));
            let inner = block.inner(popup);
            frame.render_widget(block, popup);
            let details =
                Paragraph::new(printable(&approval.description)).wrap(Wrap { trim: false });
            let max_scroll = details
                .line_count(inner.width)
                .saturating_sub(inner.height as usize);
            self.approval_scroll = self
                .approval_scroll
                .min(max_scroll.min(u16::MAX as usize) as u16);
            frame.render_widget(details.scroll((self.approval_scroll, 0)), inner);
        } else {
            self.armed_approval = None;
            self.approval_scroll = 0;
        }
    }

    fn chat(&mut self, frame: &mut Frame, area: Rect, session: &Session) {
        let panel = Block::default()
            .title(" Lead chat ")
            .title_style(accent())
            .borders(Borders::TOP)
            .border_style(border());
        let inner = panel.inner(area);
        frame.render_widget(panel, area);
        let [history, composer] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(4)]).areas(inner);
        let mut lines = Vec::new();
        for entry in session
            .entries
            .iter()
            .filter(|entry| entry.speaker != Speaker::Tool)
        {
            if entry.text.is_empty() {
                continue;
            }
            let style = match entry.speaker {
                Speaker::Codex => accent(),
                Speaker::You => Style::default().fg(INK).add_modifier(Modifier::BOLD),
                _ => dim(),
            };
            lines.push(Line::styled(format!("  {}", entry.speaker.label()), style));
            lines.extend(
                printable(&entry.text)
                    .lines()
                    .map(|line| Line::raw(format!("  {line}"))),
            );
            lines.push(Line::default());
        }
        draw_scrollback(frame, history, lines, &mut self.chat_scroll);

        let block = Block::default()
            .title(" Message Codex ")
            .title_style(accent())
            .borders(Borders::TOP)
            .border_style(border())
            .style(Style::default().bg(PANEL));
        let inner = block.inner(composer);
        frame.render_widget(block, composer);
        let (before, after) = self.draft.split_at(self.cursor);
        let visible_draft = format!("  › {before}▌{after}");
        let through_cursor = Paragraph::new(format!("  › {before}▌")).wrap(Wrap { trim: false });
        let scroll = through_cursor
            .line_count(inner.width)
            .saturating_sub(inner.height as usize);
        frame.render_widget(
            Paragraph::new(visible_draft)
                .style(Style::default().fg(INK))
                .wrap(Wrap { trim: false })
                .scroll((scroll.min(u16::MAX as usize) as u16, 0)),
            inner,
        );
    }

    fn activity(&mut self, frame: &mut Frame, area: Rect, session: &Session) {
        let panel = Block::default()
            .title(" Activity ")
            .title_style(if self.activity_focused {
                accent()
            } else {
                dim()
            })
            .borders(Borders::TOP | Borders::LEFT)
            .border_style(border());
        let inner = panel.inner(area);
        frame.render_widget(panel, area);
        let [events, meta] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(5)]).areas(inner);
        let mut lines = Vec::new();
        for entry in session
            .entries
            .iter()
            .filter(|entry| entry.speaker == Speaker::Tool)
        {
            lines.extend(
                printable(&entry.text)
                    .lines()
                    .map(|line| Line::raw(format!("  {line}"))),
            );
            lines.push(Line::default());
        }
        draw_scrollback(frame, events, lines, &mut self.activity_scroll);

        let tokens = session.tokens.map_or("—".into(), display_tokens);
        let window = session.context_window.map_or("—".into(), display_tokens);
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(format!("  Tokens {tokens}   ·   Window {window}"), dim()),
                Line::styled(
                    format!(
                        "  Thread {}",
                        printable(session.thread_id.as_deref().unwrap_or("—"))
                    ),
                    dim(),
                ),
            ])
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(border()),
            ),
            meta,
        );
    }
}

fn draw_scrollback(frame: &mut Frame, area: Rect, lines: Vec<Line<'_>>, offset: &mut usize) {
    let paragraph = Paragraph::new(Text::from(lines))
        .style(Style::default().fg(INK))
        .wrap(Wrap { trim: false });
    let bottom = paragraph
        .line_count(area.width)
        .saturating_sub(area.height as usize);
    *offset = (*offset).min(bottom);
    let top = bottom.saturating_sub(*offset).min(u16::MAX as usize);
    frame.render_widget(paragraph.scroll((top as u16, 0)), area);
}

fn printable(text: &str) -> String {
    text.chars()
        .filter(|&ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
        .collect()
}

fn display_tokens(tokens: u64) -> String {
    if tokens < 1_000 {
        tokens.to_string()
    } else {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    }
}

fn accent() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

fn dim() -> Style {
    Style::default().fg(MUTED)
}

fn border() -> Style {
    Style::default().fg(BORDER)
}

#[cfg(test)]
mod tests {
    use super::*;
    use melly::session::Approval;
    use ratatui::{Terminal, backend::TestBackend};
    use serde_json::json;

    #[test]
    fn approval_requires_a_visible_request_and_a_separate_confirmation() {
        let mut session = Session::new("/tmp/project".into(), None, None);
        session.drain_requests();
        let mut workspace = Workspace::default();
        session.approvals.push_back(Approval {
            id: json!("request-1"),
            description: "$ git diff".into(),
            can_accept: true,
        });
        let y = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        workspace.key(y, &mut session);
        assert!(session.drain_requests().is_empty());

        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal
            .draw(|frame| workspace.draw(frame, &session))
            .unwrap();
        workspace.key(y, &mut session);
        workspace.key(enter, &mut session);
        assert!(session.drain_requests().is_empty());

        terminal
            .draw(|frame| workspace.draw(frame, &session))
            .unwrap();
        workspace.key(enter, &mut session);
        assert_eq!(
            session.drain_requests(),
            vec![json!({"id":"request-1","result":{"decision":"accept"}})]
        );
    }
}
