use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap},
};

use crate::commands::{Kind, Menu};
use crate::visual::{
    self, ACCENT, AMBER, BG, BORDER, FAINT, INK, MINT, MUTED, PANEL, SELECTED, SIDEBAR,
};
use melly::session::Entry;
use melly::session::{Session, Speaker};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub enum Action {
    Submit(String),
    Agents,
    Review,
    Collect,
    Stop,
    Compact,
    Settings,
    Sessions,
    New,
}

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
    menu_selection: usize,
    menu_dismissed: bool,
    thinking_expanded: bool,
    tools_expanded: bool,
    managed: bool,
    action: Option<Action>,
    pub agent_label: String,
    pub session_title: String,
    pub roster: Vec<Line<'static>>,
    inspector: Inspector,
    render_cache: HashMap<u64, EntryView>,
    accent_override: Option<Color>,
    reduced_motion: bool,
    intro_started: Option<Instant>,
    showing_home: bool,
}

struct EntryView {
    revision: u64,
    width: u16,
    collapsed: bool,
    expanded_tools: bool,
    lines: Vec<Line<'static>>,
    height: usize,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Inspector {
    #[default]
    Activity,
    Usage,
    Context,
    Status,
    Help,
}

impl Workspace {
    pub fn animating(&self, session: &Session) -> bool {
        !self.reduced_motion
            && (session.busy()
                || session.connecting()
                || (self.showing_home
                    && self
                        .intro_started
                        .is_some_and(|t| t.elapsed().as_millis() < visual::INTRO_MS)))
    }

    pub fn configure(&mut self, color: Color, motion: bool) {
        if self.accent_override != Some(color) {
            self.accent_override = Some(color);
            self.render_cache.clear();
        }
        self.reduced_motion = !motion;
    }

    pub fn set_details(&mut self, thinking: Option<bool>, tools: Option<bool>) {
        if let Some(expanded) = thinking {
            self.thinking_expanded = expanded;
        }
        if let Some(expanded) = tools {
            self.tools_expanded = expanded;
        }
    }

    pub fn draft(&self) -> &str {
        &self.draft
    }

    fn color(&self) -> Color {
        self.accent_override.unwrap_or(ACCENT)
    }

    fn accent(&self) -> Style {
        accent(self.color())
    }

    pub fn managed() -> Self {
        Self {
            managed: true,
            ..Self::default()
        }
    }
    pub fn take_action(&mut self) -> Option<Action> {
        self.action.take()
    }
    pub fn accept_submission(&mut self) {
        self.clear_draft();
        self.chat_scroll = 0;
        self.activity_focused = false;
    }
    pub fn synchronize(&mut self, session: &mut Session) {
        if !self.menu_dismissed
            && !session.skills_loaded
            && Menu::from_draft(&self.draft, session).is_some_and(|m| m.kind == Kind::Skills)
        {
            session.load_skills();
        }
    }

    pub fn key(&mut self, key: KeyEvent, session: &mut Session) -> bool {
        if key.kind == KeyEventKind::Release {
            return false;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('q') => return true,
                KeyCode::Char('c') if self.managed => {
                    self.action = Some(Action::Stop);
                    return false;
                }
                KeyCode::Char('c') => return !session.interrupt(),
                KeyCode::Char('t') => {
                    self.thinking_expanded = !self.thinking_expanded;
                    self.activity_focused = false;
                    return false;
                }
                KeyCode::Char('o') => {
                    self.tools_expanded = !self.tools_expanded;
                    self.activity_focused = false;
                    return false;
                }
                KeyCode::Char('b') => {
                    if !self.menu_dismissed && Menu::from_draft(&self.draft, session).is_some() {
                        return false;
                    }
                    self.activity_focused = !self.activity_focused;
                    return false;
                }
                KeyCode::Char('u') if session.approvals.is_empty() => {
                    self.clear_draft();
                    session.selected_skills.clear();
                    return false;
                }
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

        if !self.menu_dismissed
            && let Some(menu) = Menu::from_draft(&self.draft, session)
        {
            self.menu_selection = self.menu_selection.min(menu.rows.len().saturating_sub(1));
            match key.code {
                KeyCode::Down => {
                    if !menu.rows.is_empty() {
                        self.menu_selection = (self.menu_selection + 1) % menu.rows.len();
                    }
                    return false;
                }
                KeyCode::Up | KeyCode::BackTab => {
                    self.menu_selection = self
                        .menu_selection
                        .checked_sub(1)
                        .unwrap_or(menu.rows.len().saturating_sub(1));
                    return false;
                }
                KeyCode::Esc => {
                    self.menu_dismissed = true;
                    return false;
                }
                KeyCode::Enter | KeyCode::Tab => {
                    if let Some(row) = menu.rows.get(self.menu_selection) {
                        if menu.kind == Kind::Commands {
                            let command = row.value.clone();
                            if matches!(command.as_str(), "model" | "effort" | "skills")
                                || key.code == KeyCode::Tab
                            {
                                self.set_draft(format!("/{command} "));
                                if command == "skills" {
                                    session.load_skills();
                                }
                            } else {
                                self.clear_draft();
                                return self.command(&command, session);
                            }
                        } else if key.code == KeyCode::Tab {
                            let prefix = match menu.kind {
                                Kind::Models => "model",
                                Kind::Effort => "effort",
                                _ => "skills",
                            };
                            self.set_draft(format!("/{prefix} {}", row.value));
                        } else {
                            match menu.kind {
                                Kind::Models => {
                                    session.select_model(&row.value);
                                }
                                Kind::Effort => {
                                    session.select_effort(&row.value);
                                }
                                Kind::Skills => {
                                    if let Some(i) = row.skill_index {
                                        session.attach_skill(i);
                                    }
                                }
                                _ => {}
                            }
                            self.clear_draft();
                        }
                    }
                    return false;
                }
                _ => {}
            }
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
            KeyCode::Enter if self.draft.starts_with('/') && !self.menu_dismissed => {
                self.menu_dismissed = false;
                if Menu::from_draft(&self.draft, session).is_none_or(|m| m.rows.is_empty()) {
                    session.notice("Unknown command. Type / to browse commands.");
                }
            }
            KeyCode::Enter if self.managed => {
                self.action = Some(Action::Submit(self.draft.clone()));
            }
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
                    self.menu_selection = 0;
                    self.menu_dismissed = false;
                }
            }
            KeyCode::Delete => {
                if let Some(ch) = self.draft[self.cursor..].chars().next() {
                    self.draft.drain(self.cursor..self.cursor + ch.len_utf8());
                    self.menu_selection = 0;
                    self.menu_dismissed = false;
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

    fn clear_draft(&mut self) {
        self.set_draft(String::new());
    }

    fn set_draft(&mut self, text: String) {
        self.draft = text;
        self.cursor = self.draft.len();
        self.menu_selection = 0;
        self.menu_dismissed = false;
    }

    fn command(&mut self, command: &str, session: &mut Session) -> bool {
        match command {
            "quit" => return true,
            "stop" => {
                if self.managed {
                    self.action = Some(Action::Stop);
                } else if !session.interrupt() {
                    session.notice("No active turn to interrupt.");
                }
            }
            "compact" => {
                if self.managed {
                    self.action = Some(Action::Compact);
                } else if !session.compact() {
                    session
                        .notice("Compaction is available when the session is connected and idle.");
                }
            }
            "thinking" => {
                self.thinking_expanded = !self.thinking_expanded;
                self.activity_focused = false;
            }
            "tools" => {
                self.tools_expanded = !self.tools_expanded;
                self.activity_focused = false;
            }
            "agents" => self.action = Some(Action::Agents),
            "review" => self.action = Some(Action::Review),
            "settings" => self.action = Some(Action::Settings),
            "sessions" => self.action = Some(Action::Sessions),
            "new" => self.action = Some(Action::New),
            "collect" => self.action = Some(Action::Collect),
            "usage" => {
                self.inspector = Inspector::Usage;
                self.activity_focused = true;
                session.refresh_quota();
            }
            "context" => {
                self.inspector = Inspector::Context;
                self.activity_focused = true;
            }
            "status" => {
                self.inspector = Inspector::Status;
                self.activity_focused = true;
            }
            "activity" => {
                self.inspector = Inspector::Activity;
                self.tools_expanded = true;
                self.activity_focused = false;
            }
            "help" => {
                self.inspector = Inspector::Help;
                self.activity_focused = true;
            }
            _ => {}
        }
        self.activity_scroll = if self.inspector == Inspector::Activity {
            0
        } else {
            usize::MAX
        };
        false
    }

    pub fn insert(&mut self, text: &str) {
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let text = printable(&text);
        let max = 48 * 1024 - self.draft.len();
        let text = &text[..text.floor_char_boundary(max.min(text.len()))];
        self.draft.insert_str(self.cursor, text);
        self.cursor += text.len();
        self.activity_focused = false;
        self.menu_selection = 0;
        self.menu_dismissed = false;
    }

    pub fn draw(&mut self, frame: &mut Frame, session: &Session) {
        self.shown_approval = None;
        self.showing_home = false;
        let area = frame.area();
        frame.render_widget(
            Block::default().style(Style::default().bg(BG).fg(INK)),
            area,
        );
        if area.height < 8 || area.width < 24 {
            frame.render_widget(Paragraph::new("Resize terminal · Ctrl+Q to quit"), area);
            return;
        }
        if !self.activity_focused
            && matches!(self.agent_label.as_str(), "" | "lead")
            && session.approvals.is_empty()
            && !session.entries.iter().any(|e| e.speaker != Speaker::Notice)
        {
            self.home(frame, area, session);
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
        let [identity, state] = Layout::horizontal([
            Constraint::Min(12),
            Constraint::Length(30.min(area.width / 2)),
        ])
        .areas(header);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  melly", self.accent()),
                Span::styled(format!("  /  {}", printable(cwd)), dim()),
                Span::styled(
                    if self.session_title.is_empty() {
                        String::new()
                    } else {
                        format!("  /  {}", printable(&self.session_title))
                    },
                    dim(),
                ),
            ])),
            identity,
        );
        let symbol = if session.busy() || session.connecting() {
            visual::beat(visual::time_ms(), !self.reduced_motion)
        } else {
            "·"
        };
        frame.render_widget(
            Paragraph::new(format!("{symbol} {}  ", session.status))
                .style(dim())
                .alignment(ratatui::layout::Alignment::Right),
            state,
        );

        let wide = area.width >= 105;
        if wide {
            let [chat, _, activity] = Layout::horizontal([
                Constraint::Min(60),
                Constraint::Length(1),
                Constraint::Length(34),
            ])
            .areas(content);
            self.chat(frame, chat, session);
            self.activity(frame, activity, session);
        } else if self.activity_focused {
            self.activity(frame, content, session);
        } else {
            self.chat(frame, content, session);
        }
        let hints: &[(&str, &str)] = if self.managed && self.agent_label != "lead" {
            &[
                ("Ctrl+R", "brief / review"),
                ("Ctrl+N/P", "agents"),
                ("Ctrl+O", "activity"),
            ]
        } else if self.activity_focused {
            &[("Tab", "chat"), ("↑↓", "scroll"), ("/", "commands")]
        } else if session.busy() {
            &[
                ("Ctrl+C", "interrupt"),
                ("Ctrl+O", "activity"),
                ("Ctrl+G", "agents"),
            ]
        } else {
            &[
                ("/", "commands"),
                ("Ctrl+S", "sessions"),
                ("Ctrl+G", "agents"),
            ]
        };
        frame.render_widget(Paragraph::new(visual::hints(hints)), footer);

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
                .title(format!(
                    " Approval required · {} ",
                    if matches!(self.agent_label.as_str(), "" | "lead") {
                        "lead"
                    } else {
                        &self.agent_label
                    }
                ))
                .title_bottom(if confirm_visible {
                    " Enter Confirm · n Decline "
                } else if approval.can_accept {
                    " y Select allow · n Decline "
                } else {
                    " n Decline "
                })
                .border_style(self.accent())
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

    fn home(&mut self, frame: &mut Frame, area: Rect, session: &Session) {
        let width = area.width.saturating_sub(4).min(75);
        let x = area.x + (area.width - width) / 2;
        let logo_height = if area.height >= 18 { 5 } else { 0 };
        self.showing_home = logo_height > 0;
        let elapsed = self
            .intro_started
            .get_or_insert_with(Instant::now)
            .elapsed()
            .as_millis();
        let y = area.y + area.height.saturating_sub(logo_height + 10) / 2;
        if logo_height > 0 {
            visual::wordmark(
                frame,
                Rect::new(x, y, width, logo_height),
                self.color(),
                elapsed,
                !self.reduced_motion,
            );
        }
        let composer = Rect::new(
            x,
            y + logo_height + 2,
            width,
            5.min(area.bottom().saturating_sub(y + logo_height + 3)),
        );
        frame.render_widget(
            Block::default()
                .borders(Borders::LEFT)
                .border_style(self.accent())
                .style(Style::default().bg(PANEL)),
            composer,
        );
        let (before, after) = self.draft.split_at(self.cursor);
        let text = if self.draft.is_empty() {
            "Ask anything…".into()
        } else {
            format!("{before}▌{after}")
        };
        let body = Rect::new(
            composer.x + 2,
            composer.y + 1,
            composer.width.saturating_sub(4),
            composer.height.saturating_sub(3).max(1),
        );
        let scroll = Paragraph::new(before)
            .wrap(Wrap { trim: false })
            .line_count(body.width)
            .saturating_sub(body.height as usize);
        frame.render_widget(
            Paragraph::new(text)
                .style(if self.draft.is_empty() {
                    dim()
                } else {
                    Style::default().fg(INK)
                })
                .wrap(Wrap { trim: false })
                .scroll((scroll.min(u16::MAX as usize) as u16, 0)),
            body,
        );
        let metadata = Line::from(vec![
            Span::styled(
                format!(
                    "  {}  ",
                    if matches!(self.agent_label.as_str(), "" | "lead") {
                        "Conductor"
                    } else {
                        &self.agent_label
                    }
                ),
                self.accent(),
            ),
            Span::styled(
                model_name(session.model.as_deref().unwrap_or("Loading models")),
                dim(),
            ),
            Span::styled(
                format!("  {}", session.effort.as_deref().unwrap_or("")),
                dim(),
            ),
        ]);
        frame.render_widget(
            Paragraph::new(metadata),
            Rect::new(x, composer.bottom().saturating_sub(1), width, 1),
        );
        if !session.selected_skills.is_empty() && composer.height >= 5 {
            let skills = session
                .selected_skills
                .iter()
                .map(|s| format!("${}", s.name))
                .collect::<Vec<_>>()
                .join("  ");
            frame.render_widget(
                Paragraph::new(format!("  {skills}")).style(Style::default().fg(MINT)),
                Rect::new(x, composer.bottom() - 2, width, 1),
            );
        }
        if composer.bottom() < area.bottom() {
            frame.render_widget(
                Paragraph::new(visual::hints(&[("/", "commands"), ("Ctrl+S", "sessions")]))
                    .alignment(ratatui::layout::Alignment::Right),
                Rect::new(x, composer.bottom() + 1, width, 1),
            );
        }
        let cwd = std::path::Path::new(&session.cwd)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&session.cwd);
        frame.render_widget(
            Paragraph::new(format!("  {cwd}   ·   {}", session.status)).style(dim()),
            Rect::new(area.x, area.bottom() - 1, area.width, 1),
        );
        if let Some(notice) = session.entries.back() {
            frame.render_widget(
                Paragraph::new(printable(&notice.text)).style(dim()),
                Rect::new(x, area.bottom().saturating_sub(3), width, 1),
            );
        }
        if !self.menu_dismissed
            && let Some(menu) = Menu::from_draft(&self.draft, session)
        {
            self.draw_menu(
                frame,
                Rect::new(x, area.y, width, composer.y.saturating_sub(area.y)),
                menu,
                session,
            );
        }
    }

    fn chat(&mut self, frame: &mut Frame, area: Rect, session: &Session) {
        let observer = self.managed && self.agent_label != "lead";
        let draft_rows = Paragraph::new(format!("{}▌", self.draft))
            .wrap(Wrap { trim: false })
            .line_count(area.width.saturating_sub(4))
            .clamp(1, 6) as u16;
        let composer_height = if observer {
            1
        } else {
            (draft_rows + 3 + u16::from(!session.selected_skills.is_empty()))
                .min((area.height / 2).max(4))
        };
        let [history, composer] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(composer_height)]).areas(area);
        let [history, waiting] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(history);
        if session.entries.is_empty() {
            let greeting = vec![
                Line::default(),
                Line::styled("  What are we building?", self.accent()),
                Line::default(),
                Line::styled("  Your project. Your models. One workspace.", dim()),
                Line::default(),
                Line::from(vec![
                    Span::styled("  /model", self.accent()),
                    Span::styled("     choose Sol, Terra, Astra or Luna", dim()),
                ]),
                Line::from(vec![
                    Span::styled("  /effort", self.accent()),
                    Span::styled("    adjust reasoning strength", dim()),
                ]),
                Line::from(vec![
                    Span::styled("  /skills", self.accent()),
                    Span::styled("    bring in your Codex skills", dim()),
                ]),
                Line::default(),
                Line::styled(
                    "  Start typing below. Everything stays in this chat.",
                    dim(),
                ),
            ];
            frame.render_widget(Paragraph::new(greeting).wrap(Wrap { trim: false }), history);
        } else {
            render_entries(
                frame,
                history,
                session,
                EntryStyle {
                    tools: self.tools_expanded,
                    collapsed: !self.thinking_expanded,
                    accent: self.color(),
                    agent: &self.agent_label,
                },
                &mut self.chat_scroll,
                &mut self.render_cache,
            );
        }
        if waiting.height > 0 && session.busy() {
            let color = if session.status == "Thinking" {
                AMBER
            } else {
                self.color()
            };
            let mut spans = vec![Span::raw("  ")];
            spans.extend(visual::signal(
                visual::time_ms(),
                !self.reduced_motion,
                color,
            ));
            spans.push(Span::styled(
                format!("  {}", session.status),
                Style::default().fg(color),
            ));
            if let Some(elapsed) = session.elapsed() {
                spans.push(Span::styled(
                    format!("  {}s", elapsed.as_secs()),
                    Style::default().fg(FAINT),
                ));
            }
            frame.render_widget(Paragraph::new(Line::from(spans)), waiting);
        }
        if observer {
            frame.render_widget(
                Paragraph::new(format!(
                    "  Observing Luna · {}   Esc conductor   Tab context",
                    printable(&self.agent_label)
                ))
                .style(dim().bg(PANEL)),
                composer,
            );
            return;
        }
        let skill_names = session
            .selected_skills
            .iter()
            .map(|s| format!("${}", s.name))
            .collect::<Vec<_>>()
            .join("  ");
        let block = Block::default()
            .borders(Borders::LEFT)
            .border_style(if self.activity_focused {
                border()
            } else {
                self.accent()
            })
            .style(Style::default().bg(PANEL));
        let block = block.padding(Padding::new(1, 2, 0, 0));
        let mut inner = block.inner(composer);
        frame.render_widget(block, composer);
        let label = if self.menu_dismissed && self.draft.starts_with('/') {
            "Literal message"
        } else if session.busy() {
            "Steer conductor"
        } else {
            "Conductor"
        };
        let meta = Line::from(vec![
            Span::styled(label, Style::default().fg(self.color())),
            Span::styled(format!(" · {}", model_settings(session)), dim()),
        ]);
        if inner.height > 1 {
            frame.render_widget(
                Paragraph::new(meta),
                Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
            );
            inner.height -= 1;
        }
        // Leave a quiet row between editable text and model metadata.
        if inner.height > 2 {
            inner.height -= 1;
        }
        if inner.height > 1 {
            inner.y += 1;
            inner.height -= 1;
        }
        if !skill_names.is_empty() && inner.height > 1 {
            frame.render_widget(
                Paragraph::new(format!("{skill_names}  · Ctrl+U clear"))
                    .style(Style::default().fg(MINT)),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
            inner.y += 1;
            inner.height -= 1;
        }
        let (before, after) = self.draft.split_at(self.cursor);
        let visible = if self.draft.is_empty() {
            "Ask anything…".into()
        } else {
            format!("{before}▌{after}")
        };
        let through = Paragraph::new(format!("{before}▌")).wrap(Wrap { trim: false });
        let scroll = through
            .line_count(inner.width)
            .saturating_sub(inner.height as usize);
        frame.render_widget(
            Paragraph::new(visible)
                .style(if self.draft.is_empty() {
                    dim()
                } else {
                    Style::default().fg(INK)
                })
                .wrap(Wrap { trim: false })
                .scroll((scroll.min(u16::MAX as usize) as u16, 0)),
            inner,
        );
        if !self.menu_dismissed
            && let Some(menu) = Menu::from_draft(&self.draft, session)
        {
            self.draw_menu(frame, history, menu, session);
        }
    }

    fn draw_menu(&mut self, frame: &mut Frame, area: Rect, menu: Menu, session: &Session) {
        self.menu_selection = self.menu_selection.min(menu.rows.len().saturating_sub(1));
        let height = (menu.rows.len().clamp(1, 7) as u16 + 4).min(area.height);
        let popup = Rect::new(
            area.x + 1,
            area.bottom().saturating_sub(height),
            area.width.saturating_sub(2),
            height,
        );
        frame.render_widget(Clear, popup);
        let title = Line::styled(
            format!(" {} ", menu.title()),
            Style::default().fg(INK).add_modifier(Modifier::BOLD),
        );
        let block = Block::bordered()
            .title(title)
            .title_bottom(Line::styled(
                " ↑↓ select  Tab complete  Enter apply  Esc close ",
                dim(),
            ))
            .border_style(border())
            .style(Style::default().bg(PANEL).fg(INK));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        if inner.height == 0 || inner.width == 0 {
            return;
        }
        if menu.rows.is_empty() {
            frame.render_widget(
                Paragraph::new(menu.empty(session))
                    .style(dim())
                    .wrap(Wrap { trim: false }),
                inner,
            );
            return;
        }
        let capacity = inner.height.saturating_sub(2).max(1) as usize;
        let start = self.menu_selection.saturating_sub(capacity - 1);
        for (i, row) in menu.rows.iter().enumerate().skip(start).take(capacity) {
            let active = i == self.menu_selection;
            let marker = if active { "›" } else { " " };
            let current = if row.selected { "  ✓" } else { "" };
            let label = format!(" {marker} {}{current}", printable(&row.label));
            let style = if active {
                self.accent().bg(SELECTED)
            } else {
                Style::default().fg(INK)
            };
            frame.render_widget(
                Paragraph::new(label).style(style),
                Rect::new(inner.x, inner.y + (i - start) as u16, inner.width, 1),
            );
        }
        if inner.height >= 3 {
            let row = &menu.rows[self.menu_selection];
            frame.render_widget(
                Paragraph::new(printable(&row.detail)).style(dim()),
                Rect::new(
                    inner.x + 1,
                    inner.bottom() - 2,
                    inner.width.saturating_sub(2),
                    1,
                ),
            );
            let hint = match menu.kind {
                Kind::Models | Kind::Effort if session.busy() => {
                    "Applies on the next turn; the active turn keeps its settings."
                }
                Kind::Models | Kind::Effort => "Settings apply to your next message.",
                Kind::Skills => "Attach to your next message. No model call yet.",
                _ => "Type to filter commands.",
            };
            frame.render_widget(
                Paragraph::new(hint).style(dim()),
                Rect::new(
                    inner.x + 1,
                    inner.bottom() - 1,
                    inner.width.saturating_sub(2),
                    1,
                ),
            );
        }
    }

    fn activity(&mut self, frame: &mut Frame, area: Rect, session: &Session) {
        let panel = Block::default()
            .padding(Padding::new(0, 0, 1, 0))
            .style(Style::default().bg(SIDEBAR));
        let inner = panel.inner(area);
        frame.render_widget(panel, area);
        let roster_height = (self.roster.len() as u16).min(inner.height.saturating_sub(7));
        let [meta, roster, events] = Layout::vertical([
            Constraint::Length(5),
            Constraint::Length(roster_height),
            Constraint::Min(1),
        ])
        .areas(inner);
        if roster.height > 0 {
            frame.render_widget(Paragraph::new(self.roster.clone()), roster);
        }
        let used = session.context_used.map_or("—".into(), display_tokens);
        let capacity = session.context_window.map_or("—".into(), display_tokens);
        let percent = match (session.context_used, session.context_window) {
            (Some(u), Some(c)) if c > 0 => format!("≈{:.0}%", 100.0 * u as f64 / c as f64),
            _ => String::new(),
        };
        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    "  Context",
                    Style::default().fg(INK).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "{:>width$}",
                        percent,
                        width = inner.width.saturating_sub(11) as usize
                    ),
                    dim(),
                ),
            ]),
            Line::styled(format!("  {used} / {capacity}"), dim()),
        ];
        let mut meter = visual::meter(
            session.context_used,
            session.context_window,
            inner.width.saturating_sub(4).min(28) as usize,
            self.color(),
        );
        meter.spans.insert(0, Span::raw("  "));
        lines.push(meter);
        if let Some(q) = session
            .quota
            .iter()
            .find(|q| q.bucket_id == "codex")
            .or(session.quota.first())
        {
            lines.push(Line::styled(
                format!(
                    "  {} · {} · {}% left",
                    if q.bucket_id == "codex" {
                        "Codex"
                    } else {
                        &q.name
                    },
                    window_name(q.minutes),
                    100u64.saturating_sub(q.used_percent)
                ),
                Style::default().fg(MINT),
            ));
        } else {
            lines.push(Line::styled(
                if session.quota_loaded {
                    "  Account usage unavailable"
                } else {
                    "  Loading account usage…"
                },
                dim(),
            ));
        }
        frame.render_widget(Paragraph::new(lines), meta);
        let events = Rect::new(events.x, events.y, events.width, events.height);
        if self.inspector == Inspector::Activity {
            let mut lines = vec![Line::styled("  Ctrl+G  browse agents", dim())];
            if self.agent_label != "lead" && !self.agent_label.is_empty() {
                lines.push(Line::default());
                lines.push(Line::styled(
                    format!(
                        "  {} tokens used",
                        session.tokens.map_or("—".into(), display_tokens)
                    ),
                    dim(),
                ));
                lines.push(Line::styled("  Ctrl+O  activity details", dim()));
            }
            frame.render_widget(Paragraph::new(lines), events);
        } else {
            let lines = inspector_lines(self.inspector, session);
            draw_scrollback(frame, events, lines, &mut self.activity_scroll);
        }
    }
}

fn window_name(minutes: Option<u64>) -> String {
    match minutes {
        Some(m) if m >= 1440 && m % 1440 == 0 => format!("{}d window", m / 1440),
        Some(m) if m >= 60 && m % 60 == 0 => format!("{}h window", m / 60),
        Some(m) => format!("{m}m window"),
        None => "Account window".into(),
    }
}

fn inspector_lines(view: Inspector, s: &Session) -> Vec<Line<'static>> {
    let token = |value: Option<u64>| value.map_or("Unavailable".into(), |v| v.to_string());
    let mut rows:Vec<String>=match view {
        Inspector::Context=>vec!["CONTEXT WINDOW".into(),String::new(),
            format!("Last model call  {} tokens",token(s.context_used)),format!("Capacity         {} tokens",token(s.context_window)),String::new(),
            "The meter estimates occupancy from Codex's latest model-call token snapshot. It updates when Codex reports usage.".into(),String::new(),
            "Total session tokens accumulate across calls and are not context occupancy.".into(),String::new(),
            "/compact  compact the selected conversation when idle. No extra summary chat is sent.".into()],
        Inspector::Usage=>vec!["SESSION TOKENS".into(),String::new(),format!("Total      {}",token(s.tokens)),
            format!("Input      {}",token(s.input_tokens)),format!("Cached     {} (part of input)",token(s.cached_tokens)),
            format!("Output     {}",token(s.output_tokens)),format!("Reasoning  {} (part of output)",token(s.reasoning_tokens)),String::new(),
            "ACCOUNT LIMITS".into(),String::new()],
        Inspector::Status=>vec!["SESSION".into(),String::new(),format!("Model   {}",s.model.as_deref().unwrap_or("Unknown")),
            format!("Effort  {}",s.effort.as_deref().unwrap_or("Codex default")),format!("Login   {}",s.account.as_deref().unwrap_or("Unknown")),
            format!("State   {}",s.status),String::new(),"TIMING".into(),
            format!("Startup      {}",s.startup_duration.map_or("Connecting…".into(),|d|format!("{:.2}s",d.as_secs_f32()))),
            format!("First event  {}",s.first_event_duration.map_or("No event yet".into(),|d|format!("{:.2}s",d.as_secs_f32()))),
            format!("Turn elapsed {}",s.elapsed().map_or("No turn yet".into(),|d|format!("{:.2}s",d.as_secs_f32()))),String::new(),
            "DIRECTORY".into(),s.cwd.clone(),String::new(),"RESUME THIS CHAT".into(),
            format!("melly --resume {}",s.thread_id.as_deref().unwrap_or("…"))],
        Inspector::Help=>vec!["KEYBOARD".into(),String::new(),"/           Commands and search".into(),
            "↑ ↓         Select a menu item".into(),"Tab         Complete menu selection".into(),"Enter       Apply choice / send".into(),
            "Esc         Close menu / return to chat".into(),"Tab, Ctrl+B Switch pane".into(),"Ctrl+T      Toggle thinking summaries".into(),
            "Ctrl+O      Toggle full tool output".into(),"Alt+1…9     Jump to an agent".into(),"Ctrl+N/P    Next / previous agent".into(),
            "Ctrl+S      Search and resume saved sessions".into(),
            "Ctrl+G      Browse agents".into(),
            "Ctrl+R      Worker brief and review".into(),
            "Ctrl+U      Clear draft and skills".into(),"Shift+Enter Newline (terminal dependent)".into(),
            "PgUp/PgDn   Scroll focused pane".into(),"Ctrl+C      Interrupt / quit if idle".into(),"Ctrl+Q      Quit".into(),String::new(),"COMMANDS".into()],
        _=>Vec::new(),
    };
    if view == Inspector::Usage {
        if s.quota.is_empty() {
            rows.push("Account usage unavailable from Codex.".into());
        }
        for q in &s.quota {
            rows.push(format!("{} · {}", q.name, window_name(q.minutes)));
            rows.push(format!(
                "{}% left ({}% used)",
                100u64.saturating_sub(q.used_percent),
                q.used_percent
            ));
            if let Some(reset) = q.resets_at {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let mins = reset.saturating_sub(now).div_ceil(60);
                rows.push(format!("Reset in {}h {}m", mins / 60, mins % 60));
            }
            rows.push(String::new());
        }
        rows.push("Quota comes from your account. Token counts do not translate into a per-model plan percentage.".into());
    }
    if view == Inspector::Help {
        rows.extend(
            crate::commands::COMMANDS
                .iter()
                .map(|(c, d)| format!("/{c}  {d}")),
        );
    }
    rows.into_iter()
        .map(|s| Line::raw(format!("  {}", printable(&s))))
        .collect()
}

struct EntryStyle<'a> {
    tools: bool,
    collapsed: bool,
    accent: Color,
    agent: &'a str,
}

fn render_entries(
    frame: &mut Frame,
    area: Rect,
    session: &Session,
    style: EntryStyle<'_>,
    offset: &mut usize,
    cache: &mut HashMap<u64, EntryView>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if cache.len() > 450 {
        cache.retain(|key, _| session.entries.iter().any(|e| e.key == *key));
    }
    // Consecutive reasoning, progress and tools form a single disclosure. Keep
    // references, not copies of tool output, so drawing a long history stays cheap.
    let mut groups: Vec<Vec<&Entry>> = Vec::new();
    for entry in &session.entries {
        if is_activity(entry) && groups.last().is_some_and(|g| is_activity(g[0])) {
            groups.last_mut().unwrap().push(entry);
        } else {
            groups.push(vec![entry]);
        }
    }
    let mut total = 0;
    for group in &groups {
        let entry = group[0];
        let revision = group.iter().fold(0u64, |hash, e| {
            hash.wrapping_mul(31)
                .wrapping_add(e.key)
                .wrapping_mul(31)
                .wrapping_add(e.revision)
        });
        if !cache.get(&entry.key).is_some_and(|v| {
            v.revision == revision
                && v.width == area.width
                && v.collapsed == style.collapsed
                && v.expanded_tools == style.tools
        }) {
            let mut lines = if is_activity(entry) {
                activity_lines(group, &style, area.width)
            } else {
                entry_lines(entry, &style)
            };
            for line in &mut lines {
                strip_gutter(line);
            }
            let card = entry.speaker == Speaker::You;
            let height = Paragraph::new(lines.clone())
                .wrap(Wrap { trim: false })
                .line_count(area.width.saturating_sub(4))
                + usize::from(card);
            cache.insert(
                entry.key,
                EntryView {
                    revision,
                    width: area.width,
                    collapsed: style.collapsed,
                    expanded_tools: style.tools,
                    lines,
                    height,
                },
            );
        }
        total += cache[&entry.key].height;
    }
    let bottom = total.saturating_sub(area.height as usize);
    *offset = (*offset).min(bottom);
    let top = bottom.saturating_sub(*offset);
    let mut position = 0usize;
    for group in groups {
        let entry = group[0];
        let view = &cache[&entry.key];
        let end = position + view.height;
        if end > top && position < top + area.height as usize {
            let skip = top.saturating_sub(position);
            let y = position.saturating_sub(top);
            let height = (view.height - skip).min(area.height as usize - y);
            let rect = Rect::new(area.x, area.y + y as u16, area.width, height as u16);
            let card = entry.speaker == Speaker::You;
            let body_height = (view.height.saturating_sub(usize::from(card) + skip)).min(height);
            if body_height > 0 {
                let rect = Rect {
                    height: body_height as u16,
                    ..rect
                };
                let mut paragraph = Paragraph::new(view.lines.clone())
                    .style(Style::default().fg(INK))
                    .wrap(Wrap { trim: false })
                    .scroll((skip.min(u16::MAX as usize) as u16, 0))
                    .block(Block::new().padding(Padding::horizontal(2)));
                if card {
                    paragraph = paragraph.block(
                        Block::new()
                            .borders(Borders::LEFT)
                            .padding(Padding::new(1, 2, 0, 0))
                            .border_style(Style::default().fg(style.accent))
                            .style(Style::default().bg(PANEL)),
                    );
                }
                frame.render_widget(paragraph, rect);
            }
        }
        position = end;
        if position >= top + area.height as usize {
            break;
        }
    }
}

/// Real padding keeps wrapped text aligned; preserve the formatter's relative indents.
fn strip_gutter(line: &mut Line<'static>) {
    let mut remaining = 2;
    for span in &mut line.spans {
        let count = span
            .content
            .chars()
            .take(remaining)
            .take_while(|c| *c == ' ')
            .count();
        if count > 0 {
            span.content = span.content[count..].to_owned().into();
            remaining -= count;
        }
        if remaining == 0 || !span.content.is_empty() {
            break;
        }
    }
}

fn is_activity(entry: &Entry) -> bool {
    matches!(entry.speaker, Speaker::Thinking | Speaker::Tool)
        || (entry.speaker == Speaker::Codex && entry.outcome.as_deref() == Some("commentary"))
}

fn activity_lines(entries: &[&Entry], style: &EntryStyle<'_>, width: u16) -> Vec<Line<'static>> {
    let thinking = entries.iter().any(|e| e.speaker == Speaker::Thinking);
    let tools = entries
        .iter()
        .filter(|e| e.speaker == Speaker::Tool)
        .count();
    let running = entries.iter().any(|e| !e.complete);
    let failed = entries.iter().any(|e| {
        matches!(
            e.outcome.as_deref(),
            Some("failed" | "declined" | "interrupted")
        )
    });
    let expanded = !style.collapsed || style.tools;
    let name = if thinking {
        if running { "Thinking" } else { "Thought" }
    } else {
        "Activity"
    };
    let count = if tools > 0 {
        format!(" · {tools} tool{}", if tools == 1 { "" } else { "s" })
    } else {
        String::new()
    };
    let mut lines = vec![Line::styled(
        format!(
            "  {} {name}{count}{}",
            if expanded { "−" } else { "+" },
            if failed { " · failed" } else { "" }
        ),
        Style::default().fg(if thinking || failed { AMBER } else { MUTED }),
    )];
    if expanded {
        lines.push(Line::default());
        for entry in entries {
            match entry.speaker {
                Speaker::Thinking if !style.collapsed => {
                    lines.extend(
                        printable(&entry.text)
                            .lines()
                            .map(|l| markdown_line(l, style.accent).style(dim())),
                    );
                    lines.push(Line::default());
                }
                Speaker::Tool if style.tools => {
                    let details = EntryStyle {
                        tools: true,
                        collapsed: false,
                        accent: style.accent,
                        agent: style.agent,
                    };
                    lines.extend(entry_lines(entry, &details));
                }
                Speaker::Codex => {
                    lines.extend(
                        printable(&entry.text)
                            .lines()
                            .map(|l| markdown_line(l, style.accent).style(dim())),
                    );
                    lines.push(Line::default());
                }
                _ => {}
            }
        }
    } else if let Some(entry) = entries.iter().rev().find(|e| {
        !e.complete
            || matches!(
                e.outcome.as_deref(),
                Some("failed" | "declined" | "interrupted")
            )
    }) {
        let label = tool_label(entry);
        lines.push(Line::styled(
            format!("    {}", ellipsis(&label, width.saturating_sub(5) as usize)),
            dim(),
        ));
    }
    lines.push(Line::default());
    lines
}

pub(crate) fn ellipsis(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    text.chars()
        .take(width - 1)
        .chain(std::iter::once('…'))
        .collect()
}

fn tool_label(entry: &Entry) -> String {
    if entry.speaker == Speaker::Thinking {
        return "Reasoning…".into();
    }
    if entry.speaker == Speaker::Codex {
        return "Working…".into();
    }
    let text = printable(&entry.text);
    let first = text.lines().next().unwrap_or("Working");
    if first.contains("SKILL.md") {
        "Reading skill".into()
    } else if first.contains("melly_agents") {
        "Coordinating Luna".into()
    } else if first.contains("test") {
        "Running checks".into()
    } else if first.starts_with('$') {
        "Running command".into()
    } else {
        first.to_owned()
    }
}

fn entry_lines(entry: &Entry, style: &EntryStyle<'_>) -> Vec<Line<'static>> {
    let collapsed = style.collapsed;
    let text = printable(&entry.text);
    if entry.speaker == Speaker::Thinking {
        if text.is_empty() && entry.complete {
            return Vec::new();
        }
        let mut lines = vec![Line::styled(
            format!(
                "  {} {}",
                if collapsed { "+" } else { "−" },
                if entry.complete {
                    "Thought"
                } else {
                    "Thinking"
                }
            ),
            Style::default().fg(AMBER),
        )];
        if !collapsed {
            lines.push(Line::default());
            lines.extend(
                text.lines()
                    .map(|l| markdown_line(l, style.accent).style(dim())),
            );
        }
        lines.push(Line::default());
        return lines;
    }
    if entry.speaker == Speaker::Tool && collapsed {
        let first = text.lines().next().unwrap_or("Tool call");
        let label = if first.contains("SKILL.md") {
            let path = first
                .split_whitespace()
                .find(|word| word.contains("SKILL.md"))
                .unwrap_or("SKILL.md");
            let skill = std::path::Path::new(path)
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|p| p.to_str())
                .unwrap_or("skill");
            format!("Read skill · {skill}")
        } else {
            first.chars().take(100).collect()
        };
        let failed = matches!(
            entry.outcome.as_deref(),
            Some("failed" | "declined" | "interrupted")
        );
        let status = if failed {
            "✕ FAILED"
        } else if entry.complete {
            "✓"
        } else {
            "◌"
        };
        return vec![
            Line::styled(
                format!("  {status} {label}"),
                Style::default().fg(if failed { AMBER } else { MUTED }),
            ),
            Line::default(),
        ];
    }
    if entry.speaker == Speaker::You {
        let mut lines = vec![Line::default()];
        lines.extend(
            text.lines()
                .map(|line| Line::styled(format!("  {line}"), Style::default().fg(INK).bg(PANEL))),
        );
        lines.push(Line::default());
        return lines;
    }
    let mut lines = Vec::new();
    if entry.speaker == Speaker::Tool {
        let title = match (entry.complete, entry.outcome.as_deref()) {
            (false, _) => "● RUNNING",
            (true, Some("failed")) => "✕ FAILED",
            (true, Some("declined")) => "⊘ DECLINED",
            (true, Some("interrupted")) => "■ INTERRUPTED",
            _ => "✓ COMPLETED",
        };
        let color = if matches!(
            entry.outcome.as_deref(),
            Some("failed" | "declined" | "interrupted")
        ) {
            AMBER
        } else {
            MINT
        };
        lines.push(Line::styled(
            format!("  {title}"),
            Style::default().fg(color),
        ));
        lines.extend(text.lines().map(|line| Line::raw(format!("  {line}"))));
    } else if entry.speaker == Speaker::Notice {
        lines.extend(
            text.lines()
                .map(|line| Line::styled(format!("  {line}"), dim())),
        );
    } else {
        let mut code = false;
        let commentary = entry.outcome.as_deref() == Some("commentary");
        for line in text.lines() {
            if line.starts_with("```") {
                code = !code;
                lines.push(Line::styled(
                    if code {
                        format!("  ┄ {}", line.trim_start_matches('`'))
                    } else {
                        "  ┄".into()
                    },
                    dim(),
                ));
            } else if code {
                lines.push(Line::styled(
                    format!("  {line}"),
                    Style::default().fg(MINT).bg(PANEL),
                ));
            } else if line.starts_with("# ") || line.starts_with("## ") || line.starts_with("### ")
            {
                lines.push(Line::styled(
                    format!("  {}", line.trim_start_matches('#').trim_start()),
                    accent(style.accent),
                ));
            } else {
                let line = markdown_line(line, style.accent);
                lines.push(if commentary { line.style(dim()) } else { line });
            }
        }
        if entry.complete && !text.is_empty() && !commentary {
            let agent = if style.agent.is_empty() || style.agent == "lead" {
                "Conductor".into()
            } else {
                printable(style.agent)
            };
            let mut footer = vec![
                Span::raw("  "),
                Span::styled(agent, Style::default().fg(style.accent)),
            ];
            if commentary {
                footer.push(Span::styled(" · progress", dim()));
            }
            if let Some(model) = entry.model.as_deref() {
                footer.push(Span::styled(format!(" · {}", model_name(model)), dim()));
            }
            lines.push(Line::default());
            lines.push(Line::from(footer));
        }
    }
    lines.push(Line::default());
    lines
}

pub(crate) fn model_name(id: &str) -> String {
    match id {
        "gpt-6-astra" => "Astra".into(),
        "gpt-6-luna" => "Luna".into(),
        "gpt-6.1-sol" | "gpt-6-sol" => "Sol".into(),
        "gpt-6.1-terra" => "Terra".into(),
        _ => printable(id),
    }
}

fn model_settings(session: &Session) -> String {
    let active = session.busy() && session.active_model.is_some();
    let model = if active {
        &session.active_model
    } else {
        &session.model
    };
    let effort = if active {
        &session.active_effort
    } else {
        &session.effort
    };
    let mut text = format!(
        "{} · {}",
        model_name(model.as_deref().unwrap_or("Codex")),
        effort.as_deref().unwrap_or("default")
    );
    if active && (session.active_model != session.model || session.active_effort != session.effort)
    {
        text.push_str(&format!(
            " → next {} · {}",
            model_name(session.model.as_deref().unwrap_or("Codex")),
            session.effort.as_deref().unwrap_or("default")
        ));
    }
    text
}

fn markdown_line(text: &str, accent_color: Color) -> Line<'static> {
    let mut spans = vec![Span::raw("  ")];
    let mut rest = text;
    while let Some(start) = rest.find(['`', '*']) {
        let delimiter = if rest[start..].starts_with("**") {
            "**"
        } else if rest[start..].starts_with('`') {
            "`"
        } else {
            spans.push(Span::raw(rest[..start + 1].to_owned()));
            rest = &rest[start + 1..];
            continue;
        };
        let after = &rest[start + delimiter.len()..];
        if let Some(end) = after.find(delimiter) {
            spans.push(Span::raw(rest[..start].to_owned()));
            let style = if delimiter == "`" {
                Style::default().fg(accent_color).bg(PANEL)
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            spans.push(Span::styled(after[..end].to_owned(), style));
            rest = &after[end + delimiter.len()..];
        } else {
            break;
        }
    }
    spans.push(Span::raw(rest.to_owned()));
    Line::from(spans)
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

fn accent(color: Color) -> Style {
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn dim() -> Style {
    Style::default().fg(MUTED)
}

fn border() -> Style {
    Style::default().fg(BORDER)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use melly::session::Approval;
    use ratatui::{Terminal, backend::TestBackend};
    use serde_json::json;

    pub(crate) fn fixture() -> Session {
        let mut s = Session::new("/workspace/melly".into(), None, None);
        s.drain_requests();
        s.thread_id = Some("preview-thread".into());
        s.models_loaded = true;
        s.status = "Ready".into();
        s.models=[("foo-pro","medium"),("foo","low"),("gpt-6-astra","medium")].iter().map(|(model,effort)|
            melly::catalog::Model::parse(&json!({"model":model,"displayName":model,"defaultReasoningEffort":effort,
                "description":"Available through your Codex account","supportedReasoningEfforts":[
                    {"reasoningEffort":"low","description":"Quick tasks and small changes"},
                    {"reasoningEffort":"medium","description":"Balanced reasoning"},
                    {"reasoningEffort":"high","description":"More reasoning for difficult problems"}]})).unwrap()).collect();
        s.select_model("gpt-6-astra");
        s
    }

    #[test]
    fn home_centers_the_composer_without_an_empty_inspector() {
        let s = fixture();
        let mut w = Workspace::default();
        let mut t = Terminal::new(TestBackend::new(120, 40)).unwrap();
        t.draw(|f| w.draw(f, &s)).unwrap();
        let text = screen(&t);
        let row = text
            .lines()
            .position(|l| l.contains("Ask anything"))
            .expect("centered composer");
        assert!((14..30).contains(&row));
        assert!(!text.contains("LIVE ACTIVITY"));
        assert!(!text.contains("Latest model-call snapshot"));
        assert!(w.animating(&s));
        if std::env::var_os("MELLY_UI_CAPTURE_DIR").is_some() {
            for ms in (0..=600).step_by(50) {
                w.intro_started = Some(Instant::now() - std::time::Duration::from_millis(ms));
                t.draw(|f| w.draw(f, &s)).unwrap();
                capture(&t, &format!("intro-{ms:03}"));
            }
        }
        w.intro_started =
            Some(Instant::now() - std::time::Duration::from_millis(visual::INTRO_MS as u64));
        assert!(!w.animating(&s));
        t.draw(|f| w.draw(f, &s)).unwrap();
        capture(&t, "home");
        w.configure(ACCENT, false);
        w.intro_started = Some(Instant::now());
        assert!(!w.animating(&s));
    }

    #[test]
    fn wrapped_messages_and_composer_share_the_same_text_gutter() {
        let mut s = fixture();
        s.submit(&"promptmark ".repeat(32));
        s.receive(json!({"method":"item/completed","params":{"threadId":"preview-thread","item":{
            "type":"agentMessage","id":"reply","phase":"final_answer","text":"replymark ".repeat(32)}}}));
        let mut w = Workspace::default();
        w.insert(&"draftmark ".repeat(16));
        let mut t = Terminal::new(TestBackend::new(80, 34)).unwrap();
        t.draw(|f| w.draw(f, &s)).unwrap();
        let text = screen(&t);
        for marker in ["promptmark", "replymark", "draftmark"] {
            let rows: Vec<_> = text.lines().filter(|line| line.contains(marker)).collect();
            assert!(rows.len() >= 2, "expected wrapping for {marker}");
            for row in rows {
                assert_eq!(
                    row.chars()
                        .position(|c| c == marker.chars().next().unwrap()),
                    Some(2),
                    "misaligned {marker}: {row}"
                );
            }
        }
    }

    #[test]
    fn active_turn_metadata_and_narrow_activity_controls_stay_truthful() {
        let mut s = fixture();
        s.submit("Check the parser");
        s.drain_requests();
        s.receive(json!({"method":"turn/started","params":{"threadId":"preview-thread","turn":{"id":"t"}}}));
        s.active_model = Some("gpt-6-astra".into());
        s.active_effort = Some("medium".into());
        s.model = Some("gpt-6.1-sol".into());
        s.effort = Some("low".into());
        s.receive(json!({"method":"item/completed","params":{"threadId":"preview-thread","item":{
            "id":"tool","type":"commandExecution","command":"cargo test parser","aggregatedOutput":"PARSER_CHECK_PASSED","exitCode":0,"status":"completed"}}}));
        let mut w = Workspace::default();
        let mut t = Terminal::new(TestBackend::new(100, 30)).unwrap();
        t.draw(|f| w.draw(f, &s)).unwrap();
        let text = screen(&t);
        assert!(text.contains("Steer conductor · Astra · medium → next Sol · low"));
        press(&mut w, &mut s, KeyCode::Tab);
        w.key(
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
            &mut s,
        );
        t.draw(|f| w.draw(f, &s)).unwrap();
        assert!(screen(&t).contains("PARSER_CHECK_PASSED"));
    }

    #[test]
    fn conversation_groups_activity_and_renders_thinking_markdown() {
        let mut s = fixture();
        s.submit("Fix the return transition.");
        s.drain_requests();
        for item in [
            json!({"type":"agentMessage","id":"progress","phase":"commentary","text":"Checking the navigation path."}),
            json!({"type":"reasoning","id":"reason","summary":["**Checking the viewport**"]}),
            json!({"type":"commandExecution","id":"command","command":"cat assets/home/styles.css","aggregatedOutput":"RAW_SHELL_OUTPUT","status":"completed","exitCode":0}),
            json!({"type":"agentMessage","id":"final","phase":"final_answer","text":"Fixed the transition. The checks pass."}),
        ] {
            s.receive(json!({"method":"item/completed","params":{"threadId":"preview-thread","item":item}}));
        }
        let mut w = Workspace::default();
        let mut t = Terminal::new(TestBackend::new(130, 34)).unwrap();
        t.draw(|f| w.draw(f, &s)).unwrap();
        let text = screen(&t);
        assert!(!text.contains("Checking the navigation path."));
        assert!(!text.contains("cat assets/home/styles.css"));
        assert!(!text.contains("RAW_SHELL_OUTPUT"));
        assert_eq!(text.matches("+ Thought").count(), 1);
        assert!(text.contains("Fixed the transition"));
        assert!(!text.contains("· progress"));
        w.key(
            KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL),
            &mut s,
        );
        w.key(
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
            &mut s,
        );
        t.draw(|f| w.draw(f, &s)).unwrap();
        let text = screen(&t);
        assert!(text.contains("Checking the viewport"));
        assert!(!text.contains("**Checking the viewport**"));
        assert!(text.contains("RAW_SHELL_OUTPUT"));
        capture(&t, "expanded");
    }

    #[test]
    fn tools_are_compact_and_thinking_is_a_disclosure_by_default() {
        let mut s = fixture();
        let mut w = Workspace::default();
        s.receive(json!({"method":"item/completed","params":{"threadId":"preview-thread","item":{
            "type":"reasoning","id":"r","summary":["Checking the caller","Second paragraph should be collapsed"]}}}));
        s.receive(json!({"method":"item/completed","params":{"threadId":"preview-thread","item":{
            "type":"commandExecution","id":"c","command":"cat /tmp/SKILL.md","aggregatedOutput":"INSTRUCTION FILE BODY","exitCode":0,"status":"completed"}}}));
        let mut t = Terminal::new(TestBackend::new(130, 34)).unwrap();
        t.draw(|f| w.draw(f, &s)).unwrap();
        let text = screen(&t);
        assert!(text.contains("+ Thought"));
        assert!(!text.contains("Second paragraph"));
        assert!(!text.contains("INSTRUCTION FILE BODY"));
        w.key(
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
            &mut s,
        );
        t.draw(|f| w.draw(f, &s)).unwrap();
        assert!(screen(&t).contains("INSTRUCTION FILE BODY"));
    }

    fn press(w: &mut Workspace, s: &mut Session, key: KeyCode) {
        w.key(KeyEvent::new(key, KeyModifiers::NONE), s);
    }

    pub(crate) fn screen(t: &Terminal<TestBackend>) -> String {
        let b = t.backend().buffer();
        (0..b.area.height)
            .map(|y| {
                (0..b.area.width)
                    .map(|x| b[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn thinking_and_palette_render_at_multiple_terminal_sizes() {
        let mut s = fixture();
        s.submit("Add a cache to model discovery and cover invalidation with tests.");
        s.drain_requests();
        s.receive(json!({"method":"turn/started","params":{"threadId":"preview-thread","turn":{"id":"t"}}}));
        s.receive(json!({"method":"item/completed","params":{"threadId":"preview-thread","item":{
            "type":"reasoning","id":"thought","summary":["Checking the cache boundary","I will reuse the existing catalog, invalidate it on a configuration change, and keep model selection responsive."]}}}));
        s.receive(json!({"method":"item/completed","params":{"threadId":"preview-thread","item":{
            "type":"commandExecution","id":"tool","command":"cargo test catalog","aggregatedOutput":"running 4 tests\ntest cache_hit ... ok\ntest refresh ... ok\ntest invalidation ... ok\ntest fallback ... ok\n\n4 passed; 0 failed","status":"completed","exitCode":0}}}));
        s.receive(json!({"method":"item/agentMessage/delta","params":{"threadId":"preview-thread","itemId":"a","delta":"The catalog now stays **cached between requests**.\n\n- Added invalidation when configuration changes.\n- Kept the picker available while refresh runs.\n- All four catalog tests pass."}}));
        s.receive(json!({"method":"thread/tokenUsage/updated","params":{"threadId":"preview-thread","tokenUsage":{
            "last":{"totalTokens":18400},"total":{"totalTokens":42600},"modelContextWindow":200000}}}));
        s.receive(json!({"method":"account/rateLimits/updated","params":{"rateLimits":{"limitId":"codex","primary":{"usedPercent":14,"windowDurationMins":300}}}}));
        for (width, height) in [(130, 34), (80, 24), (40, 16), (24, 8), (20, 5)] {
            let mut w = Workspace::default();
            let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
            t.draw(|f| w.draw(f, &s)).unwrap();
            if width == 130 {
                assert!(screen(&t).contains("Thought"));
                assert!(screen(&t).contains("catalog tests pass"));
                capture(&t, "conversation");
            }
            w.insert("/model ");
            t.draw(|f| w.draw(f, &s)).unwrap();
            if width == 130 {
                assert!(screen(&t).contains("Choose model"));
                capture(&t, "palette");
            }
        }
    }

    pub(crate) fn capture(t: &Terminal<TestBackend>, name: &str) {
        let Ok(dir) = std::env::var("MELLY_UI_CAPTURE_DIR") else {
            return;
        };
        let b = t.backend().buffer();
        let rgb = |c: Color| match c {
            Color::Rgb(r, g, b) => vec![r, g, b],
            _ => vec![235, 232, 228],
        };
        let rows:Vec<_>=(0..b.area.height).map(|y|(0..b.area.width).map(|x|{
            let c=&b[(x,y)];json!({"text":c.symbol(),"fg":rgb(c.fg),"bg":rgb(c.bg),"bold":c.modifier.contains(Modifier::BOLD)})
        }).collect::<Vec<_>>()).collect();
        std::fs::write(
            std::path::Path::new(&dir).join(format!("{name}.json")),
            serde_json::to_vec(&rows).unwrap(),
        )
        .unwrap();
    }

    #[test]
    #[ignore = "manual release render benchmark"]
    fn cached_scrollback_render_timing() {
        let mut s = fixture();
        for i in 0..400 {
            s.notice(format!(
                "Message {i}\n{}",
                "A representative long line of terminal output and context.\n".repeat(50)
            ));
        }
        let mut w = Workspace::default();
        let mut t = Terminal::new(TestBackend::new(130, 34)).unwrap();
        t.draw(|f| w.draw(f, &s)).unwrap();
        let start = std::time::Instant::now();
        for _ in 0..200 {
            t.draw(|f| w.draw(f, &s)).unwrap();
        }
        println!(
            "Cached frame: {:.3} ms (400 entries, 20,000 source lines)",
            start.elapsed().as_secs_f64() * 1000.0 / 200.0
        );
    }

    #[test]
    fn tab_completion_preserves_an_exact_model_among_prefix_matches() {
        let mut s = fixture();
        let mut w = Workspace::default();
        w.insert("/model foo");
        press(&mut w, &mut s, KeyCode::Tab);
        press(&mut w, &mut s, KeyCode::Enter);
        assert_eq!(s.model.as_deref(), Some("foo"));
        assert_eq!(s.effort.as_deref(), Some("low"));
        w.insert("/strength high");
        press(&mut w, &mut s, KeyCode::Enter);
        assert_eq!(s.effort.as_deref(), Some("high"));
        assert!(w.draft.is_empty());
    }

    #[test]
    fn escape_allows_a_literal_slash_prompt() {
        let mut s = fixture();
        let mut w = Workspace::default();
        w.insert("/Users/kai/project has a failing test");
        press(&mut w, &mut s, KeyCode::Esc);
        press(&mut w, &mut s, KeyCode::Enter);
        assert!(
            s.drain_requests()
                .iter()
                .any(|r| r["method"] == "turn/start")
        );
        assert!(w.draft.is_empty());
    }

    #[test]
    fn a_narrow_inspector_reveals_the_composer_when_typing_a_command() {
        let mut s = fixture();
        let mut w = Workspace {
            activity_focused: true,
            ..Workspace::default()
        };
        w.insert("/model ");
        w.key(
            KeyEvent::new(KeyCode::Char('b'), KeyModifiers::CONTROL),
            &mut s,
        );
        let mut t = Terminal::new(TestBackend::new(70, 28)).unwrap();
        t.draw(|f| w.draw(f, &s)).unwrap();
        assert!(screen(&t).contains("Choose model"));
        assert!(screen(&t).contains("/model"));
        press(&mut w, &mut s, KeyCode::Enter);
    }

    #[test]
    fn failed_shell_commands_keep_literal_syntax_visible() {
        let mut s = fixture();
        let mut w = Workspace::default();
        s.receive(json!({"method":"item/completed","params":{"threadId":"preview-thread","item":{
            "id":"cmd","type":"commandExecution","command":"echo `date` **/*.rs","exitCode":1,"status":"failed"}}}));
        let mut t = Terminal::new(TestBackend::new(130, 32)).unwrap();
        t.draw(|f| w.draw(f, &s)).unwrap();
        assert!(screen(&t).contains("failed"));
        w.key(
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::CONTROL),
            &mut s,
        );
        t.draw(|f| w.draw(f, &s)).unwrap();
        let text = screen(&t);
        assert!(text.contains("FAILED"));
        assert!(text.contains("echo `date` **/*.rs"));
    }

    #[test]
    fn slash_opens_a_filterable_keyboard_command_menu() {
        let mut s = Session::new("/tmp/project".into(), None, None);
        s.drain_requests();
        let mut w = Workspace::default();
        w.insert("/");
        let mut t = Terminal::new(TestBackend::new(110, 34)).unwrap();
        t.draw(|f| w.draw(f, &s)).unwrap();
        let screen = format!("{:?}", t.backend().buffer());
        assert!(screen.contains("/model"));
        w.insert("eff");
        w.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), &mut s);
        assert_eq!(w.draft, "/effort ");
        assert!(s.drain_requests().is_empty());
        w.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &mut s);
        assert_eq!(w.draft, "/effort ");
    }

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
