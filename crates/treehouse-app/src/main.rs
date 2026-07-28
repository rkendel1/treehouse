use std::{fs, path::PathBuf};

use eframe::egui;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use treehouse_core::Document;
use treehouse_parser::{parse_structured_file, DocumentFormat, ParsedDocument};
use treehouse_query::{query_json_path, value_at_path, QueryMatch};
use treehouse_search::{search_document, SearchMatch};
use treehouse_stats::{analyze, DocumentStats};
use treehouse_tree::{build_rows, TreeRow, TreeState};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Treehouse",
        options,
        Box::new(|_cc| Ok(Box::new(TreehouseApp::load()))),
    )
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct PersistedState {
    recent_files: Vec<String>,
    bookmarks: Vec<String>,
}

#[derive(Default)]
struct TreehouseApp {
    current_file: Option<PathBuf>,
    current_format: Option<DocumentFormat>,
    document: Option<Document>,
    tree_state: TreeState,
    search_query: String,
    search_results: Vec<SearchMatch>,
    stats: Option<DocumentStats>,
    jsonpath_query: String,
    jsonpath_results: Vec<QueryMatch>,
    bookmarks: Vec<String>,
    recent_files: Vec<PathBuf>,
    show_command_palette: bool,
    command_filter: String,
    error: Option<String>,
}

impl TreehouseApp {
    fn load() -> Self {
        let mut app = Self::default();
        if let Some(state) = load_persisted_state() {
            app.recent_files = state.recent_files.into_iter().map(PathBuf::from).collect();
            app.bookmarks = state.bookmarks;
        }
        app
    }

    fn open_file_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Structured",
                &["json", "jsonl", "ndjson", "yaml", "yml", "toml"],
            )
            .pick_file()
        else {
            return;
        };

        self.open_file_path(path);
    }

    fn open_file_path(&mut self, path: PathBuf) {
        match parse_structured_file(&path) {
            Ok(parsed) => {
                self.apply_document(parsed);
            }
            Err(err) => {
                self.error = Some(err.to_string());
            }
        }
    }

    fn apply_document(&mut self, parsed: ParsedDocument) {
        self.current_file = Some(parsed.path.clone());
        self.current_format = Some(parsed.format);
        self.stats = Some(analyze(&parsed.document));
        self.document = Some(parsed.document);
        self.tree_state = TreeState::default();
        self.error = None;
        self.refresh_search();
        self.run_jsonpath_query();
        self.push_recent(parsed.path);
        self.save_state();
    }

    fn push_recent(&mut self, path: PathBuf) {
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(12);
    }

    fn refresh_search(&mut self) {
        self.search_results = self
            .document
            .as_ref()
            .map(|doc| search_document(doc, &self.search_query))
            .unwrap_or_default();
    }

    fn run_jsonpath_query(&mut self) {
        self.jsonpath_results = self
            .document
            .as_ref()
            .map(|doc| query_json_path(doc, &self.jsonpath_query).unwrap_or_default())
            .unwrap_or_default();
    }

    fn add_selected_bookmark(&mut self) {
        if let Some(path) = self.tree_state.selected_path() {
            if !self.bookmarks.iter().any(|p| p == path) {
                self.bookmarks.push(path.to_string());
                self.save_state();
            }
        }
    }

    fn execute_palette_command(&mut self, command: PaletteCommand) {
        match command {
            PaletteCommand::OpenFile => self.open_file_dialog(),
            PaletteCommand::AddBookmark => self.add_selected_bookmark(),
            PaletteCommand::ClearBookmarks => {
                self.bookmarks.clear();
                self.save_state();
            }
            PaletteCommand::ClearRecentFiles => {
                self.recent_files.clear();
                self.save_state();
            }
            PaletteCommand::ClearSelection => self.tree_state.clear_selection(),
        }
        self.show_command_palette = false;
    }

    fn save_state(&self) {
        let state = PersistedState {
            recent_files: self
                .recent_files
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            bookmarks: self.bookmarks.clone(),
        };
        save_persisted_state(&state);
    }

    fn draw_tree(&mut self, ui: &mut egui::Ui, rows: &[TreeRow]) {
        let row_height = 22.0;
        egui::ScrollArea::vertical().show_rows(ui, row_height, rows.len(), |ui, row_range| {
            for row_index in row_range {
                let row = &rows[row_index];
                ui.horizontal(|ui| {
                    ui.add_space(row.depth as f32 * 16.0);

                    if row.expandable {
                        let symbol = if row.expanded { "▼" } else { "▶" };
                        if ui.small_button(symbol).clicked() {
                            self.tree_state.toggle(&row.path);
                        }
                    } else {
                        ui.label("•");
                    }

                    if ui.selectable_label(row.selected, &row.label).clicked() {
                        self.tree_state.select_path(row.path.clone());
                    }

                    ui.small(format!(
                        "[{:?}] offset={} len={} children={}",
                        row.node_type, row.meta.offset, row.meta.length, row.meta.child_count
                    ));
                });
            }
        });
    }
}

impl eframe::App for TreehouseApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.key_pressed(egui::Key::K) && i.modifiers.command) {
            self.show_command_palette = true;
        }

        let rows = self
            .document
            .as_ref()
            .map(|document| build_rows(document, &self.tree_state))
            .unwrap_or_default();

        let selected_row = self
            .tree_state
            .selected_path()
            .and_then(|path| rows.iter().find(|r| r.path == path))
            .cloned();

        let selected_value = self
            .document
            .as_ref()
            .and_then(|doc| {
                self.tree_state
                    .selected_path()
                    .and_then(|path| value_at_path(doc, path))
            })
            .cloned();

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Open File").clicked() {
                    self.open_file_dialog();
                }

                if ui.button("Command Palette").clicked() {
                    self.show_command_palette = true;
                }

                if ui.button("Add Bookmark").clicked() {
                    self.add_selected_bookmark();
                }

                if let Some(path) = &self.current_file {
                    ui.separator();
                    ui.label(path.display().to_string());
                    if let Some(format) = self.current_format {
                        ui.small(format!("({format:?})"));
                    }
                }

                ui.separator();
                ui.label("Search:");
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text("key/value/path"),
                    )
                    .changed()
                {
                    self.refresh_search();
                }

                ui.separator();
                ui.label("JSONPath:");
                let changed = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.jsonpath_query)
                            .hint_text("$.orders[*].status / $..price"),
                    )
                    .changed();
                if changed {
                    self.run_jsonpath_query();
                }
                if ui.small_button("Run").clicked() {
                    self.run_jsonpath_query();
                }
            });
        });

        egui::SidePanel::left("navigation")
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Bookmarks");
                if self.bookmarks.is_empty() {
                    ui.label("No bookmarks");
                }

                let mut remove_bookmark: Option<usize> = None;
                let bookmarks = self.bookmarks.clone();
                for (idx, path) in bookmarks.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.button("Go").clicked() {
                            self.tree_state.select_path(path.clone());
                        }
                        if ui.small_button("✕").clicked() {
                            remove_bookmark = Some(idx);
                        }
                        ui.monospace(path);
                    });
                }
                if let Some(idx) = remove_bookmark {
                    self.bookmarks.remove(idx);
                    self.save_state();
                }

                ui.separator();
                ui.heading("Recent Files");
                if self.recent_files.is_empty() {
                    ui.label("No recent files");
                }
                let recent = self.recent_files.clone();
                for path in recent {
                    if ui.button(path.display().to_string()).clicked() {
                        self.open_file_path(path);
                    }
                }
            });

        egui::SidePanel::right("details")
            .resizable(true)
            .show(ctx, |ui| {
                ui.heading("Inspector");
                if let Some(path) = self.tree_state.selected_path() {
                    ui.monospace(path);
                    if let Some(value) = &selected_value {
                        let mut formatted = serde_json::to_string_pretty(value)
                            .unwrap_or_else(|_| value.to_string());
                        ui.add(
                            egui::TextEdit::multiline(&mut formatted)
                                .desired_rows(8)
                                .interactive(false),
                        );
                    }
                } else {
                    ui.label("Select a tree node to inspect its value.");
                }

                ui.separator();
                ui.heading("Property Panel");
                if let Some(row) = &selected_row {
                    ui.label(format!("Path: {}", row.path));
                    ui.label(format!("Type: {:?}", row.node_type));
                    ui.label(format!("Depth: {}", row.depth));
                    ui.label(format!("Offset: {}", row.meta.offset));
                    ui.label(format!("Length: {}", row.meta.length));
                    ui.label(format!("Children: {}", row.meta.child_count));
                    ui.label(format!("Expandable: {}", row.expandable));
                } else {
                    ui.label("No selected node.");
                }

                ui.separator();
                ui.heading("Statistics");
                if let Some(stats) = &self.stats {
                    ui.label(format!("Objects: {}", stats.objects));
                    ui.label(format!("Arrays: {}", stats.arrays));
                    ui.label(format!("Values: {}", stats.values));
                    ui.label(format!("Max depth: {}", stats.max_depth));
                    ui.label(format!("Largest array: {}", stats.largest_array));
                    ui.label(format!("Null values: {}", stats.null_count));
                    ui.separator();
                    ui.label("Most common keys:");
                    for (key, count) in &stats.most_common_keys {
                        ui.label(format!("{} ({})", key, count));
                    }
                } else {
                    ui.label("Open a file to calculate statistics.");
                }

                ui.separator();
                ui.heading("Search Results");
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for m in &self.search_results {
                            if ui.button(format!("{} → {}", m.path, m.snippet)).clicked() {
                                self.tree_state.select_path(m.path.clone());
                            }
                        }
                        if self.search_results.is_empty() {
                            ui.label("No matches");
                        }
                    });

                ui.separator();
                ui.heading("JSONPath Results");
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(ui, |ui| {
                        for m in &self.jsonpath_results {
                            let snippet = summarize_for_results(&m.value);
                            if ui.button(format!("{} → {}", m.path, snippet)).clicked() {
                                self.tree_state.select_path(m.path.clone());
                            }
                        }
                        if self.jsonpath_results.is_empty() {
                            ui.label("No results");
                        }
                    });

                if let Some(err) = &self.error {
                    ui.separator();
                    ui.colored_label(egui::Color32::RED, err);
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.document.is_some() {
                self.draw_tree(ui, &rows);
            } else {
                ui.centered_and_justified(|ui| {
                    ui.heading("Open a JSON, YAML, or TOML file to start exploring");
                });
            }
        });

        if self.show_command_palette {
            egui::Window::new("Command Palette")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Ctrl/Cmd + K");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.command_filter)
                            .hint_text("Filter commands"),
                    );

                    for command in filtered_commands(&self.command_filter) {
                        if ui.button(command.label()).clicked() {
                            self.execute_palette_command(command);
                        }
                    }

                    if ui.button("Close").clicked() {
                        self.show_command_palette = false;
                    }
                });
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PaletteCommand {
    OpenFile,
    AddBookmark,
    ClearBookmarks,
    ClearRecentFiles,
    ClearSelection,
}

impl PaletteCommand {
    fn label(self) -> &'static str {
        match self {
            PaletteCommand::OpenFile => "Open File",
            PaletteCommand::AddBookmark => "Add Bookmark",
            PaletteCommand::ClearBookmarks => "Clear Bookmarks",
            PaletteCommand::ClearRecentFiles => "Clear Recent Files",
            PaletteCommand::ClearSelection => "Clear Selection",
        }
    }
}

fn filtered_commands(filter: &str) -> Vec<PaletteCommand> {
    let all = [
        PaletteCommand::OpenFile,
        PaletteCommand::AddBookmark,
        PaletteCommand::ClearBookmarks,
        PaletteCommand::ClearRecentFiles,
        PaletteCommand::ClearSelection,
    ];

    let filter = filter.trim().to_lowercase();
    if filter.is_empty() {
        return all.to_vec();
    }

    all.into_iter()
        .filter(|command| command.label().to_lowercase().contains(&filter))
        .collect()
}

fn summarize_for_results(value: &Value) -> String {
    match value {
        Value::Object(map) => format!("object ({})", map.len()),
        Value::Array(items) => format!("array ({})", items.len()),
        Value::String(v) => format!("\"{}\"", v),
        Value::Number(v) => v.to_string(),
        Value::Bool(v) => v.to_string(),
        Value::Null => "null".to_string(),
    }
}

fn persisted_state_path() -> Option<PathBuf> {
    let mut base = dirs::config_dir()?;
    base.push("treehouse");
    Some(base.join("state.json"))
}

fn load_persisted_state() -> Option<PersistedState> {
    let path = persisted_state_path()?;
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn save_persisted_state(state: &PersistedState) {
    let Some(path) = persisted_state_path() else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };

    if fs::create_dir_all(parent).is_err() {
        return;
    }

    let Ok(serialized) = serde_json::to_string_pretty(state) else {
        return;
    };

    let _ = fs::write(path, serialized);
}
