use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use melly::{
    delegation::{Harness, WorkerMode},
    session::Speaker,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
};

use crate::ui::{Action, Workspace};

const BG: Color = Color::Rgb(20, 23, 30);
const PANEL: Color = Color::Rgb(27, 30, 39);
const INK: Color = Color::Rgb(235, 232, 228);
const MUTED: Color = Color::Rgb(162, 166, 177);
const ACCENT: Color = Color::Rgb(210, 183, 240);

#[derive(Default)]
pub struct AgentWorkspace {
    views: Vec<Workspace>,
    selected: usize,
    picker: Option<usize>,
}

impl AgentWorkspace {
    pub fn synchronize(&mut self, harness: &mut Harness) {
        while self.views.len() < harness.agents.len() {
            self.views.push(Workspace::managed());
        }
        if let Some(index) = harness
            .agents
            .iter()
            .position(|a| !a.session.approvals.is_empty())
        {
            self.selected = index;
            self.picker = None;
        }
        for (view, agent) in self.views.iter_mut().zip(&mut harness.agents) {
            view.agent_label = agent.id.clone();
            view.synchronize(&mut agent.session);
        }
    }

    pub fn insert(&mut self, text: &str) {
        if let Some(view) = self.views.get_mut(self.selected) {
            view.insert(text);
        }
    }

    pub fn key(&mut self, key: KeyEvent, harness: &mut Harness) -> bool {
        self.synchronize(harness);
        if key.kind == KeyEventKind::Release {
            return false;
        }
        let approval = !harness.agents[self.selected].session.approvals.is_empty();
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
                        harness.cancel(self.selected);
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
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    harness.cancel(index);
                }
                KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => return true,
                _ => {}
            }
            return false;
        }
        if self.views[self.selected].key(key, &mut harness.agents[self.selected].session) {
            return true;
        }
        if let Some(action) = self.views[self.selected].take_action() {
            match action {
                Action::Submit(prompt) => {
                    if harness.submit(self.selected, &prompt) {
                        self.views[self.selected].accept_submission();
                    }
                }
                Action::Spawn(input) => match spawn_args(&input) {
                    Ok((task, mode, paths)) => match harness.spawn_manual(task, mode, paths) {
                        Ok(index) => {
                            self.views[self.selected].accept_submission();
                            self.selected = index;
                            self.synchronize(harness);
                        }
                        Err(error) => harness.agents[self.selected].session.notice(error),
                    },
                    Err(error) => harness.agents[self.selected].session.notice(error),
                },
                Action::Agents => self.picker = Some(self.selected),
                Action::Collect => {
                    self.selected = 0;
                    harness.agents[0].session.notice("Your next lead message will include any uncollected worker results. Use /agents to inspect them now.");
                }
                Action::Stop => {
                    if !harness.cancel(self.selected) {
                        harness.agents[self.selected]
                            .session
                            .notice("This agent has no active turn to stop.");
                    }
                }
                Action::Compact => {
                    harness.compact(self.selected);
                }
            }
        }
        false
    }

    pub fn draw(&mut self, frame: &mut Frame, harness: &Harness) {
        while self.views.len() < harness.agents.len() {
            self.views.push(Workspace::managed());
        }
        let mut roster = vec![
            Line::styled(
                format!("  Agents   {}/3 active", harness.active_workers()),
                Style::default().fg(INK).add_modifier(Modifier::BOLD),
            ),
            Line::default(),
        ];
        let first = self.selected.saturating_sub(3);
        for (index, agent) in harness.agents.iter().enumerate().skip(first).take(5) {
            let focus = index == self.selected;
            let status = if !agent.session.approvals.is_empty() {
                "Approval"
            } else {
                &agent.session.status
            };
            let tokens = agent.session.tokens.map_or("—".into(), tokens);
            roster.push(Line::styled(
                format!(
                    "  {} {}  {} · {}",
                    if focus { "›" } else { " " },
                    index + 1,
                    agent.id,
                    agent.source.label()
                ),
                Style::default().fg(if focus { ACCENT } else { INK }),
            ));
            roster.push(Line::styled(
                format!("      {status} · {tokens} tokens"),
                Style::default().fg(MUTED),
            ));
        }
        roster.push(Line::default());
        roster.push(Line::styled(
            format!("  Auto launches {}/2 · /spawn", harness.auto_launches()),
            Style::default().fg(MUTED),
        ));
        roster.push(Line::default());
        self.views[self.selected].agent_label = harness.agents[self.selected].id.clone();
        self.views[self.selected].roster = roster;
        self.views[self.selected].draw(frame, &harness.agents[self.selected].session);
        if let Some(index) = self.picker {
            self.draw_picker(frame, harness, index);
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
            .title_bottom(" ↑↓ select  Enter open  Ctrl+D stop  Esc close ")
            .style(Style::default().bg(BG).fg(INK))
            .border_style(Style::default().fg(ACCENT));
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
            let tool = agent
                .session
                .entries
                .iter()
                .rev()
                .find(|e| e.speaker == Speaker::Tool)
                .and_then(|e| e.text.lines().next())
                .unwrap_or(&agent.title);
            let lines = vec![
                Line::from(vec![Span::styled(
                    format!(
                        " {} {} · {}",
                        if active { "›" } else { " " },
                        agent.id,
                        agent.session.model.as_deref().unwrap_or("Luna")
                    ),
                    Style::default()
                        .fg(if active { ACCENT } else { INK })
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
                Line::styled(format!("   {}", safe(tool)), Style::default().fg(MUTED)),
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
                " Auto {}/2 this turn · {}/3 workers active",
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

fn spawn_args(input: &str) -> Result<(String, WorkerMode, Vec<String>), String> {
    if let Some(rest) = input.strip_prefix("--edit ") {
        let (paths, task) = rest
            .split_once(" -- ")
            .ok_or("Use /spawn --edit src/file.rs,tests/file.rs -- task")?;
        let paths = paths
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if paths.is_empty() || task.trim().is_empty() {
            return Err("Provide owned paths and a task.".into());
        }
        return Ok((task.trim().into(), WorkerMode::Edit, paths));
    }
    if input.trim().is_empty() {
        return Err("Use /spawn followed by a task for Luna.".into());
    }
    Ok((input.trim().into(), WorkerMode::Read, Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests::{capture, fixture, screen};
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
    fn stop_cancels_a_connecting_worker_before_any_turn_starts() {
        let mut h = harness();
        let mut w = AgentWorkspace::default();
        w.synchronize(&mut h);
        w.insert("/spawn Inspect the parser");
        w.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &mut h);
        let requests = h.drain_requests();
        let open = requests
            .iter()
            .find(|r| r["method"] == "thread/start")
            .unwrap();
        assert_eq!(w.selected, 1);
        w.insert("/stop");
        w.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &mut h);
        h.receive(json!({"id":open["id"],"result":{"thread":{"id":"worker"}}}));
        h.tick();
        assert!(h.agents[1].finished);
        assert!(
            !h.drain_requests()
                .iter()
                .any(|r| r["method"] == "turn/start")
        );
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
        assert!(text.contains("cargo test catalog"));
        assert!(text.contains("+ Thought:"));
        capture(&t, "team");
        for (width, height) in [(80, 24), (40, 16), (24, 8), (20, 5)] {
            let mut t = Terminal::new(TestBackend::new(width, height)).unwrap();
            t.draw(|f| w.draw(f, &h)).unwrap();
        }
    }
    #[test]
    fn manual_edit_requires_explicit_paths_and_a_task() {
        assert!(spawn_args("--edit src/file.rs").is_err());
        let (task, mode, paths) =
            spawn_args("--edit src/file.rs,tests/a.rs -- Fix the bug").unwrap();
        assert_eq!(task, "Fix the bug");
        assert_eq!(paths.len(), 2);
        assert!(matches!(mode, WorkerMode::Edit));
        assert!(matches!(
            spawn_args("Inspect the parser").unwrap().1,
            WorkerMode::Read
        ));
    }
}
