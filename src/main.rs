//This is a Simple TUI todo list manager based on rust built on the ratatui framework and  is inspired by the togo app available on aur
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{self};
use std::path::PathBuf;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Position},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
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
    Renaming, // Added for renaming feature
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
    // Resolves data file path to: ~/.config/todo/todos.json
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

            // 1. Create a temporary file path in the same directory
            let mut temp_path = path.clone();
            temp_path.set_extension("json.tmp");

            // 2. Open and write to the temporary file
            let file = File::create(&temp_path)?;
            serde_json::to_writer_pretty(&file, &self.todos)?;

            // 3. Ensure all OS buffers are fully flushed to actual hardware storage
            file.sync_all()?;

            // 4. Atomically rename the temporary file to the target path
            fs::rename(&temp_path, path)?;
        }
        Ok(())
    }

    // Filters visible items based on search queries
    pub fn filtered_indices(&self) -> Vec<usize> {
        self.todos
            .iter()
            .enumerate()
            .filter(|(_, todo)| {
                !todo.archived && 
                todo.text.to_lowercase().contains(&self.search_query.to_lowercase())
            })
            .map(|(idx, _)| idx)
            .collect()
    }
}

pub fn ui(frame: &mut Frame, app: &mut App) {
    // 1. App-wide outer boundary block with centered title string
    let outer_block = Block::default()
        .borders(Borders::ALL)
        .title(" To-Do List ")
        .title_alignment(Alignment::Center)
        .style(Style::default().fg(Color::Gray));

    let area = outer_block.inner(frame.area());
    frame.render_widget(outer_block, frame.area());

    // 2. Split inner content space (leaving exactly 2 rows at the bottom for hints)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    
            Constraint::Length(2), 
        ])
        .split(area);

    let filtered_indices = app.filtered_indices();
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

    // 3. Render contextual context action prompts dynamically
    let prompt_text = match app.mode {
        AppMode::Filtering => Line::from(vec![
            Span::styled("Filter: ", Style::default().fg(Color::Magenta).bold()),
            Span::raw(&app.search_query),
        ]),
        AppMode::Adding => Line::from(vec![
            Span::styled("Add Task: ", Style::default().fg(Color::Green).bold()),
            Span::raw(&app.new_todo_query),
        ]),
        AppMode::Renaming => Line::from(vec![
            Span::styled("Rename Task: ", Style::default().fg(Color::Cyan).bold()),
            Span::raw(&app.new_todo_query),
        ]),
        AppMode::ConfirmDelete => Line::from(vec![
            Span::styled("Delete selected item? Are you sure? (y/n): ", Style::default().fg(Color::Red).bold()),
        ]),
        AppMode::Normal => Line::from(vec![
            Span::styled(" [j/k] Nav | [Shift+J/K] Move | [space] Toggle | [a] Add | [r] Rename | [d] Delete | [/] Filter | [q] Quit", Style::default().fg(Color::DarkGray)),
        ]),
    };

    // 4. Horizontal crisp line separator using a targeted top border layout block
    let hint_block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::DarkGray));

    let prompt = Paragraph::new(prompt_text).block(hint_block);
    frame.render_widget(prompt, chunks[1]);

    // 5. Calculate and place the blinking terminal cursor dynamically based on active typing context
    match app.mode {
        AppMode::Adding => {
            let cursor_x = chunks[1].x + 10 + app.new_todo_query.chars().count() as u16;
            let cursor_y = chunks[1].y + 1;
            frame.set_cursor_position(Position::new(cursor_x, cursor_y));
        }
        AppMode::Renaming => {
            // "Rename Task: " has a length of 13 characters
            let cursor_x = chunks[1].x + 13 + app.new_todo_query.chars().count() as u16;
            let cursor_y = chunks[1].y + 1;
            frame.set_cursor_position(Position::new(cursor_x, cursor_y));
        }
        AppMode::Filtering => {
            let cursor_x = chunks[1].x + 8 + app.search_query.chars().count() as u16;
            let cursor_y = chunks[1].y + 1;
            frame.set_cursor_position(Position::new(cursor_x, cursor_y));
        }
        _ => {} // No active cursor for normal or delete mode
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

    loop {
        let filtered_len = app.filtered_indices().len();
        if filtered_len == 0 {
            app.selected_index = 0;
        } else if app.selected_index >= filtered_len {
            app.selected_index = filtered_len - 1;
        }

        terminal.draw(|f| ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                let current_filtered_len = app.filtered_indices().len();
                let max_index = current_filtered_len.saturating_sub(1);

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
                                let filtered = app.filtered_indices();
                                if let Some(&actual_idx) = filtered.get(app.selected_index) {
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
                            let filtered = app.filtered_indices();
                            if let Some(&actual_idx) = filtered.get(app.selected_index) {
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
                            if current_filtered_len > 0 && app.selected_index < max_index {
                                app.selected_index += 1;
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            if app.selected_index > 0 {
                                app.selected_index -= 1;
                            }
                        }
                        // Shift + J to move down
                        KeyCode::Char('J') => {
                            if current_filtered_len > 0 && app.selected_index < max_index {
                                let filtered = app.filtered_indices();
                                let current_actual_idx = filtered[app.selected_index];
                                let target_actual_idx = filtered[app.selected_index + 1];
                                
                                app.todos.swap(current_actual_idx, target_actual_idx);
                                app.selected_index += 1;
                                app.save()?;
                            }
                        }
                        // Shift + K to move up
                        KeyCode::Char('K') => {
                            if current_filtered_len > 0 && app.selected_index > 0 {
                                let filtered = app.filtered_indices();
                                let current_actual_idx = filtered[app.selected_index];
                                let target_actual_idx = filtered[app.selected_index - 1];
                                
                                app.todos.swap(current_actual_idx, target_actual_idx);
                                app.selected_index -= 1;
                                app.save()?;
                            }
                        }
                        KeyCode::Char(' ') => {
                            let filtered = app.filtered_indices();
                            if let Some(&actual_idx) = filtered.get(app.selected_index) {
                                app.todos[actual_idx].completed = !app.todos[actual_idx].completed;
                                app.save()?;
                            }
                        }
                        KeyCode::Char('/') => {
                            app.mode = AppMode::Filtering;
                        }
                        KeyCode::Char('a') => {
                            app.mode = AppMode::Adding;
                        }
                        KeyCode::Char('r') => {
                            let filtered = app.filtered_indices();
                            if let Some(&actual_idx) = filtered.get(app.selected_index) {
                                app.new_todo_query = app.todos[actual_idx].text.clone();
                                app.mode = AppMode::Renaming;
                            }
                        }
                        KeyCode::Char('d') => {
                            if current_filtered_len > 0 {
                                app.mode = AppMode::ConfirmDelete;
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    ratatui::restore();
    Ok(())
}
