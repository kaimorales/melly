//! Shared terminal palette and small, bounded native-cell animations.
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::{sync::OnceLock, time::Instant};

pub const BG: Color = Color::Rgb(12, 13, 15);
pub const PANEL: Color = Color::Rgb(23, 25, 28);
pub const SIDEBAR: Color = Color::Rgb(17, 19, 21);
pub const INK: Color = Color::Rgb(233, 232, 228);
pub const MUTED: Color = Color::Rgb(149, 152, 159);
pub const FAINT: Color = Color::Rgb(93, 98, 106);
pub const BORDER: Color = Color::Rgb(44, 48, 54);
pub const SELECTED: Color = Color::Rgb(33, 37, 42);
pub const ACCENT: Color = Color::Rgb(210, 183, 240);
pub const MINT: Color = Color::Rgb(140, 219, 188);
pub const AMBER: Color = Color::Rgb(234, 188, 110);
pub const INTRO_MS: u128 = 600;

pub fn hints(items: &[(&str, &str)]) -> Line<'static> {
    let mut spans = vec![Span::raw("  ")];
    for (index, (key, label)) in items.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("   "));
        }
        spans.push(Span::styled((*key).to_owned(), Style::default().fg(INK)));
        spans.push(Span::styled(
            format!(" {label}"),
            Style::default().fg(MUTED),
        ));
    }
    Line::from(spans)
}

/// Every busy indicator uses the same monotonic clock, including worker views.
pub fn time_ms() -> u128 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis()
}

pub fn beat(ms: u128, motion: bool) -> &'static str {
    if !motion {
        return "∙";
    }
    ["∙", "•", "●", "•"][(ms / 160 % 4) as usize]
}

/// A three-note activity signal. It indicates work, never estimated progress.
pub fn signal(ms: u128, motion: bool, accent: Color) -> Vec<Span<'static>> {
    (0..3)
        .map(|i| {
            let phase = (ms % 1600) as f64 / 1600.0 * std::f64::consts::TAU - i as f64 * 0.9;
            let weight = ((phase.cos() + 1.0) / 2.0).powi(3);
            Span::styled(
                if motion && weight > 0.45 {
                    "●"
                } else {
                    "∙"
                },
                Style::default().fg(if motion {
                    blend(FAINT, accent, weight)
                } else {
                    accent
                }),
            )
        })
        .collect()
}

fn blend(from: Color, to: Color, weight: f64) -> Color {
    match (from, to) {
        (Color::Rgb(r, g, b), Color::Rgb(x, y, z)) => {
            let mix = |a: u8, b: u8| {
                (a as f64 + (b as f64 - a as f64) * weight.clamp(0.0, 1.0)).round() as u8
            };
            Color::Rgb(mix(r, x), mix(g, y), mix(b, z))
        }
        _ => to,
    }
}

pub fn meter(used: Option<u64>, total: Option<u64>, width: usize, accent: Color) -> Line<'static> {
    let Some((used, total)) = used.zip(total).filter(|(_, t)| *t > 0) else {
        return Line::styled("─".repeat(width), Style::default().fg(BORDER));
    };
    let filled = ((used.min(total) as u128 * width as u128) / total as u128) as usize;
    Line::from(vec![
        Span::styled("━".repeat(filled), Style::default().fg(accent)),
        Span::styled("─".repeat(width - filled), Style::default().fg(BORDER)),
    ])
}

// Original pixel lettering. Two vertical pixels occupy one terminal cell.
const LETTERS: [&[&str]; 5] = [
    &[
        "00000", "00000", "11011", "10101", "10101", "10101", "10101", "00000", "00000",
    ],
    &[
        "00000", "00000", "01110", "10001", "11111", "10000", "01111", "00000", "00000",
    ],
    &[
        "110", "010", "010", "010", "010", "010", "001", "000", "000",
    ],
    &[
        "110", "010", "010", "010", "010", "010", "001", "000", "000",
    ],
    &[
        "00000", "00000", "10001", "10001", "10001", "01111", "00001", "10001", "01110",
    ],
];

pub fn wordmark(frame: &mut Frame, area: Rect, accent: Color, elapsed: u128, motion: bool) {
    if area.height < 5 || area.width < 32 {
        frame.render_widget(
            Paragraph::new("melly")
                .style(Style::default().fg(INK))
                .alignment(ratatui::layout::Alignment::Center),
            area,
        );
        return;
    }
    let scale = if area.width >= 60 { 2 } else { 1 };
    let width = 29 * scale + 1;
    let left = area.x + (area.width - width) / 2;
    let mut pixels = vec![vec![BG; width as usize]; 10];
    let mut x = 0;
    for bitmap in LETTERS {
        for (y, row) in bitmap.iter().enumerate() {
            for (column, bit) in row.bytes().enumerate() {
                if bit != b'1' {
                    continue;
                }
                for dx in 0..scale as usize {
                    let px = x + column * scale as usize + dx;
                    pixels[y + 1][px + 1] = BORDER;
                }
            }
        }
        x += (bitmap[0].len() + 2) * scale as usize;
    }
    x = 0;
    for (letter, bitmap) in LETTERS.iter().enumerate() {
        for (y, row) in bitmap.iter().enumerate() {
            for (column, bit) in row.bytes().enumerate() {
                if bit != b'1' {
                    continue;
                }
                let lit = !motion || elapsed >= 60 + letter as u128 * 85 + y as u128 * 10;
                let color = if !lit {
                    FAINT
                } else if letter == 0 {
                    accent
                } else {
                    INK
                };
                for dx in 0..scale as usize {
                    pixels[y][x + column * scale as usize + dx] = color;
                }
            }
        }
        x += (bitmap[0].len() + 2) * scale as usize;
    }
    for (y, rows) in pixels.as_chunks::<2>().0.iter().enumerate() {
        for (x, (&top, &bottom)) in rows[0].iter().zip(&rows[1]).enumerate() {
            let cell = &mut frame.buffer_mut()[(left + x as u16, area.y + y as u16)];
            cell.set_symbol("▀").set_fg(top).set_bg(bottom);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::tests::capture;
    use ratatui::{Terminal, backend::TestBackend, widgets::Block};

    #[test]
    fn logo_settles_and_reduced_motion_is_immediate_at_every_size() {
        for width in [24, 40, 80, 120] {
            let render = |ms, motion| {
                let mut terminal = Terminal::new(TestBackend::new(width, 10)).unwrap();
                terminal
                    .draw(|f| {
                        f.render_widget(Block::default().style(Style::default().bg(BG)), f.area());
                        wordmark(f, Rect::new(0, 2, width, 5), ACCENT, ms, motion);
                    })
                    .unwrap();
                terminal
            };
            let settled = render(INTRO_MS, true);
            assert_eq!(
                settled.backend().buffer(),
                render(INTRO_MS * 3, true).backend().buffer()
            );
            assert_eq!(
                settled.backend().buffer(),
                render(0, false).backend().buffer()
            );
            if width == 80 {
                capture(&settled, "wordmark");
            }
        }
        assert_eq!(signal(0, false, ACCENT), signal(400, false, ACCENT));
        assert_ne!(signal(0, true, ACCENT), signal(200, true, ACCENT));
    }
}
