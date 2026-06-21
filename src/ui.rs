use chrono::{DateTime, Local};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, InputKind, Mode};
use crate::i18n;

pub fn render(frame: &mut Frame, app: &App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    render_header(frame, app, header);
    render_browse(frame, app, body);
    render_footer(frame, app, footer);

    if let Mode::Input(kind) = app.mode {
        render_input(frame, app, kind, body);
    } else if app.mode == Mode::ConfirmDelete {
        render_confirm(frame, app, body);
    } else if app.mode == Mode::Help {
        render_help(frame, app, body);
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled(
            " chira ",
            Style::new().bg(Color::Cyan).fg(Color::Black).bold(),
        ),
        Span::raw(format!(" {}  ", app.rel_path())),
        Span::styled(
            i18n::header_count(app.lang, app.visible().len()),
            Style::new().fg(Color::Gray),
        ),
    ];
    if app.mode == Mode::Search || !app.search.is_empty() {
        let cursor = if app.mode == Mode::Search { "▌" } else { "" };
        spans.push(Span::raw("   "));
        spans.push(Span::styled(
            i18n::header_search(app.lang, &app.search, cursor),
            Style::new().fg(Color::Yellow),
        ));
    }
    frame.render_widget(Line::from(spans), area);
}

fn render_browse(frame: &mut Frame, app: &App, area: Rect) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(45), Constraint::Percentage(55)]).areas(area);

    let visible = app.visible();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|&i| {
            let e = &app.entries[i];
            let when: DateTime<Local> = e.modified.into();
            let name = if e.is_dir {
                Span::styled(format!("{}/", e.name), Style::new().fg(Color::Blue).bold())
            } else {
                Span::raw(e.name.clone())
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    when.format("%m/%d %H:%M").to_string(),
                    Style::new().fg(Color::Gray),
                ),
                Span::raw("  "),
                name,
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(Block::bordered().title(i18n::list_title(app.lang)))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("› ");
    let mut state = ListState::default();
    if !visible.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, left, &mut state);

    let title = match app.selected_entry() {
        Some(e) if e.is_dir => i18n::preview_dir_title(app.lang),
        Some(_) => i18n::preview_file_title(app.lang),
        None => i18n::preview_file_title(app.lang),
    };
    let preview = if app.preview.is_empty() && app.selected_entry().is_none() {
        let hint = if app.search.is_empty() {
            i18n::empty_hint(app.lang)
        } else {
            i18n::empty_search_hint(app.lang)
        };
        Paragraph::new(hint).style(Style::new().fg(Color::Gray))
    } else {
        Paragraph::new(app.preview.as_str()).wrap(Wrap { trim: false })
    };
    frame.render_widget(preview.block(Block::bordered().title(title)), right);
}

fn render_input(frame: &mut Frame, app: &App, kind: InputKind, area: Rect) {
    let title = match kind {
        InputKind::NewFile => i18n::input_title_new_file(app.lang),
        InputKind::NewDir => i18n::input_title_new_dir(app.lang),
        InputKind::Rename => i18n::input_title_rename(app.lang),
    };
    let popup = centered(area, 60, 3);
    frame.render_widget(Clear, popup);
    let text = Line::from(vec![
        Span::raw(&app.input),
        Span::styled("▌", Style::new().fg(Color::Cyan)),
    ]);
    frame.render_widget(
        Paragraph::new(text).block(Block::bordered().title(title).border_style(Color::Cyan)),
        popup,
    );
}

fn render_confirm(frame: &mut Frame, app: &App, area: Rect) {
    let (name, is_dir) = app
        .selected_entry()
        .map(|e| (e.name.clone(), e.is_dir))
        .unwrap_or_default();
    let popup = centered(area, 54, 4);
    frame.render_widget(Clear, popup);
    let what = if is_dir {
        i18n::confirm_delete_dir(app.lang, &truncate(&name, 26))
    } else {
        i18n::confirm_delete_file(app.lang, &truncate(&name, 30))
    };
    let text = vec![
        Line::raw(what),
        Line::from(vec![
            Span::styled("y", Style::new().fg(Color::Red).bold()),
            Span::raw(i18n::confirm_delete_label(app.lang)),
            Span::styled("n/Esc", Style::new().fg(Color::Green).bold()),
            Span::raw(i18n::confirm_cancel_label(app.lang)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(text).block(
            Block::bordered()
                .title(i18n::confirm_title(app.lang))
                .border_style(Color::Red),
        ),
        popup,
    );
}

fn render_help(frame: &mut Frame, app: &App, area: Rect) {
    let keys = i18n::help_rows(app.lang);
    let lines: Vec<Line> = keys
        .iter()
        .map(|(k, desc)| {
            Line::from(vec![
                Span::styled(format!("  {k:<16}"), Style::new().fg(Color::Cyan).bold()),
                Span::raw(*desc),
            ])
        })
        .collect();

    let popup = centered(area, 56, keys.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(i18n::help_title(app.lang))),
        popup,
    );
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let help = match app.mode {
        Mode::Browse => i18n::footer_browse(app.lang),
        Mode::Search => i18n::footer_search(app.lang),
        Mode::Input(_) => i18n::footer_input(app.lang),
        Mode::ConfirmDelete => i18n::footer_confirm(app.lang),
        Mode::Help => i18n::footer_help_close(app.lang),
    };
    let line = if app.status.is_empty() {
        Line::from(Span::styled(help, Style::new().fg(Color::Gray)))
    } else {
        Line::from(vec![
            Span::styled(&app.status, Style::new().fg(Color::Green)),
            Span::raw("   "),
            Span::styled(help, Style::new().fg(Color::Gray)),
        ])
    };
    frame.render_widget(line, area);
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width.saturating_sub(2));
    let h = height.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(w)) / 2,
        y: area.y + (area.height.saturating_sub(h)) / 2,
        width: w,
        height: h,
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars).collect();
        out.push('…');
        out
    }
}
