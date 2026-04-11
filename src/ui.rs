use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::app::{App, Branch, Decision};

pub fn render(frame: &mut Frame<'_>, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    let items = app
        .branches
        .iter()
        .map(render_branch)
        .map(ListItem::new)
        .collect::<Vec<_>>();

    let list = List::new(items)
        .block(
            Block::default()
                .title(format!(
                    " git-broom: {} of {} gone branches ",
                    app.delete_count(),
                    app.branches.len()
                ))
                .borders(Borders::ALL),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");

    let mut state = ListState::default();
    if !app.is_empty() {
        state.select(Some(app.selected));
    }

    frame.render_stateful_widget(list, chunks[0], &mut state);

    let footer =
        Paragraph::new("j/k: move  d: delete  s: keep  a: all  u: clear  Enter: go  q: quit");
    frame.render_widget(footer, chunks[1]);
}

fn render_branch(branch: &Branch) -> Line<'static> {
    let marker = match branch.decision {
        Decision::Delete => ("✗", Style::default().fg(Color::Red)),
        Decision::Keep => ("✓", Style::default().fg(Color::Green)),
        Decision::Undecided => ("-", Style::default().fg(Color::DarkGray)),
    };

    let mut spans = vec![
        Span::styled(format!("{} ", marker.0), marker.1),
        Span::raw(pad(&branch.display_name(), 30)),
        Span::styled(
            pad(&branch.relative_date, 12),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(format!("\"{}\"", truncate(&branch.subject, 40))),
    ];

    if branch.is_protected() {
        spans.push(Span::styled(" locked", Style::default().fg(Color::Yellow)));
    }

    Line::from(spans)
}

fn pad(value: &str, width: usize) -> String {
    let visible = value.chars().count();
    if visible >= width {
        return truncate(value, width);
    }

    let mut padded = value.to_string();
    padded.push_str(&" ".repeat(width - visible));
    padded
}

fn truncate(value: &str, width: usize) -> String {
    let visible = value.chars().count();
    if visible <= width {
        return value.to_string();
    }

    value
        .chars()
        .take(width.saturating_sub(1))
        .collect::<String>()
        + "…"
}
