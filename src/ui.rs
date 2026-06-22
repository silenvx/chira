use chrono::{DateTime, Local};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, ConfigItem, ConfigState, ConfigSubmode, InputKind, Mode};
use crate::config::Source;
use crate::i18n::{self, ConfigItemHelp};

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
    } else if app.mode == Mode::ActionPick {
        render_action_pick(frame, app, body);
    } else if app.mode == Mode::ConfirmAction {
        render_confirm_action(frame, app, body);
    } else if app.mode == Mode::Help {
        render_help(frame, app, body);
    } else if app.mode == Mode::Config
        && let Some(state) = app.config_state.as_ref()
    {
        render_config(frame, app, state, body);
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
            // 失敗マーカー: 直近の bootstrap が非ゼロ終了で残された半端な dir を見分ける。
            // dir 内に `.chira/bootstrap-failed` が存在する場合のみ表示 (read_entries が判定)。
            let marker = if e.failed {
                Span::styled("[!] ", Style::new().fg(Color::Red).bold())
            } else {
                Span::raw("")
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    when.format("%m/%d %H:%M").to_string(),
                    Style::new().fg(Color::Gray),
                ),
                Span::raw("  "),
                marker,
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

fn render_action_pick(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .actions
        .iter()
        .map(|a| {
            let mut spans = vec![Span::styled(
                a.name.clone(),
                Style::new().fg(Color::Blue).bold(),
            )];
            if let Some(desc) = &a.description {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(desc.clone(), Style::new().fg(Color::Gray)));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let popup = centered(area, 60, app.actions.len() as u16 + 2);
    frame.render_widget(Clear, popup);
    let list = List::new(items)
        .block(
            Block::bordered()
                .title(i18n::action_pick_title(app.lang))
                .border_style(Color::Cyan),
        )
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("› ");
    let mut state = ListState::default();
    if !app.actions.is_empty() {
        state.select(Some(app.action_cursor));
    }
    frame.render_stateful_widget(list, popup, &mut state);
}

/// s を inner_w 桁で折返したときの各行を返す (空行は 1 行として保持)。
/// 概算は char 数ベース (全角は実表示幅で更に折れうるが、ASCII コマンドが主のため許容)。
fn wrap_rows(s: &str, inner_w: usize) -> Vec<String> {
    let mut out = Vec::new();
    for logical in s.split('\n') {
        let chars: Vec<char> = logical.chars().collect();
        if chars.is_empty() {
            out.push(String::new());
        } else {
            for chunk in chars.chunks(inner_w.max(1)) {
                out.push(chunk.iter().collect());
            }
        }
    }
    out
}

fn render_confirm_action(frame: &mut Frame, app: &App, area: Rect) {
    let command = app
        .pending_action()
        .map(|a| a.run.clone())
        .unwrap_or_default();
    let name = app.pending_name();

    // 信頼ゲートはコマンド全文と操作行を必ず見せるのが要件。固定高だと長い dir 名で折返す
    // prompt 行や長い / 複数行 run でコマンド・操作行がクリップされるため、prompt と command
    // 両方の折返し行数からポップアップ高さを算出する。
    // wrap 計算には centered() が後段で行う実クランプ幅 (= 72 と area.width-2 の小さい方)
    // をそのまま使う。max(20) の下駄を wrap に被せると狭い端末で行数を過小評価し操作行が
    // クリップされる (debate-review round3 後の coderabbit / cubic 指摘)。
    let popup_width = 72.min(area.width.saturating_sub(2));
    let inner_w = (popup_width as usize).saturating_sub(2).max(1);
    let prompt_lines = wrap_rows(&i18n::confirm_action_prompt(app.lang, name), inner_w);
    let cmd_lines = wrap_rows(&command, inner_w);
    // 内側 = prompt N + command M + 空行 1 + 操作行 1、border 上下 2 を足し area に cap する
    let height = ((prompt_lines.len() + cmd_lines.len()) as u16 + 4).min(area.height);

    let popup = centered(area, popup_width, height);
    frame.render_widget(Clear, popup);

    let mut text: Vec<Line> = prompt_lines.into_iter().map(Line::raw).collect();
    for l in cmd_lines {
        text.push(Line::from(Span::styled(l, Style::new().fg(Color::Yellow))));
    }
    text.push(Line::raw(""));
    text.push(Line::from(vec![
        Span::styled("y", Style::new().fg(Color::Red).bold()),
        Span::raw(i18n::confirm_action_run_label(app.lang)),
        Span::styled("n/Esc", Style::new().fg(Color::Green).bold()),
        Span::raw(i18n::confirm_cancel_label(app.lang)),
    ]));

    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }).block(
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
        Mode::ActionPick => i18n::footer_action_pick(app.lang),
        Mode::ConfirmAction => i18n::footer_confirm_action(app.lang),
        Mode::Help => i18n::footer_help_close(app.lang),
        Mode::Config => match app
            .config_state
            .as_ref()
            .map(|s| std::mem::discriminant(&s.submode))
        {
            Some(d) if d == std::mem::discriminant(&ConfigSubmode::Edit) => {
                i18n::footer_config_edit(app.lang)
            }
            Some(d) if d == std::mem::discriminant(&ConfigSubmode::KeepEdit { index: 0 }) => {
                i18n::footer_config_edit(app.lang)
            }
            Some(d) if d == std::mem::discriminant(&ConfigSubmode::KeepList { selected: 0 }) => {
                i18n::footer_config_keep(app.lang)
            }
            _ => i18n::footer_config(app.lang),
        },
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

fn render_config(frame: &mut Frame, app: &App, state: &ConfigState, area: Rect) {
    frame.render_widget(Clear, area);

    let [list_area, detail_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(7)]).areas(area);

    // KeepList / KeepEdit 中はメイン pane を keep entries 一覧に差し替え、
    // どの entry が選択中か視覚的に分かるようにする (誤削除/誤編集の防止)
    match state.submode {
        ConfigSubmode::KeepList { selected } | ConfigSubmode::KeepEdit { index: selected } => {
            render_keep_list(frame, app, state, selected, list_area);
        }
        _ => render_config_list(frame, app, state, list_area),
    }
    render_config_detail(frame, app, state, detail_area);

    match state.submode {
        ConfigSubmode::Edit => render_config_input(frame, app, state, area),
        ConfigSubmode::KeepEdit { .. } => render_config_input(frame, app, state, area),
        _ => {}
    }
}

fn render_keep_list(
    frame: &mut Frame,
    app: &App,
    state: &ConfigState,
    selected: usize,
    area: Rect,
) {
    let items: Vec<ListItem> = if state.keep.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            i18n::config_keep_empty(app.lang),
            Style::new().fg(Color::DarkGray),
        )))]
    } else {
        state
            .keep
            .iter()
            .map(|s| ListItem::new(Line::raw(s.clone())))
            .collect()
    };
    let list = List::new(items)
        .block(
            Block::bordered()
                .title(i18n::config_keep_title(app.lang))
                .border_style(Color::Cyan),
        )
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("› ");
    let mut s = ListState::default();
    if !state.keep.is_empty() {
        s.select(Some(selected.min(state.keep.len() - 1)));
    }
    frame.render_stateful_widget(list, area, &mut s);
}

fn render_config_list(frame: &mut Frame, app: &App, state: &ConfigState, area: Rect) {
    let mut items: Vec<ListItem> = Vec::new();
    let mut last_section: Option<&'static str> = None;
    for (i, &item) in ConfigItem::ALL.iter().enumerate() {
        let section = section_of(app.lang, item);
        if last_section != Some(section) {
            items.push(ListItem::new(Line::from(Span::styled(
                format!("  {section}"),
                Style::new().fg(Color::DarkGray).bold(),
            ))));
            last_section = Some(section);
        }
        items.push(ListItem::new(config_row_line(app, state, item, i)));
    }

    let list = List::new(items)
        .block(Block::bordered().title(i18n::config_title(app.lang)))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("› ");
    let mut s = ListState::default();
    s.select(Some(selected_visual_index(state.selected)));
    frame.render_stateful_widget(list, area, &mut s);
}

/// general (3 項目 + 見出し 1) + archive (4 項目 + 見出し 1) のリストで、
/// 論理選択 index (0..7) を visual index に写像する (見出し 2 行を跨ぐため +1/+2)。
fn selected_visual_index(logical: usize) -> usize {
    if logical < 3 {
        1 + logical
    } else {
        1 + 1 + logical
    }
}

fn config_row_line<'a>(
    app: &App,
    state: &'a ConfigState,
    item: ConfigItem,
    _idx: usize,
) -> Line<'a> {
    let help = help_for(app.lang, item);
    let (value_str, source) = value_and_source(state, item);
    let mut spans = vec![
        Span::raw("    "),
        Span::styled(format!("{:<20}", help.label), Style::new().fg(Color::Cyan)),
        Span::styled(format!("{:<32}", truncate(&value_str, 30)), Style::new()),
        Span::raw("  "),
        source_span(app, &source),
    ];
    // env override badge は元の effective source (起動時の優先順位) で判定する。
    // 編集して state.edit に値が入ると source は Config に変わるが、env は依然優先される
    // ため badge を消すと「保存値が有効」とユーザーを誤認させる
    if matches!(effective_source(state, item), Source::Env(_)) {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            i18n::config_env_override_badge(app.lang),
            Style::new().fg(Color::Yellow),
        ));
    }
    Line::from(spans)
}

fn render_config_detail(frame: &mut Frame, app: &App, state: &ConfigState, area: Rect) {
    let item = ConfigItem::ALL[state.selected];
    let help = help_for(app.lang, item);

    let mut lines = vec![
        Line::from(vec![
            Span::styled(help.label, Style::new().fg(Color::Cyan).bold()),
            Span::raw("  "),
            Span::styled(
                format!("[{}]", help.type_hint),
                Style::new().fg(Color::DarkGray),
            ),
        ]),
        Line::raw(help.description.to_string()),
        Line::from(vec![
            Span::styled("Resolution: ", Style::new().fg(Color::DarkGray)),
            Span::raw(help.resolution),
        ]),
    ];
    // env override note も元の effective source で判定 (badge と同じ理由)
    if let Source::Env(var) = effective_source(state, item) {
        lines.push(Line::from(Span::styled(
            i18n::config_env_override_note(app.lang, var),
            Style::new().fg(Color::Yellow),
        )));
    }
    let saves = match state.save_path.as_ref() {
        Some(p) => i18n::config_saves_to(app.lang, &p.display()),
        None => i18n::config_saves_to_unknown(app.lang).into(),
    };
    lines.push(Line::from(Span::styled(
        saves,
        Style::new().fg(Color::DarkGray),
    )));

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::bordered()),
        area,
    );
}

fn render_config_input(frame: &mut Frame, app: &App, state: &ConfigState, area: Rect) {
    let item = ConfigItem::ALL[state.selected];
    let label = match state.submode {
        ConfigSubmode::KeepEdit { .. } => "archive.keep[]",
        _ => help_for(app.lang, item).label,
    };
    let popup = centered(area, 70, 3);
    frame.render_widget(Clear, popup);
    let text = Line::from(vec![
        Span::raw(&app.input),
        Span::styled("▌", Style::new().fg(Color::Cyan)),
    ]);
    frame.render_widget(
        Paragraph::new(text).block(
            Block::bordered()
                .title(i18n::config_input_title(app.lang, label))
                .border_style(Color::Cyan),
        ),
        popup,
    );
}

/// 編集中の項目は source を Config に変える (出力ファイルに書かれる予定の値のため)。
fn value_and_source(state: &ConfigState, item: ConfigItem) -> (String, Source) {
    match item {
        ConfigItem::Dir => match state.edit.dir.clone() {
            Some(v) => (v, Source::Config),
            None => state.effective.dir.clone(),
        },
        ConfigItem::Editor => match state.edit.editor.clone() {
            Some(v) => (v, Source::Config),
            None => state.effective.editor.clone(),
        },
        ConfigItem::Shell => match state.edit.shell.clone() {
            Some(v) => (v, Source::Config),
            None => state.effective.shell.clone(),
        },
        ConfigItem::ArchiveTtlDays => {
            let (v, src) = match state.edit.archive_ttl_days {
                Some(n) => (Some(n), Source::Config),
                None => state.effective.archive_ttl_days.clone(),
            };
            (
                v.map(|n| n.to_string()).unwrap_or_else(|| "(unset)".into()),
                src,
            )
        }
        ConfigItem::ArchiveDir => match state.edit.archive_dir.clone() {
            Some(v) => (v, Source::Config),
            None => state.effective.archive_dir.clone(),
        },
        ConfigItem::ArchiveOnStartup => {
            let (v, src) = match state.edit.archive_on_startup {
                Some(b) => (b, Source::Config),
                None => state.effective.archive_on_startup.clone(),
            };
            (if v { "true".into() } else { "false".into() }, src)
        }
        ConfigItem::ArchiveKeep => {
            let keep = state.keep.clone();
            let src = if state.edit.archive_keep.is_some() {
                Source::Config
            } else {
                state.effective.archive_keep.1.clone()
            };
            let s = if keep.is_empty() {
                "[]".into()
            } else {
                format!("[{}]", keep.join(", "))
            };
            (s, src)
        }
    }
}

/// state.effective から item の元 source (env / config / default) を取り出す。
/// value_and_source は state.edit を含む見かけの source を返すため、
/// env override badge / note の判定にはこちら (起動時の優先順位を反映) を使う。
fn effective_source(state: &ConfigState, item: ConfigItem) -> Source {
    match item {
        ConfigItem::Dir => state.effective.dir.1.clone(),
        ConfigItem::Editor => state.effective.editor.1.clone(),
        ConfigItem::Shell => state.effective.shell.1.clone(),
        ConfigItem::ArchiveTtlDays => state.effective.archive_ttl_days.1.clone(),
        ConfigItem::ArchiveDir => state.effective.archive_dir.1.clone(),
        ConfigItem::ArchiveOnStartup => state.effective.archive_on_startup.1.clone(),
        ConfigItem::ArchiveKeep => state.effective.archive_keep.1.clone(),
    }
}

fn section_of(lang: i18n::Lang, item: ConfigItem) -> &'static str {
    match item {
        ConfigItem::Dir | ConfigItem::Editor | ConfigItem::Shell => {
            i18n::config_section_general(lang)
        }
        _ => i18n::config_section_archive(lang),
    }
}

fn help_for(lang: i18n::Lang, item: ConfigItem) -> ConfigItemHelp {
    match item {
        ConfigItem::Dir => i18n::config_item_dir(lang),
        ConfigItem::Editor => i18n::config_item_editor(lang),
        ConfigItem::Shell => i18n::config_item_shell(lang),
        ConfigItem::ArchiveTtlDays => i18n::config_item_archive_ttl(lang),
        ConfigItem::ArchiveDir => i18n::config_item_archive_dir(lang),
        ConfigItem::ArchiveOnStartup => i18n::config_item_archive_on_startup(lang),
        ConfigItem::ArchiveKeep => i18n::config_item_archive_keep(lang),
    }
}

fn source_span<'a>(app: &App, source: &Source) -> Span<'a> {
    match source {
        Source::Env(var) => Span::styled(
            i18n::config_source_env(app.lang, var),
            Style::new().fg(Color::Yellow),
        ),
        Source::Config => Span::styled(
            i18n::config_source_config(app.lang).to_string(),
            Style::new().fg(Color::Green),
        ),
        Source::Default => Span::styled(
            i18n::config_source_default(app.lang).to_string(),
            Style::new().fg(Color::DarkGray),
        ),
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
