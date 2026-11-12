use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent};
use melly::{
    delegation::{Agent, Harness, ReviewVerdict},
    session::Speaker,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};

use crate::{
    handoff_ui::HandoffView,
    session_ui::{SessionAction, SessionPicker, session_title},
    state::{SessionSummary, Settings},
    ui::{Action, Workspace},
};

use crate::visual::{self, BG, BORDER, INK, MUTED, PANEL};
const ACCENTS: &[(&str, Color)] = &[
    ("Lavender", Color::Rgb(210, 183, 240)),
    ("Sky", Color::Rgb(132, 193, 245)),
    ("Mint", Color::Rgb(140, 219, 188)),
    ("Amber", Color::Rgb(234, 188, 110)),
    ("Rose", Color::Rgb(239, 163, 188)),
    ("Silver", Color::Rgb(214, 220, 229)),
];

#[derive(Default)]
pub struct AgentWorkspace {
    pub title: String,
    sessions: Option<SessionPicker>,
    session_action: Option<SessionAction>,
    views: Vec<Workspace>,
    selected: usize,
    picker: Option<usize>,
    handoff: Option<(usize, HandoffView)>,
    settings: Settings,
    settings_row: Option<usize>,
    settings_changed: bool,
    settings_error: Option<String>,
}

impl AgentWorkspace {
    pub fn scrolling(&self) -> bool {
        self.sessions.is_none()
            && self.settings_row.is_none()
            && self.picker.is_none()
            && self.handoff.is_none()
            && self
                .views
                .get(self.selected)
                .is_some_and(Workspace::scrolling)
    }

    pub fn animating(&self, harness: &Harness) -> bool {
        self.settings.motion
            && self
                .views
                .iter()
                .zip(&harness.agents)
                .any(|(view, agent)| view.animating(&agent.session))
    }
    pub fn take_session_action(&mut self) -> Option<SessionAction> {
        self.session_action.take()
    }

    pub fn show_sessions(&mut self, sessions: Vec<SessionSummary>, harness: &Harness) {
        let lead = &harness.agents[0].session;
        self.sessions = Some(SessionPicker::new(
            sessions,
            lead.cwd.clone(),
            lead.thread_id.clone(),
        ));
        self.picker = None;
        self.handoff = None;
        self.settings_row = None;
    }

    pub fn session_error(&mut self, message: String, harness: &mut Harness) {
        if let Some(picker) = &mut self.sessions {
            picker.error = Some(message);
        } else {
            harness.agents[0].session.notice(&message);
        }
    }

    pub fn close_sessions(&mut self) {
        self.sessions = None;
    }

    pub fn with_settings(settings: Settings) -> Self {
        Self {
            settings,
            ..Self::default()
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn settings_changed(&mut self) -> bool {
        std::mem::take(&mut self.settings_changed)
    }

    pub fn settings_error(&mut self, error: String) {
        self.settings_error = Some(error);
    }

    pub fn snapshot(&self, harness: &Harness) -> (String, BTreeMap<String, String>) {
        let drafts = harness
            .agents
            .iter()
            .zip(&self.views)
            .filter(|(agent, view)| agent.id == "lead" && !view.draft().is_empty())
            .map(|(agent, view)| (agent.id.clone(), view.draft().into()))
            .collect();
        (harness.agents[self.selected].id.clone(), drafts)
    }

    pub fn restore(
        &mut self,
        harness: &mut Harness,
        selected: &str,
        mut drafts: BTreeMap<String, String>,
    ) {
        self.synchronize(harness);
        self.selected = harness
            .agents
            .iter()
            .position(|agent| agent.id == selected)
            .unwrap_or(0);
        for (agent, view) in harness.agents.iter().zip(&mut self.views) {
            if agent.id == "lead"
                && let Some(draft) = drafts.remove(&agent.id)
            {
                view.insert(&draft);
            }
        }
    }

    fn accent(&self) -> Color {
        ACCENTS.get(self.settings.accent).unwrap_or(&ACCENTS[0]).1
    }

    fn ensure_views(&mut self, count: usize) {
        while self.views.len() < count {
            let mut view = Workspace::managed();
            view.configure(self.accent(), self.settings.motion);
            view.set_details(
                Some(self.settings.expand_thinking),
                Some(self.settings.expand_tools),
            );
            self.views.push(view);
        }
    }

    pub fn synchronize(&mut self, harness: &mut Harness) {
        self.ensure_views(harness.agents.len());
        if let Some(index) = harness
            .agents
            .iter()
            .position(|a| !a.session.approvals.is_empty())
        {
            self.selected = index;
            self.picker = None;
            self.settings_row = None;
            self.handoff = None;
            self.sessions = None;
            self.session_action = None;
        }
        for (view, agent) in self.views.iter_mut().zip(&mut harness.agents) {
            view.agent_label = agent.id.clone();
            view.synchronize(&mut agent.session);
        }
    }

    pub fn insert(&mut self, text: &str) {
        if let Some(picker) = &mut self.sessions {
            picker.insert(text);
            return;
        }
        if self.selected == 0
            && self.settings_row.is_none()
            && self.picker.is_none()
            && self.handoff.is_none()
            && let Some(view) = self.views.get_mut(self.selected)
        {
            view.insert(text);
        }
    }

    pub fn mouse(&mut self, mouse: MouseEvent, harness: &mut Harness) -> bool {
        self.synchronize(harness);
        // Modal surfaces own input; never scroll the hidden conversation underneath.
        if self.sessions.is_some()
            || self.settings_row.is_some()
            || self.picker.is_some()
            || self.handoff.is_some()
        {
            return false;
        }
        self.views[self.selected].mouse(mouse, &mut harness.agents[self.selected].session)
    }

    pub fn key(&mut self, key: KeyEvent, harness: &mut Harness) -> bool {
        self.synchronize(harness);
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let approval = !harness.agents[self.selected].session.approvals.is_empty();
        if !approval
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('g')
        {
            self.picker = if self.picker.is_some() {
                None
            } else {
                Some(self.selected)
            };
            self.sessions = None;
            self.settings_row = None;
            self.handoff = None;
            return false;
        }
        if !approval
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('s')
        {
            if self.sessions.is_some() {
                self.sessions = None;
            } else {
                self.session_action = Some(SessionAction::History);
            }
            return false;
        }
        if let Some(picker) = &mut self.sessions {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('q') {
                return true;
            }
            if key.code == KeyCode::Esc {
                self.sessions = None;
            } else {
                self.session_action = picker.key(key);
            }
            return false;
        }
        if let Some(row) = self.settings_row {
            return self.settings_key(key, harness, row);
        }
        if let Some((index, view)) = &mut self.handoff {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('q') => return true,
                    KeyCode::Char('c') => {
                        harness.cancel(0);
                        return false;
                    }
                    KeyCode::Char('r') => {
                        self.handoff = None;
                        return false;
                    }
                    KeyCode::Char('n' | 'p') if harness.agents.len() > 2 => {
                        *index = if key.code == KeyCode::Char('n') {
                            if *index + 1 == harness.agents.len() {
                                1
                            } else {
                                *index + 1
                            }
                        } else if *index == 1 {
                            harness.agents.len() - 1
                        } else {
                            *index - 1
                        };
                        let review = view.review;
                        *view = HandoffView::default();
                        view.review = review;
                        return false;
                    }
                    _ => return false,
                }
            }
            if key.code == KeyCode::Esc {
                self.handoff = None;
            } else if key.modifiers.is_empty() || key.code == KeyCode::BackTab {
                view.key(key.code);
            }
            return false;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('c')
            && !harness.busy()
            && !approval
        {
            return true;
        }
        if !approval {
            if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
                self.open_handoff(harness);
                return false;
            }
            if key.modifiers.contains(KeyModifiers::ALT)
                && let KeyCode::Char(digit) = key.code
                && let Some(number) = digit.to_digit(10)
                && number > 0
                && (number as usize) <= harness.agents.len()
            {
                self.selected = number as usize - 1;
                self.picker = None;
                return false;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('n') => {
                        self.selected = (self.selected + 1) % harness.agents.len();
                        self.picker = None;
                        return false;
                    }
                    KeyCode::Char('p') => {
                        self.selected = self
                            .selected
                            .checked_sub(1)
                            .unwrap_or(harness.agents.len() - 1);
                        self.picker = None;
                        return false;
                    }
                    KeyCode::Char('c') if harness.busy() => {
                        harness.cancel(0);
                        return false;
                    }
                    _ => {}
                }
            }
        }
        if let Some(mut index) = self.picker {
            match key.code {
                KeyCode::Esc => self.picker = None,
                KeyCode::Up | KeyCode::BackTab => {
                    index = index.checked_sub(1).unwrap_or(harness.agents.len() - 1);
                    self.picker = Some(index);
                }
                KeyCode::Down | KeyCode::Tab => {
                    self.picker = Some((index + 1) % harness.agents.len())
                }
                KeyCode::Enter => {
                    self.selected = index;
                    self.picker = None;
                }
                KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
                _ => {}
            }
            return false;
        }
        if self.selected != 0 && !approval {
            if key.code == KeyCode::Esc {
                self.selected = 0;
            } else if matches!(
                key.code,
                KeyCode::Tab | KeyCode::PageUp | KeyCode::PageDown | KeyCode::Up | KeyCode::Down
            ) || (key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(
                    key.code,
                    KeyCode::Home | KeyCode::End | KeyCode::Char('b' | 't' | 'o' | 'q')
                ))
            {
                return self.views[self.selected]
                    .key(key, &mut harness.agents[self.selected].session);
            }
            return false;
        }
        if self.views[self.selected].key(key, &mut harness.agents[self.selected].session) {
            return true;
        }
        if let Some(action) = self.views[self.selected].take_action() {
            match action {
                Action::Submit(prompt) => {
                    if harness.submit(0, &prompt) {
                        if self.title.is_empty() {
                            self.title = session_title(&prompt);
                        }
                        self.views[self.selected].accept_submission();
                    }
                }
                Action::Agents => self.picker = Some(self.selected),
                Action::Review => self.open_handoff(harness),
                Action::Settings => self.settings_row = Some(0),
                Action::Sessions => self.session_action = Some(SessionAction::History),
                Action::New => self.session_action = Some(SessionAction::New),
                Action::Collect => {
                    self.selected = 0;
                    harness.agents[0].session.notice("Your next lead message will include any uncollected worker results. Use /agents to inspect them now.");
                }
                Action::Stop => {
                    if !harness.cancel(0) {
                        harness.agents[self.selected]
                            .session
                            .notice("This agent has no active turn to stop.");
                    }
                }
                Action::Compact => {
                    harness.compact(0);
                }
            }
        }
        false
    }

    pub fn draw(&mut self, frame: &mut Frame, harness: &Harness) {
        self.ensure_views(harness.agents.len());
        let mut roster = vec![
            Line::styled(
                "  Agents",
                Style::default().fg(INK).add_modifier(Modifier::BOLD),
            ),
            Line::default(),
        ];
        let available_rows = frame.area().height.saturating_sub(15) as usize / 4;
        let mut visible: Vec<usize> = (0..harness.agents.len())
            .filter(|&i| i == 0 || i == self.selected || needs_attention(&harness.agents[i]))
            .collect();
        // Keep the observed worker visible even on short terminals.
        visible.sort_by_key(|&i| {
            (
                i != self.selected,
                i != 0,
                harness.agents[i].finished,
                std::cmp::Reverse(i),
            )
        });
        visible.truncate(available_rows.max(1));
        visible.sort_unstable();
        let hidden = harness.agents.len().saturating_sub(visible.len());
        for index in visible {
            let agent = &harness.agents[index];
            let focus = index == self.selected;
            let name = if index == 0 { "Conductor" } else { &agent.id };
            let elapsed = if agent.session.busy() {
                agent
                    .session
                    .elapsed()
                    .map(|d| format!(" · {}s", d.as_secs()))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            roster.push(Line::styled(
                format!("  {} {name}{elapsed}", if focus { "›" } else { " " },),
                Style::default().fg(if focus { self.accent() } else { INK }),
            ));
            if index > 0 {
                roster.push(Line::styled(
                    format!("    {}", crate::ui::ellipsis(&safe(&agent.title), 28)),
                    Style::default().fg(INK),
                ));
            }
            let detail = if index == 0 {
                harness.lead_wait_status().map(str::to_owned)
            } else {
                None
            }
            .unwrap_or_else(|| agent_activity(agent));
            let indicator = if agent.session.busy() {
                visual::beat(visual::time_ms(), self.settings.motion)
            } else {
                " "
            };
            roster.push(Line::styled(
                format!("  {indicator} {}", crate::ui::ellipsis(&detail, 28)),
                Style::default().fg(MUTED),
            ));
            roster.push(Line::default());
        }
        if hidden > 0 {
            roster.push(Line::styled(
                format!("  {hidden} more · /agents"),
                Style::default().fg(MUTED),
            ));
            roster.push(Line::default());
        }
        self.views[self.selected].agent_label = harness.agents[self.selected].id.clone();
        self.views[self.selected]
            .session_title
            .clone_from(&self.title);
        self.views[self.selected].roster = roster;
        self.views[self.selected].draw(frame, &harness.agents[self.selected].session);
        if let Some(index) = self.picker {
            self.draw_picker(frame, harness, index);
        }
        if let Some(row) = self.settings_row {
            self.draw_settings(frame, row);
        }
        let accent = self.accent();
        if let Some((index, view)) = &mut self.handoff {
            view.draw(frame, &harness.agents[*index], accent, self.settings.motion);
        }
        if let Some(picker) = &self.sessions {
            picker.draw(frame, accent, harness.busy());
        }
    }

    fn open_handoff(&mut self, harness: &mut Harness) {
        if harness.agents.len() <= 1 {
            harness.agents[0]
                .session
                .notice("No worker handoffs yet. A brief appears when the conductor delegates.");
            return;
        }
        let index = if self.selected > 0 {
            self.selected
        } else {
            harness.agents.len() - 1
        };
        self.handoff = Some((index, HandoffView::default()));
        self.picker = None;
    }

    fn settings_key(&mut self, key: KeyEvent, harness: &mut Harness, row: usize) -> bool {
        const ROWS: usize = 5;
        match key.code {
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.settings_row = None;
                if !harness.busy() {
                    return true;
                }
                harness.cancel(0);
            }
            KeyCode::Esc => self.settings_row = None,
            KeyCode::Up | KeyCode::BackTab => self.settings_row = Some((row + ROWS - 1) % ROWS),
            KeyCode::Down | KeyCode::Tab => self.settings_row = Some((row + 1) % ROWS),
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right => {
                let mut thinking = None;
                let mut tools = None;
                match row {
                    0 => {
                        let step = if key.code == KeyCode::Left {
                            ACCENTS.len() - 1
                        } else {
                            1
                        };
                        self.settings.accent =
                            (self.settings.accent.min(ACCENTS.len() - 1) + step) % ACCENTS.len();
                    }
                    1 => self.settings.motion = !self.settings.motion,
                    2 => {
                        self.settings.expand_thinking = !self.settings.expand_thinking;
                        thinking = Some(self.settings.expand_thinking);
                    }
                    3 => {
                        self.settings.expand_tools = !self.settings.expand_tools;
                        tools = Some(self.settings.expand_tools);
                    }
                    4 => {
                        self.settings.auto_delegate = !self.settings.auto_delegate;
                        harness.automatic_enabled = self.settings.auto_delegate;
                    }
                    _ => return false,
                }
                let accent = self.accent();
                for view in &mut self.views {
                    view.configure(accent, self.settings.motion);
                    view.set_details(thinking, tools);
                }
                self.settings_changed = true;
                self.settings_error = None;
            }
            _ => {}
        }
        false
    }

    fn draw_settings(&self, frame: &mut Frame, selection: usize) {
        let area = frame.area();
        if area.width < 24 || area.height < 8 {
            return;
        }
        let width = area.width.saturating_sub(4).min(57);
        let height = area.height.saturating_sub(2).min(13);
        let popup = Rect::new(
            (area.width - width) / 2,
            (area.height - height) / 2,
            width,
            height,
        );
        let accent = self.accent();
        frame.render_widget(Clear, popup);
        let block = Block::bordered()
            .title(Line::styled(
                " Settings ",
                Style::default().fg(INK).add_modifier(Modifier::BOLD),
            ))
            .title_bottom(" ↑↓ move   ←→ / Enter change   Esc close ")
            .style(Style::default().bg(PANEL).fg(INK))
            .border_style(Style::default().fg(BORDER));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let switch = |value| if value { "On" } else { "Off" };
        let rows = [
            (
                "Accent",
                ACCENTS.get(self.settings.accent).unwrap_or(&ACCENTS[0]).0,
            ),
            ("Animations", switch(self.settings.motion)),
            ("Expand thinking", switch(self.settings.expand_thinking)),
            ("Expand tool output", switch(self.settings.expand_tools)),
            ("Automatic delegation", switch(self.settings.auto_delegate)),
        ];
        let start = selection.saturating_sub(inner.height.saturating_sub(1) as usize);
        let lines: Vec<_> = rows
            .iter()
            .enumerate()
            .skip(start)
            .map(|(index, (label, value))| {
                let selected = index == selection;
                let marker = if selected { "›" } else { " " };
                Line::from(vec![
                    Span::styled(
                        format!(" {marker} {label:<22}"),
                        Style::default().fg(if selected { accent } else { INK }),
                    ),
                    Span::styled(*value, Style::default().fg(accent)),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), inner);
        if let Some(error) = &self.settings_error
            && inner.height > 6
        {
            frame.render_widget(
                Paragraph::new(safe(error)).style(Style::default().fg(MUTED)),
                Rect::new(
                    inner.x + 1,
                    inner.y + 6,
                    inner.width.saturating_sub(1),
                    inner.height - 6,
                ),
            );
        }
    }

    fn draw_picker(&self, frame: &mut Frame, harness: &Harness, index: usize) {
        let area = frame.area();
        if area.width < 24 || area.height < 8 {
            return;
        }
        let width = area.width.saturating_sub(4).min(75);
        let height =
            (harness.agents.len().min(6) as u16 * 4 + 5).min(area.height.saturating_sub(2));
        let popup = Rect::new(
            (area.width - width) / 2,
            (area.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup);
        let block = Block::bordered()
            .title(Line::styled(
                " Agents ",
                Style::default().fg(INK).add_modifier(Modifier::BOLD),
            ))
            .title_bottom(" ↑↓ select  Enter open  Esc close ")
            .style(Style::default().bg(BG).fg(INK))
            .border_style(Style::default().fg(BORDER));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let capacity = inner.height.saturating_sub(2).div_euclid(4).max(1) as usize;
        let first = index.saturating_sub(capacity - 1);
        for (row, (i, agent)) in harness
            .agents
            .iter()
            .enumerate()
            .skip(first)
            .take(capacity)
            .enumerate()
        {
            let active = i == index;
            let rect = Rect::new(
                inner.x,
                inner.y + row as u16 * 4,
                inner.width,
                4.min(inner.height.saturating_sub(row as u16 * 4)),
            );
            let lines = vec![
                Line::from(vec![Span::styled(
                    format!(
                        " {} {} · {}",
                        if active { "›" } else { " " },
                        if i == 0 { "Conductor" } else { &agent.id },
                        crate::ui::model_name(agent.session.model.as_deref().unwrap_or("Luna"))
                    ),
                    Style::default()
                        .fg(if active { self.accent() } else { INK })
                        .add_modifier(Modifier::BOLD),
                )]),
                Line::styled(
                    format!(
                        "   {} · {} tokens",
                        if i == 0 {
                            harness
                                .lead_wait_status()
                                .unwrap_or_else(|| agent_phase(agent))
                        } else {
                            agent_phase(agent)
                        },
                        agent.session.tokens.map_or("—".into(), tokens),
                    ),
                    Style::default().fg(MUTED),
                ),
                Line::styled(
                    format!(
                        "   {}",
                        if i == 0 {
                            session_title(&self.title)
                        } else {
                            safe(&agent.title)
                        }
                    ),
                    Style::default().fg(MUTED),
                ),
                Line::styled(
                    format!(
                        "   {}",
                        if agent.session.busy() {
                            agent_activity(agent)
                        } else {
                            String::new()
                        }
                    ),
                    Style::default().fg(MUTED),
                ),
            ];
            frame.render_widget(
                Paragraph::new(lines).style(Style::default().bg(if active { PANEL } else { BG })),
                rect,
            );
        }
        let limit = if let Some(reason) = &harness.automatic_block_reason {
            format!(" Auto: {reason}")
        } else {
            format!(
                " Worker turns {}/2 · {}/3 workers active",
                harness.auto_launches(),
                harness.active_workers()
            )
        };
        frame.render_widget(
            Paragraph::new(limit).style(Style::default().fg(MUTED)),
            Rect::new(inner.x, inner.bottom().saturating_sub(1), inner.width, 1),
        );
    }
}

fn needs_attention(agent: &Agent) -> bool {
    !agent.finished
        || agent.session.busy()
        || agent.session.connecting()
        || !agent.session.approvals.is_empty()
        || !agent
            .handoff
            .review
            .as_ref()
            .is_some_and(|r| r.verdict == ReviewVerdict::Accepted)
        || agent.session.status != "completed"
}

fn agent_phase(agent: &Agent) -> &str {
    if !agent.session.approvals.is_empty() {
        return "Approval needed";
    }
    if agent.session.connecting() || agent.session.busy() {
        return &agent.session.status;
    }
    if agent.finished {
        if agent.session.status != "completed" {
            return &agent.session.status;
        }
        return match &agent.handoff.review {
            Some(review) if review.verdict == ReviewVerdict::Accepted => "Accepted",
            Some(_) => "Changes requested",
            None if agent.handoff.prompt.is_none() => "Review unknown",
            None => "Awaiting review",
        };
    }
    &agent.session.status
}

fn agent_activity(agent: &Agent) -> String {
    if !agent.session.approvals.is_empty() {
        return "Approval needed".into();
    }
    if agent.session.busy() {
        if let Some(entry) = agent.session.entries.iter().rev().find(|e| !e.complete) {
            if entry.speaker == Speaker::Tool {
                let first = entry.text.lines().next().unwrap_or("Running tool");
                if first.starts_with("Files ·") {
                    let path = entry.text.lines().nth(1).unwrap_or("files");
                    let name = std::path::Path::new(path)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(path);
                    return format!("Editing {}", safe(name));
                }
                if let Some(command) = first.strip_prefix("$ ") {
                    let command = command
                        .strip_prefix("/bin/zsh -lc ")
                        .or_else(|| command.strip_prefix("/bin/bash -lc "))
                        .unwrap_or(command)
                        .trim_matches(['\'', '"']);
                    return format!("$ {}", safe(command));
                }
                return safe(first);
            }
            if entry.speaker == Speaker::Thinking {
                return "Thinking".into();
            }
        }
        if agent.session.status == "Running tool" {
            return "Working".into();
        }
    }
    safe(agent_phase(agent))
}

fn tokens(n: u64) -> String {
    if n < 1000 {
        n.to_string()
    } else {
        format!("{:.1}k", n as f64 / 1000.0)
    }
}
fn safe(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(140).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests::{capture, fixture, screen};
    use melly::delegation::WorkerMode;
    use ratatui::{Terminal, backend::TestBackend};
    use serde_json::json;

    fn harness() -> Harness {
        let mut h = Harness::new("/tmp/project".into(), None, None);
        h.drain_requests();
        h.agents[0].session = fixture();
        h.agents[0].session.models.push(
            melly::catalog::Model::parse(
                &json!({"model":"gpt-6-luna","defaultReasoningEffort":"low"}),
            )
            .unwrap(),
        );
        h
    }

    #[test]
    fn worker_chat_scrolls_without_sending_input_and_overlays_own_input() {
        let mut h = harness();
        h.spawn_manual("Inspect the code".into(), WorkerMode::Read, vec![])
            .unwrap();
        h.agents[1].session.thread_id = Some("worker-scroll".into());
        h.agents[1].session.receive(json!({"method":"item/completed","params":{
            "threadId":"worker-scroll","item":{"type":"agentMessage","id":"worker-answer","phase":"final_answer",
            "text": (0..100).map(|i| format!("WORKER_ROW_{i:03}\n")).collect::<String>()}}}));
        h.drain_requests();
        let mut w = AgentWorkspace::with_settings(Settings {
            motion: false,
            ..Settings::default()
        });
        w.synchronize(&mut h);
        w.insert("Conductor draft");
        w.key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::ALT), &mut h);
        let mut t = Terminal::new(TestBackend::new(130, 38)).unwrap();
        t.draw(|f| w.draw(f, &h)).unwrap();
        let before = screen(&t);
        let wheel = MouseEvent {
            kind: crossterm::event::MouseEventKind::ScrollUp,
            column: 10,
            row: 10,
            modifiers: KeyModifiers::NONE,
        };
        assert!(w.mouse(wheel, &mut h));
        t.draw(|f| w.draw(f, &h)).unwrap();
        assert_ne!(screen(&t), before);
        assert!(screen(&t).contains("↑ History"));
        w.key(KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL), &mut h);
        t.draw(|f| w.draw(f, &h)).unwrap();
        assert!(screen(&t).contains("WORKER_ROW_099"));
        assert!(!screen(&t).contains("↑ History"));
        w.show_sessions(vec![], &h);
        assert!(!w.mouse(wheel, &mut h));
        assert_eq!(w.views[0].draft(), "Conductor draft");
        assert!(
            h.drain_requests().is_empty(),
            "Inspecting history must not call the model"
        );
    }

    #[test]
    fn session_history_and_new_are_discoverable_commands() {
        let h = harness();
        for command in ["sessions", "new"] {
            let menu =
                crate::commands::Menu::from_draft(&format!("/{command}"), &h.agents[0].session)
                    .unwrap();
            assert!(menu.rows.iter().any(|r| r.value == command));
        }
    }

    #[test]
    fn local_navigation_preserves_draft_and_approval_preempts_history() {
        let mut h = harness();
        let mut w = AgentWorkspace::default();
        w.synchronize(&mut h);
        w.insert("Keep this draft");
        let control = |c| KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        w.key(control('s'), &mut h);
        assert_eq!(w.take_session_action(), Some(SessionAction::History));
        w.show_sessions(vec![], &h);
        w.insert("search only");
        w.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &mut h);
        assert!(w.take_session_action().is_none());
        assert_eq!(w.views[0].draft(), "Keep this draft");
        w.key(control('g'), &mut h);
        assert!(w.sessions.is_none());
        assert_eq!(w.picker, Some(0));
        w.show_sessions(vec![], &h);
        h.agents[0]
            .session
            .approvals
            .push_back(melly::session::Approval {
                id: json!("approval"),
                description: "Approve this command".into(),
                can_accept: true,
            });
        w.synchronize(&mut h);
        assert!(w.sessions.is_none());
        assert_eq!(w.views[0].draft(), "Keep this draft");
        w.key(control('s'), &mut h);
        assert!(w.take_session_action().is_none());
    }

    #[test]
    fn finished_unreviewed_worker_stays_visible_without_opening_its_chat() {
        let mut h = harness();
        h.spawn_manual("Fix session restoration".into(), WorkerMode::Read, vec![])
            .unwrap();
        h.agents[1].finished = true;
        h.agents[1].session.thread_id = Some("finished-worker".into());
        h.agents[1].session.status = "completed".into();
        h.agents[1].result = Some("Checks passed".into());
        h.agents[1].handoff.prompt = Some("Fix session restoration".into());
        h.agents[0].session.receive(json!({"method":"item/completed","params":{"threadId":"preview-thread","item":{
            "type":"agentMessage","id":"a","phase":"final_answer","text":"Session visibility fixture"}}}));
        let mut w = AgentWorkspace::default();
        let mut t = Terminal::new(TestBackend::new(130, 38)).unwrap();
        t.draw(|f| w.draw(f, &h)).unwrap();
        assert!(screen(&t).contains("Awaiting review"));
        assert!(screen(&t).contains("Fix session restoration"));
    }

    #[test]
    fn handoff_view_is_read_only_and_switches_brief_review() {
        let mut h = harness();
        h.spawn_manual("Fix the return transition".into(), WorkerMode::Read, vec![])
            .unwrap();
        let mut w = AgentWorkspace::default();
        w.synchronize(&mut h);
        w.insert("Keep my conductor draft");
        w.key(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            &mut h,
        );
        let mut t = Terminal::new(TestBackend::new(130, 38)).unwrap();
        t.draw(|f| w.draw(f, &h)).unwrap();
        assert!(screen(&t).contains("Instructions from conductor"));
        assert!(screen(&t).contains("Fix the return transition"));
        w.insert("never send this");
        w.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &mut h);
        assert_eq!(w.views[0].draft(), "Keep my conductor draft");
        w.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), &mut h);
        t.draw(|f| w.draw(f, &h)).unwrap();
        assert!(screen(&t).contains("No verdict recorded"));
        w.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &mut h);
        assert_eq!(w.selected, 0);
        assert_eq!(w.views[0].draft(), "Keep my conductor draft");
    }

    #[test]
    fn handoff_renders_recorded_evidence_and_gives_approvals_priority() {
        use melly::delegation::{AssignmentBrief, ObservedCommand, ReviewRecord, ReviewVerdict};
        let mut h = harness();
        h.spawn_manual("Fix the return transition".into(), WorkerMode::Read, vec![])
            .unwrap();
        let a = &mut h.agents[1];
        a.handoff.brief = Some(AssignmentBrief {
            objective: "Remove the flicker when returning home.".into(),
            context: "The scrollbar disappears on the short homepage. Reserve its space in the shared html rule; preserve the intro and reduced-motion behavior.".into(),
            expected_output: "Run the navigation checks and verify the return transition at desktop and mobile sizes. Report commands and remaining limitations.".into(),
            reason: "The change is isolated; the cause and acceptance checks are established.".into(),
        });
        a.owned_paths = vec!["assets/site/styles.css".into()];
        a.mode = WorkerMode::Edit;
        a.session.status = "completed".into();
        a.finished = true;
        a.result = Some("Reserved scrollbar space. Navigation checks pass; visual verification is still outstanding.".into());
        a.handoff.report_delivered = true;
        a.handoff.commands = vec![ObservedCommand {
            command: "node --test tests/navigation.test.cjs".into(),
            exit_code: 0,
            output: Some("3 checks passed".into()),
        }];
        a.handoff.review = Some(ReviewRecord {
            verdict: ReviewVerdict::ChangesRequested,
            summary: "The diff is scoped correctly and navigation tests pass. Verify the reported flicker in a browser before accepting the fix.".into(),
            commands: vec![ObservedCommand {command:"git diff -- assets/site/styles.css".into(), exit_code:0, output:Some("+  scrollbar-gutter: stable;".into())}],
        });
        let mut w = AgentWorkspace::default();
        w.synchronize(&mut h);
        w.key(
            KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
            &mut h,
        );
        for (width, height) in [(130, 44), (80, 30), (40, 16), (24, 8), (20, 5)] {
            let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
            t.draw(|f| w.draw(f, &h)).unwrap();
            if width == 130 {
                capture(&t, "handoff-brief");
            }
        }
        w.key(
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::NONE),
            &mut h,
        );
        let mut t = Terminal::new(TestBackend::new(130, 44)).unwrap();
        t.draw(|f| w.draw(f, &h)).unwrap();
        assert!(screen(&t).contains("Full prompt sent to Luna"));
        w.key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), &mut h);
        t.draw(|f| w.draw(f, &h)).unwrap();
        let text = screen(&t);
        assert!(text.contains("Changes requested"));
        assert!(text.contains("git diff -- assets/site/styles.css"));
        assert!(text.contains("exit 0"));
        assert!(!text.contains("Accepted by conductor"));
        capture(&t, "handoff-review");
        w.key(
            KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE),
            &mut h,
        );
        t.draw(|f| w.draw(f, &h)).unwrap();
        assert!(screen(&t).contains("scrollbar-gutter: stable;"));
        capture(&t, "handoff-output");
        h.agents[1].handoff = Default::default();
        t.draw(|f| w.draw(f, &h)).unwrap();
        assert!(screen(&t).contains("No saved delivery or review metadata"));
        assert!(!screen(&t).contains("has not been delivered"));
        h.agents[0]
            .session
            .approvals
            .push_back(melly::session::Approval {
                description: "Approve a command".into(),
                can_accept: true,
                id: json!("approval"),
            });
        w.synchronize(&mut h);
        assert!(w.handoff.is_none());
        t.draw(|f| w.draw(f, &h)).unwrap();
        assert!(screen(&t).contains("Approval required"));
    }

    #[test]
    fn worker_view_accepts_no_user_input() {
        let mut h = harness();
        h.spawn_manual("Inspect parser".into(), WorkerMode::Read, vec![])
            .unwrap();
        let mut w = AgentWorkspace::default();
        w.synchronize(&mut h);
        w.selected = 1;
        w.insert("pasted worker prompt");
        for key in [KeyCode::Char('x'), KeyCode::Enter] {
            w.key(KeyEvent::new(key, KeyModifiers::NONE), &mut h);
        }
        assert!(w.views[1].draft().is_empty());
        assert!(
            !h.drain_requests()
                .iter()
                .any(|r| r["method"] == "turn/start")
        );
        w.key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &mut h);
        assert_eq!(w.selected, 0);
    }

    #[test]
    fn selected_agent_shows_its_own_activity_tokens_and_context() {
        let mut h = harness();
        let mut w = AgentWorkspace::default();
        for task in ["Review the cache boundary", "Check invalidation tests"] {
            h.spawn_manual(task.into(), WorkerMode::Read, vec![])
                .unwrap();
        }
        let s = &mut h.agents[1].session;
        s.thread_id = Some("worker".into());
        s.status = "Working".into();
        s.receive(json!({"method":"turn/started","params":{"threadId":"worker","turn":{"id":"live-worker-turn"}}}));
        s.receive(json!({"method":"item/completed","params":{"threadId":"worker","item":{
            "type":"reasoning","id":"r","summary":["Tracing cache invalidation","Checking whether configuration changes invalidate the catalog."]}}}));
        s.receive(
            json!({"method":"item/agentMessage/delta","params":{"threadId":"worker","itemId":"a",
            "delta":"I found the cache boundary. Checking the configuration-change path now."}}),
        );
        s.receive(json!({"method":"item/started","params":{"threadId":"worker","item":{
            "type":"commandExecution","id":"c","command":"cargo test catalog","status":"inProgress"}}}));
        s.tokens = Some(14200);
        s.context_used = Some(8100);
        s.context_window = Some(200000);
        w.synchronize(&mut h);
        w.selected = 1;
        let mut t = Terminal::new(TestBackend::new(130, 38)).unwrap();
        t.draw(|f| w.draw(f, &h)).unwrap();
        let text = screen(&t);
        assert!(text.contains("luna-1"));
        assert!(text.contains("luna-2"));
        assert!(text.contains("14.2k"));
        assert!(text.contains("Running checks"));
        assert!(
            w.views[1]
                .roster
                .iter()
                .any(|line| line.to_string().contains("$ cargo test catalog"))
        );
        assert!(text.contains("+ Thought"));
        capture(&t, "team");
        for (width, height) in [(80, 24), (40, 16), (24, 8), (20, 5)] {
            let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
            t.draw(|f| w.draw(f, &h)).unwrap();
        }
    }
}
