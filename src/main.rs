// Simple TUI todo list manager built on Rust with the Ratatui framework
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
    DefaultTerminal, Frame,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, File};
use std::io::{self};
use std::path::PathBuf;
use chrono::{NaiveDate, NaiveDateTime, Local};

// ---------------------------------------------------------------------------
// Theme & Color Parsing
// ---------------------------------------------------------------------------

fn parse_hex_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        ) {
            return Color::Rgb(r, g, b);
        }
    }
    Color::Reset
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub border: Color,
    pub title: Color,
    pub selected: Color,
    pub completed: Color,
    pub normal_text: Color,
    pub status_checked: Color,
    pub status_unchecked: Color,
    pub mode_adding: Color,
    pub mode_renaming: Color,
    pub mode_deleting: Color,
    pub mode_filtering: Color,
    pub mode_due: Color,
    pub shortcut_hint: Color,
    pub due_date_text: Color,
}

impl Theme {
    pub fn load_or_default() -> Self {
        let config_path = dirs::config_dir().map(|mut path| {
            path.push("todo");
            path.push("theme.json");
            path
        });

        let local_path = PathBuf::from("theme.json");
        let target_path = config_path.filter(|p| p.exists()).unwrap_or(local_path);

        if target_path.exists() {
            if let Ok(content) = fs::read_to_string(&target_path) {
                if let Ok(v) = serde_json::from_str::<Value>(&content) {
                    let get_color = |key: &str, default: &str| -> Color {
                        v.get(key)
                            .and_then(|val| val.as_str())
                            .map(parse_hex_color)
                            .unwrap_or_else(|| parse_hex_color(default))
                    };

                    return Self {
                        border: get_color("border", "#008080"),
                        title: get_color("title", "#70eceb"),
                        selected: get_color("selected", "#ffff00"),
                        completed: get_color("completed", "#555555"),
                        normal_text: get_color("normal_text", "#cfedf0"),
                        status_checked: get_color("status_checked", "#00ff00"),
                        status_unchecked: get_color("status_unchecked", "#20b2aa"),
                        mode_adding: get_color("mode_adding", "#00ff00"),
                        mode_renaming: get_color("mode_renaming", "#70eceb"),
                        mode_deleting: get_color("mode_deleting", "#ff0000"),
                        mode_filtering: get_color("mode_filtering", "#ff00ff"),
                        mode_due: get_color("mode_due", "#ffa500"),
                        shortcut_hint: get_color("shortcut_hint", "#555555"),
                        due_date_text: get_color("due_date_text", "#ff7f50"),
                    };
                }
            }
        }

        Self::default()
    }

    pub fn default() -> Self {
        Self {
            border: Color::Gray,
            title: Color::Reset,
            selected: Color::Yellow,
            completed: Color::DarkGray,
            normal_text: Color::Reset,
            status_checked: Color::Green,
            status_unchecked: Color::Cyan,
            mode_adding: Color::Green,
            mode_renaming: Color::Cyan,
            mode_deleting: Color::Red,
            mode_filtering: Color::Magenta,
            mode_due: Color::Yellow,
            shortcut_hint: Color::DarkGray,
            due_date_text: Color::LightRed,
        }
    }
}

// ---------------------------------------------------------------------------
// App Data Structures
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Todo {
    pub text: String,
    pub completed: bool,
    pub archived: bool,
    pub due_date: Option<String>,
    #[serde(default)]
    pub notified: bool,
}

#[derive(PartialEq)]
pub enum AppMode {
    Normal,
    Filtering,
    Adding,
    Renaming,
    SettingDueDate,
    ConfirmDelete,
    ConfirmQuit,
}

pub struct App {
    pub todos: Vec<Todo>,
    pub selected_index: usize,
    pub search_query: String,
    pub new_todo_query: String,
    pub cursor_position: usize,
    pub mode: AppMode,
    pub theme: Theme,
    pub notifications_enabled: bool,
}

impl App {
    pub fn move_cursor_left(&mut self) {
        self.cursor_position = self.cursor_position.saturating_sub(1);
    }

    pub fn move_cursor_right(&mut self) {
        let max_len = self.new_todo_query.chars().count();
        if self.cursor_position < max_len {
            self.cursor_position += 1;
        }
    }

    pub fn enter_char(&mut self, new_char: char) {
        let index = self
            .new_todo_query
            .char_indices()
            .map(|(i, _)| i)
            .nth(self.cursor_position)
            .unwrap_or(self.new_todo_query.len());

        self.new_todo_query.insert(index, new_char);
        self.cursor_position += 1;
    }

    pub fn delete_char(&mut self) {
        if self.cursor_position != 0 {
            let current_index = self.cursor_position;
            let from_left_to_current = current_index - 1;

            if let Some(byte_idx) = self
                .new_todo_query
                .char_indices()
                .map(|(i, _)| i)
                .nth(from_left_to_current)
            {
                self.new_todo_query.remove(byte_idx);
                self.cursor_position = from_left_to_current;
            }
        }
    }

    fn get_storage_path() -> Option<PathBuf> {
        dirs::config_dir().map(|mut path| {
            path.push("todo");
            path.push("todos.json");
            path
        })
    }

    pub fn load() -> Vec<Todo> {
        if let Some(path) = Self::get_storage_path() {
            if path.exists() {
                if let Ok(file) = File::open(path) {
                    if let Ok(todos) = serde_json::from_reader(file) {
                        return todos;
                    }
                }
            }
        }
        Vec::new()
    }

    pub fn save(&self) -> io::Result<()> {
        if let Some(path) = Self::get_storage_path() {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut temp_path = path.clone();
            temp_path.set_extension("json.tmp");

            let file = File::create(&temp_path)?;
            serde_json::to_writer_pretty(&file, &self.todos)?;

            file.sync_all()?;
            fs::rename(&temp_path, path)?;
        }
        Ok(())
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        let lower_search = self.search_query.to_lowercase();
        self.todos
            .iter()
            .enumerate()
            .filter(|(_, todo)| !todo.archived && todo.text.to_lowercase().contains(&lower_search))
            .map(|(idx, _)| idx)
            .collect()
    }

    pub fn check_notifications(&mut self) {
        if !self.notifications_enabled {
            return;
        }
        let now = Local::now().naive_local();
        for todo in &mut self.todos {
            if !todo.completed && !todo.archived && !todo.notified {
                if let Some(ref date_str) = todo.due_date {
                    let due_parsed = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
                        .map(|d| d.and_hms_opt(0, 0, 0).unwrap())
                        .or_else(|_| NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M"));

                    if let Ok(due_dt) = due_parsed {
                        if now >= due_dt {
                            let _ = notify_rust::Notification::new()
                                .summary("Task Due!")
                                .body(&format!("'{}' is due now!", todo.text))
                                .show();
                            todo.notified = true;
                        }
                    }
                }
            }
        }
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

pub fn ui(frame: &mut Frame, app: &mut App, filtered_indices: &[usize]) {
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.border))
        .title(Span::styled(" To-Do List ", Style::default().fg(app.theme.title)))
        .title_alignment(Alignment::Center);

    let area = outer_block.inner(frame.area());
    frame.render_widget(outer_block, frame.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(2)])
        .split(area);

    let mut list_items = Vec::new();

    for (display_idx, &actual_idx) in filtered_indices.iter().enumerate() {
        let todo = &app.todos[actual_idx];
        let status = if todo.completed { "[x]" } else { "[ ]" };
        let status_color = if todo.completed {
            app.theme.status_checked
        } else {
            app.theme.status_unchecked
        };

        let due_display = if let Some(ref date) = todo.due_date {
            format!(" [Due: {}]", date)
        } else {
            String::new()
        };

        let line_content = if display_idx == app.selected_index {
            let text_style = if todo.completed {
                Style::default().fg(app.theme.completed).add_modifier(Modifier::CROSSED_OUT)
            } else {
                Style::default().fg(app.theme.selected)
            };

            Line::from(vec![
                Span::styled(
                    format!("> {}. ", display_idx + 1),
                    Style::default().fg(app.theme.selected).bold(),
                ),
                Span::styled(format!("{} ", status), Style::default().fg(status_color)),
                Span::styled(&todo.text, text_style),
                Span::styled(due_display, Style::default().fg(app.theme.due_date_text)),
            ])
        } else {
            let text_style = if todo.completed {
                Style::default().fg(app.theme.completed).add_modifier(Modifier::CROSSED_OUT)
            } else {
                Style::default().fg(app.theme.normal_text)
            };

            Line::from(vec![
                Span::styled(
                    format!("  {}. ", display_idx + 1),
                    Style::default().fg(app.theme.normal_text),
                ),
                Span::styled(format!("{} ", status), Style::default().fg(status_color)),
                Span::styled(&todo.text, text_style),
                Span::styled(due_display, Style::default().fg(app.theme.due_date_text)),
            ])
        };

        list_items.push(ListItem::new(line_content));
    }

    let todo_list = List::new(list_items);
    frame.render_widget(todo_list, chunks[0]);

    let mut scrollbar_state = ScrollbarState::new(filtered_indices.len()).position(app.selected_index);
    frame.render_stateful_widget(
        Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"))
            .style(Style::default().fg(app.theme.border)),
        chunks[0],
        &mut scrollbar_state,
    );

    let notif_status = if app.notifications_enabled { "ON" } else { "OFF" };
    let prompt_text = match app.mode {
        AppMode::Filtering => {
            let max_width = chunks[1].width.saturating_sub(10) as usize;
            let query_len = app.search_query.chars().count();
            let scrolled_query: String = if query_len >= max_width {
                app.search_query.chars().skip(query_len - max_width + 1).collect()
            } else {
                app.search_query.clone()
            };
            Line::from(vec![
                Span::styled("Filter: ", Style::default().fg(app.theme.mode_filtering).bold()),
                Span::raw(scrolled_query),
            ])
        }
        AppMode::Adding => Line::from(vec![
            Span::styled(" Adding item... ", Style::default().fg(app.theme.mode_adding).bold()),
        ]),
        AppMode::Renaming => Line::from(vec![
            Span::styled(" Renaming item...", Style::default().fg(app.theme.mode_renaming).bold()),
        ]),
        AppMode::SettingDueDate => Line::from(vec![
            Span::styled(" Set Due Date (YYYY-MM-DD): ", Style::default().fg(app.theme.mode_due).bold()),
        ]),
        AppMode::ConfirmDelete => Line::from(vec![
            Span::styled(" Deleting item... ", Style::default().fg(app.theme.mode_deleting).bold()),
        ]),
        AppMode::ConfirmQuit => Line::from(vec![
            Span::styled(" Quitting... ", Style::default().fg(app.theme.mode_deleting).bold()),
        ]),
        AppMode::Normal => Line::from(vec![
            Span::styled(
                format!(" [j/k] Nav | [space] Toggle | [i] Add | [r] Rename | [D] Due | [N] Notifs: {} | [q] Quit", notif_status),
                Style::default().fg(app.theme.shortcut_hint),
            ),
        ]),
    };

    let hint_block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(app.theme.border));

    let prompt = Paragraph::new(prompt_text).block(hint_block);
    frame.render_widget(prompt, chunks[1]);

    match app.mode {
        AppMode::Adding | AppMode::Renaming | AppMode::SettingDueDate => {
            let popup_area = centered_rect(60, 20, frame.area());
            frame.render_widget(Clear, popup_area);

            let (title, color) = match app.mode {
                AppMode::Adding => (" Add New Task ", app.theme.mode_adding),
                AppMode::Renaming => (" Rename Task ", app.theme.mode_renaming),
                _ => (" Set Due Date (YYYY-MM-DD) ", app.theme.mode_due),
            };

            let popup_block = Block::default()
                .title(Span::styled(title, Style::default().fg(color)))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color));

            let max_input_width = popup_area.width.saturating_sub(2) as usize;
            let skip_count = if app.cursor_position >= max_input_width {
                app.cursor_position - max_input_width + 1
            } else {
                0
            };

            let viewable_text: String = app
                .new_todo_query
                .chars()
                .skip(skip_count)
                .take(max_input_width)
                .collect();

            let popup_paragraph = Paragraph::new(viewable_text.as_str()).block(popup_block);
            frame.render_widget(popup_paragraph, popup_area);

            let visual_cursor_offset = (app.cursor_position - skip_count) as u16;
            let cursor_x = popup_area.x + 1 + visual_cursor_offset;
            let cursor_y = popup_area.y + 1;
            frame.set_cursor_position(Position::new(cursor_x, cursor_y));
        }
        AppMode::ConfirmDelete | AppMode::ConfirmQuit => {
            let popup_area = centered_rect(50, 25, frame.area());
            frame.render_widget(Clear, popup_area);

            let prompt_msg = if app.mode == AppMode::ConfirmDelete {
                "Are you sure you want to delete?"
            } else {
                "Are you sure you want to quit?"
            };

            let popup_block = Block::default()
                .title(Span::styled(" Confirmation ", Style::default().fg(app.theme.mode_deleting)))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(app.theme.mode_deleting));

            let message = vec![
                Line::from(""),
                Line::from(prompt_msg.bold()),
                Line::from("  [y] Yes  |  [n] Cancel  ".fg(app.theme.shortcut_hint)),
            ];

            let popup_paragraph = Paragraph::new(message)
                .alignment(Alignment::Center)
                .block(popup_block);

            frame.render_widget(popup_paragraph, popup_area);
        }
        AppMode::Filtering => {
            let max_width = chunks[1].width.saturating_sub(10) as usize;
            let query_len = app.search_query.chars().count();
            let cursor_offset = if query_len >= max_width { max_width - 1 } else { query_len };

            let cursor_x = chunks[1].x + 8 + cursor_offset as u16;
            let cursor_y = chunks[1].y + 1;
            frame.set_cursor_position(Position::new(cursor_x, cursor_y));
        }
        _ => {}
    }
}

fn main() -> io::Result<()> {
    let mut terminal = ratatui::init();

    let mut app = App {
        todos: App::load(),
        selected_index: 0,
        search_query: String::new(),
        new_todo_query: String::new(),
        cursor_position: 0,
        mode: AppMode::Normal,
        theme: Theme::load_or_default(),
        notifications_enabled: true,
    };

    let result = run_app(&mut terminal, &mut app);

    ratatui::restore();
    result
}

fn run_app(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        let filtered_indices = app.filtered_indices();
        let filtered_len = filtered_indices.len();

        if filtered_len == 0 {
            app.selected_index = 0;
        } else if app.selected_index >= filtered_len {
            app.selected_index = filtered_len - 1;
        }

        terminal.draw(|f| ui(f, app, &filtered_indices))?;

        if event::poll(std::time::Duration::from_secs(1))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let max_index = filtered_len.saturating_sub(1);

                    match app.mode {
                        AppMode::Filtering => match key.code {
                            KeyCode::Enter | KeyCode::Esc => app.mode = AppMode::Normal,
                            KeyCode::Backspace => {
                                app.search_query.pop();
                            }
                            KeyCode::Char(c) => {
                                app.search_query.push(c);
                            }
                            _ => {}
                        },
                        AppMode::Adding | AppMode::Renaming | AppMode::SettingDueDate => match key.code {
                            KeyCode::Esc => {
                                app.new_todo_query.clear();
                                app.cursor_position = 0;
                                app.mode = AppMode::Normal;
                            }
                            KeyCode::Enter => {
                                let query_val = app.new_todo_query.trim().to_string();
                                if app.mode == AppMode::SettingDueDate {
                                    if let Some(&actual_idx) = filtered_indices.get(app.selected_index) {
                                        app.todos[actual_idx].due_date = if query_val.is_empty() {
                                            None
                                        } else {
                                            Some(query_val)
                                        };
                                        app.todos[actual_idx].notified = false;
                                        app.save()?;
                                    }
                                } else if !query_val.is_empty() {
                                    if app.mode == AppMode::Adding {
                                        app.todos.push(Todo {
                                            text: query_val,
                                            completed: false,
                                            archived: false,
                                            due_date: None,
                                            notified: false,
                                        });
                                        app.selected_index = app.filtered_indices().len().saturating_sub(1);
                                    } else if app.mode == AppMode::Renaming {
                                        if let Some(&actual_idx) = filtered_indices.get(app.selected_index) {
                                            app.todos[actual_idx].text = query_val;
                                        }
                                    }
                                    app.save()?;
                                }
                                app.new_todo_query.clear();
                                app.cursor_position = 0;
                                app.mode = AppMode::Normal;
                            }
                            KeyCode::Left => app.move_cursor_left(),
                            KeyCode::Right => app.move_cursor_right(),
                            KeyCode::Home => app.cursor_position = 0,
                            KeyCode::End => app.cursor_position = app.new_todo_query.chars().count(),
                            KeyCode::Backspace => app.delete_char(),
                            KeyCode::Char(c) => app.enter_char(c),
                            _ => {}
                        },
                        AppMode::ConfirmDelete => match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                                if let Some(&actual_idx) = filtered_indices.get(app.selected_index) {
                                    app.todos.remove(actual_idx);
                                    app.save()?;
                                }
                                app.mode = AppMode::Normal;
                            }
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                app.mode = AppMode::Normal;
                            }
                            _ => {}
                        },
                        AppMode::ConfirmQuit => match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => break,
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                app.mode = AppMode::Normal;
                            }
                            _ => {}
                        },
                        AppMode::Normal => match key.code {
                            KeyCode::Char('q') => {
                                app.mode = AppMode::ConfirmQuit;
                            }
                            KeyCode::Char('N') => {
                                app.notifications_enabled = !app.notifications_enabled;
                            }
                            KeyCode::Char('j') | KeyCode::Down => {
                                if filtered_len > 0 && app.selected_index < max_index {
                                    app.selected_index += 1;
                                }
                            }
                            KeyCode::Char('k') | KeyCode::Up => {
                                if app.selected_index > 0 {
                                    app.selected_index -= 1;
                                }
                            }
                            KeyCode::Char('J') => {
                                if filtered_len > 0 && app.selected_index < max_index {
                                    let current_actual_idx = filtered_indices[app.selected_index];
                                    let target_actual_idx = filtered_indices[app.selected_index + 1];

                                    app.todos.swap(current_actual_idx, target_actual_idx);
                                    app.selected_index += 1;
                                    app.save()?;
                                }
                            }
                            KeyCode::Char('K') => {
                                if filtered_len > 0 && app.selected_index > 0 {
                                    let current_actual_idx = filtered_indices[app.selected_index];
                                    let target_actual_idx = filtered_indices[app.selected_index - 1];

                                    app.todos.swap(current_actual_idx, target_actual_idx);
                                    app.selected_index -= 1;
                                    app.save()?;
                                }
                            }
                            KeyCode::Char(' ') => {
                                if let Some(&actual_idx) = filtered_indices.get(app.selected_index) {
                                    app.todos[actual_idx].completed = !app.todos[actual_idx].completed;
                                    app.save()?;
                                }
                            }
                            KeyCode::Char('/') => {
                                app.mode = AppMode::Filtering;
                            }
                            KeyCode::Char('i') => {
                                app.new_todo_query.clear();
                                app.cursor_position = 0;
                                app.mode = AppMode::Adding;
                            }
                            KeyCode::Char('r') => {
                                if let Some(&actual_idx) = filtered_indices.get(app.selected_index) {
                                    app.new_todo_query = app.todos[actual_idx].text.clone();
                                    app.cursor_position = app.new_todo_query.chars().count();
                                    app.mode = AppMode::Renaming;
                                }
                            }
                            KeyCode::Char('D') => {
                                if let Some(&actual_idx) = filtered_indices.get(app.selected_index) {
                                    app.new_todo_query = app.todos[actual_idx].due_date.clone().unwrap_or_default();
                                    app.cursor_position = app.new_todo_query.chars().count();
                                    app.mode = AppMode::SettingDueDate;
                                }
                            }
                            KeyCode::Char('d') => {
                                if filtered_len > 0 {
                                    app.mode = AppMode::ConfirmDelete;
                                }
                            }
                            _ => {}
                        },
                    }
                }
            }
        } else {
            app.check_notifications();
        }
    }
    Ok(())
}
