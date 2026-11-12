use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use melly::delegation::Harness;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};

use crate::{
    state::Settings,
    ui::{Action, Workspace},
};

const BG: Color = Color::Rgb(11, 11, 11);
const PANEL: Color = Color::Rgb(24, 24, 24);
const INK: Color = Color::Rgb(235, 232, 228);
const MUTED: Color = Color::Rgb(144, 144, 144);
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
    views: Vec<Workspace>,
    selected: usize,
    picker: Option<usize>,
    settings: Settings,
    settings_row: Option<usize>,
    settings_changed: bool,
    settings_error: Option<String>,
}

impl AgentWorkspace {
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
        }
        for (view, agent) in self.views.iter_mut().zip(&mut harness.agents) {
            view.agent_label = agent.id.clone();
            view.synchronize(&mut agent.session);
        }
    }

    pub fn insert(&mut self, text: &str) {
        if self.selected == 0
            && self.settings_row.is_none()
            && self.picker.is_none()
            && let Some(view) = self.views.get_mut(self.selected)
        {
            view.insert(text);
        }
    }

    pub fn key(&mut self, key: KeyEvent, harness: &mut Harness) -> bool {
        self.synchronize(harness);
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let approval = !harness.agents[self.selected].session.approvals.is_empty();
        if let Some(row) = self.settings_row {
            return self.settings_key(key, harness, row);
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('c')
            && !harness.busy()
            && !approval
        {
            return true;
        }
        if !approval {
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
            } else if matches!(key.code, KeyCode::Tab | KeyCode::PageUp | KeyCode::PageDown)
                || (key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(key.code, KeyCode::Char('b' | 't' | 'o' | 'q')))
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
                        self.views[self.selected].accept_submission();
                    }
                }
                Action::Agents => self.picker = Some(self.selected),
                Action::Settings => self.settings_row = Some(0),
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
        let completed = harness.agents.iter().skip(1).filter(|a| a.finished).count();
        let available_rows = frame.area().height.saturating_sub(15) as usize / 3;
        let mut visible: Vec<usize> = (0..harness.agents.len())
            .filter(|&i| i == 0 || i == self.selected || !harness.agents[i].finished)
            .collect();
        // Keep the observed worker visible even on short terminals.
        visible.sort_by_key(|&i| (i != self.selected, i != 0, i));
        visible.truncate(available_rows.max(1));
        visible.sort_unstable();
        for index in visible {
            let agent = &harness.agents[index];
            let focus = index == self.selected;
            let status = if !agent.session.approvals.is_empty() {
                "Approval needed"
            } else {
                &agent.session.status
            };
            let name = if index == 0 { "Conductor" } else { &agent.id };
            roster.push(Line::styled(
                format!(
                    "  {} {name}{}",
                    if focus { "›" } else { " " },
                    if index == 0 {
                        String::new()
                    } else {
                        format!(" · {}", crate::ui::ellipsis(status, 17))
                    }
                ),
                Style::default().fg(if focus { self.accent() } else { INK }),
            ));
            let detail = if index == 0 {
                status.to_owned()
            } else {
                safe(&agent.title)
            };
            roster.push(Line::styled(
                format!("    {}", crate::ui::ellipsis(&detail, 28)),
                Style::default().fg(MUTED),
            ));
            roster.push(Line::default());
        }
        if completed > 0 {
            roster.push(Line::styled(
                format!("  {completed} completed · /agents"),
                Style::default().fg(MUTED),
            ));
            roster.push(Line::default());
        }
        self.views[self.selected].agent_label = harness.agents[self.selected].id.clone();
        self.views[self.selected].roster = roster;
        self.views[self.selected].draw(frame, &harness.agents[self.selected].session);
        if let Some(index) = self.picker {
            self.draw_picker(frame, harness, index);
        }
        if let Some(row) = self.settings_row {
            self.draw_settings(frame, row);
        }
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
            .title(" Settings ")
            .title_bottom(" ↑↓ move   ←→ / Enter change   Esc close ")
            .style(Style::default().bg(PANEL).fg(INK))
            .border_style(Style::default().fg(accent));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let switch = |value| if value { "On" } else { "Off" };
        let rows = [
            (
                "Accent",
                ACCENTS.get(self.settings.accent).unwrap_or(&ACCENTS[0]).0,
            ),
            ("Thinking animation", switch(self.settings.motion)),
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
            (harness.agents.len().min(6) as u16 * 3 + 5).min(area.height.saturating_sub(2));
        let popup = Rect::new(
            (area.width - width) / 2,
            (area.height - height) / 2,
            width,
            height,
        );
        frame.render_widget(Clear, popup);
        let block = Block::bordered()
            .title(" Agents ")
            .title_bottom(" ↑↓ select  Enter open  Esc close ")
            .style(Style::default().bg(BG).fg(INK))
            .border_style(Style::default().fg(self.accent()));
        let inner = block.inner(popup);
        frame.render_widget(block, popup);
        let capacity = inner.height.saturating_sub(2).div_euclid(3).max(1) as usize;
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
                inner.y + row as u16 * 3,
                inner.width,
                3.min(inner.height.saturating_sub(row as u16 * 3)),
            );
            let lines = vec![
                Line::from(vec![Span::styled(
                    format!(
                        " {} {} · {}",
                        if active { "›" } else { " " },
                        agent.id,
                        agent.session.model.as_deref().unwrap_or("Luna")
                    ),
                    Style::default()
                        .fg(if active { self.accent() } else { INK })
                        .add_modifier(Modifier::BOLD),
                )]),
                Line::styled(
                    format!(
                        "   {} · {} tokens · {}",
                        agent.session.status,
                        agent.session.tokens.map_or("—".into(), tokens),
                        agent.mode.label()
                    ),
                    Style::default().fg(MUTED),
                ),
                Line::styled(
                    format!("   {}", safe(&agent.title)),
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
        assert!(text.contains("+ Thought"));
        capture(&t, "team");
        for (width, height) in [(80, 24), (40, 16), (24, 8), (20, 5)] {
            let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
            t.draw(|f| w.draw(f, &h)).unwrap();
        }
    }
}
