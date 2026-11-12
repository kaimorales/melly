use crossterm::event::KeyCode;
use melly::delegation::{Agent, ObservedCommand, ReviewVerdict};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph, Wrap},
};

use crate::visual::{self, AMBER, BG, INK, MINT, MUTED};

#[derive(Default)]
pub struct HandoffView {
    pub review: bool,
    full_prompt: bool,
    show_output: bool,
    scroll: [usize; 2],
}

impl HandoffView {
    pub fn key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Left | KeyCode::Right => {
                self.review = !self.review
            }
            KeyCode::Char('p') if !self.review => {
                self.full_prompt = !self.full_prompt;
                self.scroll[0] = 0;
            }
            KeyCode::Char('o') if self.review => {
                self.show_output = !self.show_output;
                self.scroll[1] = 0;
            }
            KeyCode::Up => {
                self.scroll[self.review as usize] =
                    self.scroll[self.review as usize].saturating_sub(1)
            }
            KeyCode::Down => {
                self.scroll[self.review as usize] =
                    self.scroll[self.review as usize].saturating_add(1)
            }
            KeyCode::PageUp => {
                self.scroll[self.review as usize] =
                    self.scroll[self.review as usize].saturating_sub(10)
            }
            KeyCode::PageDown => {
                self.scroll[self.review as usize] =
                    self.scroll[self.review as usize].saturating_add(10)
            }
            KeyCode::Home => self.scroll[self.review as usize] = 0,
            KeyCode::End => self.scroll[self.review as usize] = usize::MAX,
            _ => {}
        }
    }

    pub fn draw(&mut self, frame: &mut Frame, agent: &Agent, accent: Color, motion: bool) {
        let area = frame.area();
        frame.render_widget(Clear, area);
        frame.render_widget(Block::new().style(Style::default().bg(BG).fg(INK)), area);
        if area.width < 24 || area.height < 8 {
            frame.render_widget(Paragraph::new("Resize terminal · Esc back"), area);
            return;
        }
        let width = area.width.saturating_sub(4).min(96);
        let x = area.x + (area.width - width) / 2;
        let top = area.y + 1;
        let heading = Line::from(vec![
            Span::styled(
                clean(&agent.id),
                Style::default().fg(accent).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  /  handoff", Style::default().fg(MUTED)),
        ]);
        frame.render_widget(Paragraph::new(heading), Rect::new(x, top, width, 1));

        let live = if !agent.finished {
            visual::beat(visual::time_ms(), motion)
        } else {
            "○"
        };
        let (end, color) = match agent.handoff.review.as_ref().map(|r| r.verdict) {
            Some(ReviewVerdict::Accepted) => ("● Accepted by conductor", MINT),
            Some(ReviewVerdict::ChangesRequested) => ("● Changes requested", AMBER),
            None if metadata_missing(agent) => ("○ Review unknown", MUTED),
            None if agent.finished && agent.handoff.report_delivered => {
                ("○ Awaiting verdict", MUTED)
            }
            None if agent.finished => ("○ Awaiting review", MUTED),
            None => ("○ Review", MUTED),
        };
        let lifecycle = Line::from(vec![
            Span::styled(
                if agent.handoff.prompt.is_some() {
                    "● Brief"
                } else {
                    "○ Brief"
                },
                Style::default().fg(MUTED),
            ),
            Span::styled("  ──  ", Style::default().fg(MUTED)),
            Span::styled(
                if agent.finished {
                    format!("● {}", clean(&agent.session.status))
                } else {
                    format!("{live} Luna")
                },
                Style::default().fg(if agent.finished { MUTED } else { accent }),
            ),
            Span::styled("  ──  ", Style::default().fg(MUTED)),
            Span::styled(end, Style::default().fg(color)),
        ]);
        frame.render_widget(Paragraph::new(lifecycle), Rect::new(x, top + 2, width, 1));
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" Brief ", tab_style(!self.review, accent)),
                Span::raw("   "),
                Span::styled(" Review ", tab_style(self.review, accent)),
            ])),
            Rect::new(x, top + 4, width, 1),
        );

        let body = Rect::new(x, top + 6, width, area.height.saturating_sub(10));
        let lines = if self.review {
            review_lines(agent, self.show_output)
        } else {
            brief_lines(agent, self.full_prompt)
        };
        let paragraph = Paragraph::new(lines).wrap(Wrap { trim: false });
        let total = paragraph.line_count(body.width);
        let max_scroll = total
            .saturating_sub(body.height as usize)
            .min(u16::MAX as usize);
        let scroll = &mut self.scroll[self.review as usize];
        *scroll = (*scroll).min(max_scroll);
        frame.render_widget(paragraph.scroll((*scroll as u16, 0)), body);
        let hint = if width < 52 {
            "Tab tabs · ↑↓ scroll · Esc back"
        } else if self.review {
            "Tab brief · o output · ↑↓ scroll · Ctrl+N/P workers · Esc back"
        } else if self.full_prompt {
            "Tab review · p brief · ↑↓ scroll · Esc back"
        } else {
            "Tab review · p full prompt · ↑↓ scroll · Esc back"
        };
        let footer = format!(
            "{hint}{}",
            if max_scroll > 0 {
                format!("  {}/{}", *scroll + 1, max_scroll + 1)
            } else {
                String::new()
            }
        );
        frame.render_widget(
            Paragraph::new(footer).style(Style::default().fg(MUTED)),
            Rect::new(x, area.bottom() - 2, width, 1),
        );
    }
}

fn tab_style(selected: bool, accent: Color) -> Style {
    if selected {
        Style::default()
            .fg(accent)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::default().fg(MUTED)
    }
}

fn section(lines: &mut Vec<Line<'static>>, title: &str, body: &str) {
    lines.push(Line::styled(title.to_owned(), Style::default().fg(MUTED)));
    lines.push(Line::default());
    lines.extend(body.lines().map(|line| Line::raw(clean(line))));
    lines.push(Line::default());
    lines.push(Line::default());
}

fn brief_lines(agent: &Agent, full: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if full {
        section(
            &mut lines,
            "Full prompt sent to Luna",
            agent
                .handoff
                .prompt
                .as_deref()
                .unwrap_or("Original prompt was not saved for this older session."),
        );
    } else if let Some(brief) = &agent.handoff.brief {
        section(&mut lines, "Instructions from conductor", &brief.objective);
        section(
            &mut lines,
            "Scope",
            &format!(
                "{} · {}",
                agent.mode.label(),
                if agent.owned_paths.is_empty() {
                    "No owned paths".into()
                } else {
                    agent.owned_paths.join("\n")
                }
            ),
        );
        if !brief.context.is_empty() {
            section(&mut lines, "Findings & approach", &brief.context);
        }
        section(
            &mut lines,
            "Expected result & checks",
            &brief.expected_output,
        );
        section(&mut lines, "Reason for delegation", &brief.reason);
    } else {
        section(
            &mut lines,
            "Instructions from conductor",
            "The brief was not saved for this older session. The worker's chat may still contain its original prompt.",
        );
    }
    if agent.handoff.corrections_omitted > 0 {
        section(
            &mut lines,
            "Earlier follow-ups",
            &format!(
                "{} older prompts are no longer retained.",
                agent.handoff.corrections_omitted
            ),
        );
    }
    for (index, correction) in agent.handoff.corrections.iter().enumerate() {
        section(
            &mut lines,
            &format!(
                "Follow-up {} · exact prompt",
                agent.handoff.corrections_omitted + index + 1
            ),
            correction,
        );
    }
    lines
}

fn review_lines(agent: &Agent, show_output: bool) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(review) = &agent.handoff.review {
        section(
            &mut lines,
            match review.verdict {
                ReviewVerdict::Accepted => "Accepted by conductor",
                ReviewVerdict::ChangesRequested => "Conductor requested changes",
            },
            &review.summary,
        );
        commands(
            &mut lines,
            "Conductor commands observed after result delivery",
            &review.commands,
            show_output,
        );
    } else {
        section(
            &mut lines,
            "No verdict recorded",
            if metadata_missing(agent) {
                "No saved delivery or review metadata for this older session. Its past review status is unknown."
            } else if agent.handoff.report_delivered {
                "The result reached the conductor. No explicit review verdict has been recorded for this attempt."
            } else if agent.finished {
                "Luna stopped. The result has not been delivered to the conductor."
            } else {
                "Luna is working. Completion alone will not mark this task accepted."
            },
        );
    }
    commands(
        &mut lines,
        "Luna commands · actual exit codes",
        &agent.handoff.commands,
        show_output,
    );
    section(
        &mut lines,
        "Luna's report",
        agent.result.as_deref().unwrap_or("No final report yet."),
    );
    lines
}

fn metadata_missing(agent: &Agent) -> bool {
    agent.handoff.prompt.is_none()
        && agent.handoff.corrections.is_empty()
        && !agent.handoff.report_delivered
}

fn commands(
    lines: &mut Vec<Line<'static>>,
    title: &str,
    commands: &[ObservedCommand],
    show_output: bool,
) {
    lines.push(Line::styled(title.to_owned(), Style::default().fg(MUTED)));
    lines.push(Line::default());
    if commands.is_empty() {
        lines.push(Line::styled(
            "No command outcomes recorded.",
            Style::default().fg(MUTED),
        ));
    }
    for command in commands {
        lines.push(Line::styled(
            format!(
                "{} exit {}",
                if command.exit_code == 0 { "✓" } else { "×" },
                command.exit_code
            ),
            Style::default().fg(if command.exit_code == 0 { MINT } else { AMBER }),
        ));
        lines.extend(
            command
                .command
                .lines()
                .map(|l| Line::raw(format!("  {}", clean(l)))),
        );
        if show_output {
            if let Some(output) = &command.output {
                lines.extend(output.lines().map(|line| {
                    Line::styled(
                        format!("  {}", clean(line)),
                        Style::default()
                            .bg(Color::Rgb(24, 24, 24))
                            .fg(if line.starts_with('+') {
                                MINT
                            } else if line.starts_with('-') {
                                AMBER
                            } else {
                                MUTED
                            }),
                    )
                }));
            } else {
                lines.push(Line::styled(
                    "  Output not recorded",
                    Style::default().fg(MUTED),
                ));
            }
        }
        lines.push(Line::default());
    }
    lines.push(Line::default());
    lines.push(Line::default());
}

fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
