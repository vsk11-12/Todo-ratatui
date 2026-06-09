// Simple TUI todo list manager built on Rust with the Ratatui framework
// Inspired by the togo app on aur
// Built and maintained by vsk11-12

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self};
use std::path::PathBuf;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    DefaultTerminal, Frame,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Todo {
    pub text: String,
    pub completed: bool,
    pub archived: bool,
}

#[derive(PartialEq)]
pub enum AppMode {
    Normal,
    Filtering,
    Adding,
    Renaming,
    ConfirmDelete,
}

pub struct App {
    pub todos: Vec<Todo>,
    pub selected_index: usize,
    pub search_query: String,
    pub new_todo_query: String,
    pub mode: AppMode,
}

impl App {
    // Resolves data file path to: ~/.config/togo/todos.json
    fn get_storage_path() -> Option<PathBuf> {
        dirs::config_dir().map(|mut path| {
            path.push("togo");
            path.push("todos.json");
            path
        })
    }

    // Loads state from the disk
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

    // Saves state back to the disk atomically to prevent corruption
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

    // Filters visible items based on search queries
    pub fn filtered_indices(&self) -> Vec<usize> {
        let lower_search = self.search_query.to_lowercase();
        self.todos
            .iter()
            .enumerate()
            .filter(|(_, todo)| {
                !todo.archived && todo.text.to_lowercase().contains(&lower_search)
            })
            .map(|(idx, _)| idx)
            .collect()
    }
}

/// Helper function to create a centered rect layout for popups
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
    // 1. App-wide outer boundary block
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .title(" To-Do List ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));

    let area = outer_block.inner(frame.area());
    frame.render_widget(outer_block, frame.area());

    // 2. Split inner content space (allocating exactly 2 rows at the bottom for instructions)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    
            Constraint::Length(2), 
        ])
        .split(area);

    let mut list_items = Vec::new();

    for (display_idx, &actual_idx) in filtered_indices.iter().enumerate() {
        let todo = &app.todos[actual_idx];
        let status = if todo.completed { "[x]" } else { "[ ]" };
        let mut text_style = Style::default();
        
        if todo.completed {
            text_style = text_style.fg(Color::DarkGray).add_modifier(Modifier::CROSSED_OUT);
        }

        let line_content = if display_idx == app.selected_index {
            Line::from(vec![
                Span::styled(format!("> {}. ", display_idx + 1), Style::default().fg(Color::Yellow).bold()),
                Span::styled(format!("{} ", status), Style::default().fg(Color::Green)),
                Span::styled(&todo.text, text_style.fg(Color::Yellow)),
            ])
        } else {
            Line::from(vec![
                Span::raw(format!("  {}. ", display_idx + 1)),
                Span::styled(format!("{} ", status), Style::default().fg(Color::Cyan)),
                Span::styled(&todo.text, text_style),
            ])
        };

        list_items.push(ListItem::new(line_content));
    }

    let todo_list = List::new(list_items);
    frame.render_widget(todo_list, chunks[0]);

    // Render a scrollbar if items go beyond viewport bounds
    let mut scrollbar_state = ScrollbarState::new(filtered_indices.len()).position(app.selected_index);
    frame.render_stateful_widget(
        Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓")),
        chunks[0],
        &mut scrollbar_state,
    );

    // 3. Render context action prompts dynamically at the bottom status bar
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
                Span::styled("Filter: ", Style::default().fg(Color::Magenta).bold()),
                Span::raw(scrolled_query),
            ])
        }
        AppMode::Adding => Line::from(vec![
            Span::styled(" Adding item... ", Style::default().fg(Color::Green).bold()),
        ]),
        AppMode::Renaming => Line::from(vec![
            Span::styled(" Renaming item...", Style::default().fg(Color::Cyan).bold()),
        ]),
        AppMode::ConfirmDelete => Line::from(vec![
            Span::styled(" Deleting item... ", Style::default().fg(Color::Red).bold()),
        ]),
        AppMode::Normal => Line::from(vec![
            Span::styled(" [j/k] Nav | [Shift+J/K] Move | [space] Toggle | [i] Add | [r] Rename | [d] Delete | [/] Filter | [q] Quit", Style::default().fg(Color::DarkGray)),
        ]),
    };

    // 4. Horizontal layout separator line
    let hint_block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));

    let prompt = Paragraph::new(prompt_text).block(hint_block);
    frame.render_widget(prompt, chunks[1]);

    // 5. Render Modal Popups
    match app.mode {
        AppMode::Adding | AppMode::Renaming => {
            let popup_area = centered_rect(60, 20, frame.area());
            frame.render_widget(Clear, popup_area);

            let (title, color) = if app.mode == AppMode::Adding {
                (" Add New Task ", Color::Green)
            } else {
                (" Rename Task ", Color::Cyan)
            };

            let popup_block = Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color));
            
            // Calculate a horizontal scrolling sliding-window for text boxes
            let max_input_width = popup_area.width.saturating_sub(2) as usize;
            let current_len = app.new_todo_query.chars().count();
            let viewable_text: String = if current_len >= max_input_width {
                app.new_todo_query.chars().skip(current_len - max_input_width + 1).collect()
            } else {
                app.new_todo_query.clone()
            };

            let popup_paragraph = Paragraph::new(viewable_text.as_str()).block(popup_block);
            frame.render_widget(popup_paragraph, popup_area);

            // Dynamically clip cursor position safely inside boundaries
            let cursor_x = popup_area.x + 1 + viewable_text.chars().count() as u16;
            let cursor_y = popup_area.y + 1;
            frame.set_cursor_position(Position::new(cursor_x, cursor_y));
        }
        AppMode::ConfirmDelete => {
            let popup_area = centered_rect(50, 25, frame.area());
            frame.render_widget(Clear, popup_area);

            let popup_block = Block::default()
                .title(" Confirmation ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red));

            let message = vec![
                Line::from(""),
                Line::from("Are you sure you want to delete?".bold()),
                Line::from(""),
                Line::from("  [y] Yes  |  [n] Cancel  ".dark_gray()),
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
        mode: AppMode::Normal,
    };

    // Execute application loop logic wrapped inside error isolation
    let result = run_app(&mut terminal, &mut app);

    // Always clean up raw terminal state back to the user shell regardless of app crashes
    ratatui::restore();
    result
}

fn run_app(terminal: &mut DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        // Cache filtered list indexing configurations once per rendering pass
        let filtered_indices = app.filtered_indices();
        let filtered_len = filtered_indices.len();
        
        if filtered_len == 0 {
            app.selected_index = 0;
        } else if app.selected_index >= filtered_len {
            app.selected_index = filtered_len - 1;
        }

        terminal.draw(|f| ui(f, app, &filtered_indices))?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                let max_index = filtered_len.saturating_sub(1);

                match app.mode {
                    AppMode::Filtering => match key.code {
                        KeyCode::Enter | KeyCode::Esc => app.mode = AppMode::Normal,
                        KeyCode::Backspace => { app.search_query.pop(); }
                        KeyCode::Char(c) => { app.search_query.push(c); }
                        _ => {}
                    },
                    AppMode::Adding => match key.code {
                        KeyCode::Esc => {
                            app.new_todo_query.clear();
                            app.mode = AppMode::Normal;
                        }
                        KeyCode::Enter => {
                            if !app.new_todo_query.trim().is_empty() {
                                app.todos.push(Todo {
                                    text: app.new_todo_query.trim().to_string(),
                                    completed: false,
                                    archived: false,
                                });
                                app.new_todo_query.clear();
                                app.mode = AppMode::Normal;
                                // Automatically jump focus to the newly appended item
                                app.selected_index = app.filtered_indices().len().saturating_sub(1);
                                app.save()?;
                            }
                        }
                        KeyCode::Backspace => { app.new_todo_query.pop(); }
                        KeyCode::Char(c) => { app.new_todo_query.push(c); }
                        _ => {}
                    },
                    AppMode::Renaming => match key.code {
                        KeyCode::Esc => {
                            app.new_todo_query.clear();
                            app.mode = AppMode::Normal;
                        }
                        KeyCode::Enter => {
                            if !app.new_todo_query.trim().is_empty() {
                                if let Some(&actual_idx) = filtered_indices.get(app.selected_index) {
                                    app.todos[actual_idx].text = app.new_todo_query.trim().to_string();
                                    app.save()?;
                                }
                                app.new_todo_query.clear();
                                app.mode = AppMode::Normal;
                            }
                        }
                        KeyCode::Backspace => { app.new_todo_query.pop(); }
                        KeyCode::Char(c) => { app.new_todo_query.push(c); }
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
                    AppMode::Normal => match key.code {
                        KeyCode::Char('q') => break,
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
                        // Shift + J to move items down
                        KeyCode::Char('J') => {
                            if filtered_len > 0 && app.selected_index < max_index {
                                let current_actual_idx = filtered_indices[app.selected_index];
                                let target_actual_idx = filtered_indices[app.selected_index + 1];
                                
                                app.todos.swap(current_actual_idx, target_actual_idx);
                                app.selected_index += 1;
                                app.save()?;
                            }
                        }
                        // Shift + K to move items up
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
                            app.mode = AppMode::Adding;
                        }
                        KeyCode::Char('r') => {
                            if let Some(&actual_idx) = filtered_indices.get(app.selected_index) {
                                app.new_todo_query = app.todos[actual_idx].text.clone();
                                app.mode = AppMode::Renaming;
                            }
                        }
                        KeyCode::Char('d') => {
                            if filtered_len > 0 {
                                app.mode = AppMode::ConfirmDelete;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(())
}
