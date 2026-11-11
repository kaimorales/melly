use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::commands::{Kind, Menu};
use melly::session::Entry;
use melly::session::{Session, Speaker};
use serde_json::Value;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

const BG: Color = Color::Rgb(20, 23, 30);
const PANEL: Color = Color::Rgb(27, 30, 39);
const INK: Color = Color::Rgb(235, 232, 228);
const MUTED: Color = Color::Rgb(162, 166, 177);
const BORDER: Color = Color::Rgb(62, 67, 81);
const ACCENT: Color = Color::Rgb(210, 183, 240);
const MINT: Color = Color::Rgb(164, 233, 193);
const SELECTED: Color = Color::Rgb(46, 42, 61);
const AMBER: Color = Color::Rgb(218, 184, 125);

pub enum Action {
    Submit(String),
    Spawn(String),
    Agents,
    Collect,
    Stop,
    Compact,
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
    pub roster: Vec<Line<'static>>,
    inspector: Inspector,
    render_cache: HashMap<u64, EntryView>,
}

struct EntryView {
    revision: u64,
    width: u16,
    collapsed: bool,
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
                    return false;
                }
                KeyCode::Char('o') => {
                    self.tools_expanded = !self.tools_expanded;
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
                            if command == "spawn" && key.code == KeyCode::Enter {
                                let task =
                                    self.draft.strip_prefix("/spawn").unwrap_or_default().trim();
                                if task.is_empty() {
                                    self.set_draft("/spawn ".into());
                                } else {
                                    self.action = Some(Action::Spawn(task.to_owned()));
                                }
                                return false;
                            }
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
            "thinking" => self.thinking_expanded = !self.thinking_expanded,
            "tools" => self.tools_expanded = !self.tools_expanded,
            "agents" => self.action = Some(Action::Agents),
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
                self.activity_focused = true;
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
            && session.approvals.is_empty()
            && !session.entries.iter().any(|e| e.speaker != Speaker::Notice)
        {
            self.home(frame, area, session);
            return;
        }

        let [header, content, footer] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .areas(area);
        let cwd = std::path::Path::new(&session.cwd)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&session.cwd);
        let model = session.model.as_deref().unwrap_or("Discovering models");
        let settings = if session.busy() && session.active_model.is_some() {
            let active = session.active_model.as_deref().unwrap_or("Codex");
            let effort = session.active_effort.as_deref().unwrap_or("unreported");
            let next = if session.active_model != session.model
                || session.active_effort != session.effort
            {
                format!(
                    " → next {} · {}",
                    model.trim_start_matches("gpt-"),
                    session.effort.as_deref().unwrap_or("default")
                )
            } else {
                String::new()
            };
            format!(
                "  active {} · {effort}{next}",
                active.trim_start_matches("gpt-")
            )
        } else {
            format!(
                "  {}  ·  {}",
                printable(model),
                session.effort.as_deref().unwrap_or("default")
            )
        };
        let [identity, state] = Layout::horizontal([
            Constraint::Min(12),
            Constraint::Length(30.min(area.width / 2)),
        ])
        .areas(header);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!(
                            "  melly / {}  ",
                            if self.agent_label.is_empty() {
                                "lead"
                            } else {
                                &self.agent_label
                            }
                        ),
                        accent(),
                    ),
                    Span::styled(printable(cwd), dim()),
                ]),
                Line::styled(settings, dim()),
            ])
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(border()),
            ),
            identity,
        );
        let seconds = session
            .elapsed()
            .map_or(String::new(), |d| format!("  {:.1}s", d.as_secs_f32()));
        let busy = session.busy() || session.connecting();
        let symbol = if busy {
            ["◐", "◓", "◑", "◒"][(SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                / 200
                % 4) as usize]
        } else {
            "●"
        };
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(
                    format!("{symbol} {}{seconds}  ", session.status),
                    Style::default().fg(MINT),
                ),
                Line::styled(
                    if session.busy() {
                        "Ctrl+C interrupt  "
                    } else {
                        "/ commands  "
                    },
                    dim(),
                ),
            ])
            .alignment(ratatui::layout::Alignment::Right)
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(border()),
            ),
            state,
        );

        let wide = area.width >= 105;
        if wide {
            let [chat, activity] =
                Layout::horizontal([Constraint::Min(60), Constraint::Length(40)]).areas(content);
            self.chat(frame, chat, session);
            self.activity(frame, activity, session);
        } else if self.activity_focused {
            self.activity(frame, content, session);
        } else {
            self.chat(frame, content, session);
        }
        frame.render_widget(
            Paragraph::new(
                "  / commands   Alt+1…9 agents   Ctrl+T thinking   Ctrl+O tool output   Ctrl+Q quit",
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
                .title(format!(
                    " Approval required · {} ",
                    if self.agent_label.is_empty() {
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

    fn home(&mut self, frame: &mut Frame, area: Rect, session: &Session) {
        let width = area.width.saturating_sub(4).min(75);
        let x = area.x + (area.width - width) / 2;
        let logo_height = if area.height >= 18 { 4 } else { 0 };
        let y = area.y + area.height.saturating_sub(logo_height + 10) / 2;
        if logo_height > 0 {
            let logo = if width >= 43 {
                vec![
                    "███   ███ ███████ ██      ██      ██   ██",
                    "████ ████ ██▄▄    ██      ██       ██ ██ ",
                    "██ ███ ██ ██▀▀    ██      ██        ███  ",
                    "██     ██ ███████ ███████ ███████    ██  ",
                ]
            } else {
                vec![
                    "█▀▄▀█ █▀▀ █   █   █ █",
                    "█ ▀ █ █▀▀ █   █   ▀█▀",
                    "▀   ▀ ▀▀▀ ▀▀▀ ▀▀▀  ▀ ",
                ]
            };
            frame.render_widget(
                Paragraph::new(
                    logo.into_iter()
                        .map(|l| Line::styled(l, Style::default().fg(INK)))
                        .collect::<Vec<_>>(),
                )
                .alignment(ratatui::layout::Alignment::Center),
                Rect::new(x, y, width, logo_height),
            );
        }
        let composer = Rect::new(
            x,
            y + logo_height + 2,
            width,
            5.min(area.bottom().saturating_sub(y + logo_height + 3)),
        );
        frame.render_widget(Block::default().style(Style::default().bg(PANEL)), composer);
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
                    if self.agent_label.is_empty() {
                        "Lead"
                    } else {
                        &self.agent_label
                    }
                ),
                accent(),
            ),
            Span::styled(session.model.as_deref().unwrap_or("Loading models"), dim()),
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
                Paragraph::new("/ commands     /spawn Luna     /agents workspace")
                    .style(dim())
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
        let composer_height = if session.selected_skills.is_empty() {
            4
        } else {
            5
        };
        let [history, composer] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(composer_height)]).areas(area);
        let [history, waiting] = Layout::vertical([
            Constraint::Min(1),
            Constraint::Length(
                if session.busy() && session.first_event_duration.is_none() {
                    3
                } else {
                    0
                },
            ),
        ])
        .areas(history);
        if session.entries.is_empty() {
            let greeting = vec![
                Line::default(),
                Line::styled("  What are we building?", accent()),
                Line::default(),
                Line::styled("  Your project. Your models. One workspace.", dim()),
                Line::default(),
                Line::from(vec![
                    Span::styled("  /model", accent()),
                    Span::styled("     choose Sol, Terra, Astra or Luna", dim()),
                ]),
                Line::from(vec![
                    Span::styled("  /effort", accent()),
                    Span::styled("    adjust reasoning strength", dim()),
                ]),
                Line::from(vec![
                    Span::styled("  /skills", accent()),
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
                false,
                &mut self.chat_scroll,
                !self.thinking_expanded,
                &mut self.render_cache,
            );
        }
        if waiting.height > 0 {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::styled(
                        format!(
                            "  ◌ Thinking · {}",
                            session
                                .active_model
                                .as_deref()
                                .or(session.model.as_deref())
                                .unwrap_or("Codex")
                        ),
                        accent(),
                    ),
                    Line::styled("  Waiting for Codex's next event…", dim()),
                ]),
                waiting,
            );
        }
        let skill_names = session
            .selected_skills
            .iter()
            .map(|s| format!("${}", s.name))
            .collect::<Vec<_>>()
            .join("  ");
        let title = if self.menu_dismissed && self.draft.starts_with('/') {
            " Literal message · Enter send "
        } else if session.busy() {
            " Steer this turn "
        } else {
            " Message "
        };
        let block = Block::default()
            .title(title)
            .title_style(accent())
            .borders(Borders::TOP)
            .border_style(if self.activity_focused {
                border()
            } else {
                accent()
            })
            .style(Style::default().bg(PANEL));
        let mut inner = block.inner(composer);
        frame.render_widget(block, composer);
        if !skill_names.is_empty() && inner.height > 1 {
            frame.render_widget(
                Paragraph::new(format!("  {skill_names}  · Ctrl+U clear"))
                    .style(Style::default().fg(MINT)),
                Rect::new(inner.x, inner.y, inner.width, 1),
            );
            inner.y += 1;
            inner.height -= 1;
        }
        let (before, after) = self.draft.split_at(self.cursor);
        let visible = if self.draft.is_empty() {
            "  › ▌ Ask, build, fix… or / for commands".into()
        } else {
            format!("  › {before}▌{after}")
        };
        let through = Paragraph::new(format!("  › {before}▌")).wrap(Wrap { trim: false });
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
        let title = format!(" {} ", menu.title());
        let block = Block::bordered()
            .title(title)
            .title_bottom(" ↑↓ select  Tab complete  Enter apply  Esc close ")
            .border_style(accent())
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
                accent().bg(SELECTED)
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
        let title = match self.inspector {
            Inspector::Activity => " Activity ",
            Inspector::Usage => " Usage ",
            Inspector::Context => " Context ",
            Inspector::Status => " Session ",
            Inspector::Help => " Shortcuts ",
        };
        let panel = Block::default()
            .title(title)
            .title_style(if self.activity_focused {
                accent()
            } else {
                dim()
            })
            .borders(Borders::LEFT)
            .border_style(border());
        let inner = panel.inner(area);
        frame.render_widget(panel, area);
        let roster_height = (self.roster.len() as u16).min(inner.height.saturating_sub(10));
        let [roster, meta, events] = Layout::vertical([
            Constraint::Length(roster_height),
            Constraint::Length(6),
            Constraint::Min(1),
        ])
        .areas(inner);
        if roster.height > 0 {
            frame.render_widget(Paragraph::new(self.roster.clone()), roster);
        }
        let used = session.context_used.map_or("—".into(), display_tokens);
        let capacity = session.context_window.map_or("—".into(), display_tokens);
        let percent = match (session.context_used, session.context_window) {
            (Some(u), Some(c)) if c > 0 => format!(" · ≈{:.0}%", 100.0 * u as f64 / c as f64),
            _ => String::new(),
        };
        let mut lines = vec![
            Line::styled(
                "  Context",
                Style::default().fg(INK).add_modifier(Modifier::BOLD),
            ),
            Line::styled(format!("  {used} / {capacity}{percent}"), dim()),
            Line::styled(
                format!(
                    "  {} session tokens",
                    session.tokens.map_or("—".into(), display_tokens)
                ),
                dim(),
            ),
        ];
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
            if session.entries.iter().all(|e| e.speaker != Speaker::Tool) {
                frame.render_widget(
                    Paragraph::new(vec![
                        Line::styled("  LIVE ACTIVITY", dim()),
                        Line::default(),
                        Line::styled("  Commands and file changes appear here.", dim()),
                        Line::default(),
                        Line::styled("  /usage    quota and token details", dim()),
                        Line::styled("  /context  inspect the context window", dim()),
                        Line::styled("  /status   connection and timing", dim()),
                    ])
                    .wrap(Wrap { trim: false }),
                    events,
                );
            } else {
                render_entries(
                    frame,
                    events,
                    session,
                    true,
                    &mut self.activity_scroll,
                    !self.tools_expanded,
                    &mut self.render_cache,
                );
            }
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
            "/compact  compact the active conversation when idle. No extra summary chat is sent.".into()],
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

fn render_entries(
    frame: &mut Frame,
    area: Rect,
    session: &Session,
    tools: bool,
    offset: &mut usize,
    collapsed: bool,
    cache: &mut HashMap<u64, EntryView>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    if cache.len() > 450 {
        cache.retain(|key, _| session.entries.iter().any(|e| e.key == *key));
    }
    let entries: Vec<_> = session
        .entries
        .iter()
        .filter(|e| (e.speaker == Speaker::Tool) == tools)
        .collect();
    let mut total = 0;
    for entry in &entries {
        if !cache.get(&entry.key).is_some_and(|v| {
            v.revision == entry.revision && v.width == area.width && v.collapsed == collapsed
        }) {
            let lines = entry_lines(entry, collapsed);
            let height = Paragraph::new(lines.clone())
                .wrap(Wrap { trim: false })
                .line_count(area.width);
            cache.insert(
                entry.key,
                EntryView {
                    revision: entry.revision,
                    width: area.width,
                    collapsed,
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
    for entry in entries {
        let view = &cache[&entry.key];
        let end = position + view.height;
        if end > top && position < top + area.height as usize {
            let skip = top.saturating_sub(position);
            let y = position.saturating_sub(top);
            let height = (view.height - skip).min(area.height as usize - y);
            let rect = Rect::new(area.x, area.y + y as u16, area.width, height as u16);
            frame.render_widget(
                Paragraph::new(view.lines.clone())
                    .style(Style::default().fg(INK))
                    .wrap(Wrap { trim: false })
                    .scroll((skip.min(u16::MAX as usize) as u16, 0)),
                rect,
            );
        }
        position = end;
        if position >= top + area.height as usize {
            break;
        }
    }
}

fn entry_lines(entry: &Entry, collapsed: bool) -> Vec<Line<'static>> {
    let text = printable(&entry.text);
    if entry.speaker == Speaker::Thinking {
        if text.is_empty() && entry.complete {
            return Vec::new();
        }
        let summary = text
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("Waiting for Codex…")
            .trim_matches('*')
            .trim_start_matches('#')
            .trim();
        let summary: String = summary.chars().take(88).collect();
        let mut lines = vec![Line::styled(
            format!(
                "  {} {}: {summary}",
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
                    .map(|l| Line::styled(format!("    {l}"), dim().bg(PANEL))),
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
    let name = entry.model.as_deref().unwrap_or("Codex");
    let title = match entry.speaker {
        Speaker::You => "YOU".into(),
        Speaker::Codex => printable(name),
        Speaker::Notice => "MELLY".into(),
        Speaker::Thinking => format!(
            "{} {} · {}",
            if collapsed && entry.complete {
                "▸"
            } else {
                "▾"
            },
            if entry.complete {
                "Thought"
            } else {
                "Thinking"
            },
            printable(name)
        ),
        Speaker::Tool => match (entry.complete, entry.outcome.as_deref()) {
            (false, _) => "● RUNNING",
            (true, Some("failed")) => "✕ FAILED",
            (true, Some("declined")) => "⊘ DECLINED",
            (true, Some("interrupted")) => "■ INTERRUPTED",
            _ => "✓ COMPLETED",
        }
        .into(),
    };
    let color = match entry.speaker {
        Speaker::Codex => ACCENT,
        Speaker::You => INK,
        Speaker::Tool
            if matches!(
                entry.outcome.as_deref(),
                Some("failed" | "declined" | "interrupted")
            ) =>
        {
            Color::Rgb(244, 174, 163)
        }
        Speaker::Tool => MINT,
        _ => MUTED,
    };
    let mut lines = vec![
        Line::default(),
        Line::styled(
            format!("  {title}"),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
    ];
    let text = printable(&entry.text);
    if text.is_empty() && entry.speaker == Speaker::Thinking && !entry.complete {
        lines.push(Line::styled("  Waiting for a thinking summary…", dim()));
    }
    if collapsed && entry.complete && entry.speaker == Speaker::Thinking {
        lines.push(Line::styled(
            format!(
                "  {}",
                text.lines()
                    .find(|l| !l.is_empty())
                    .unwrap_or("No summary provided")
            ),
            dim(),
        ));
        lines.push(Line::styled("  Ctrl+T to expand", dim()));
    } else {
        let mut code = false;
        for line in text.lines() {
            if matches!(entry.speaker, Speaker::Tool | Speaker::You) {
                lines.push(Line::raw(format!("  {line}")));
            } else if line.starts_with("```") {
                code = !code;
                lines.push(Line::styled(
                    if code {
                        format!("  ┄ {}", line.trim_start_matches('`'))
                    } else {
                        "  ┄".into()
                    },
                    dim(),
                ));
            } else if entry.speaker == Speaker::Thinking || entry.speaker == Speaker::Notice {
                lines.push(Line::styled(format!("  {line}"), dim()));
            } else if code {
                lines.push(Line::styled(
                    format!("  {line}"),
                    Style::default().fg(MINT).bg(PANEL),
                ));
            } else if line.starts_with("# ") || line.starts_with("## ") || line.starts_with("### ")
            {
                lines.push(Line::styled(
                    format!("  {}", line.trim_start_matches('#').trim_start()),
                    accent(),
                ));
            } else {
                lines.push(markdown_line(line));
            }
        }
    }
    lines.push(Line::default());
    lines
}

fn markdown_line(text: &str) -> Line<'static> {
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
                Style::default().fg(ACCENT).bg(PANEL)
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
        capture(&t, "home");
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
        assert!(text.contains("+ Thought:"));
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
