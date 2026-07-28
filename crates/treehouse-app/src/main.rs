use std::path::PathBuf;

use eframe::egui;
use treehouse_core::Document;
use treehouse_parser::parse_json_file;
use treehouse_search::{search_document, SearchMatch};
use treehouse_stats::{analyze, DocumentStats};
use treehouse_tree::{build_rows, TreeRow, TreeState};

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Treehouse",
        options,
        Box::new(|_cc| Ok(Box::new(TreehouseApp::default()))),
    )
}

#[derive(Default)]
struct TreehouseApp {
    current_file: Option<PathBuf>,
    document: Option<Document>,
    tree_state: TreeState,
    search_query: String,
    search_results: Vec<SearchMatch>,
    stats: Option<DocumentStats>,
    error: Option<String>,
}

impl TreehouseApp {
    fn open_file(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .pick_file()
        else {
            return;
        };

        match parse_json_file(&path) {
            Ok(doc) => {
                self.stats = Some(analyze(&doc));
                self.search_results = search_document(&doc, &self.search_query);
                self.current_file = Some(path);
                self.document = Some(doc);
                self.tree_state = TreeState::default();
                self.error = None;
            }
            Err(err) => {
                self.error = Some(err.to_string());
            }
        }
    }

    fn refresh_search(&mut self) {
        self.search_results = self
            .document
            .as_ref()
            .map(|doc| search_document(doc, &self.search_query))
            .unwrap_or_default();
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

                    ui.monospace(&row.label);
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
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open JSON").clicked() {
                    self.open_file();
                }

                if let Some(path) = &self.current_file {
                    ui.label(path.display().to_string());
                }

                ui.separator();
                ui.label("Search:");
                let changed = ui
                    .add(egui::TextEdit::singleline(&mut self.search_query).hint_text("key/value/path"))
                    .changed();
                if changed {
                    self.refresh_search();
                }
            });
        });

        egui::SidePanel::right("stats").resizable(true).show(ctx, |ui| {
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
                ui.label("Open a JSON file to calculate statistics.");
            }

            ui.separator();
            ui.heading("Search Results");
            egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                for m in &self.search_results {
                    ui.monospace(format!("{} → {}", m.path, m.snippet));
                }
                if self.search_results.is_empty() {
                    ui.label("No matches");
                }
            });

            if let Some(err) = &self.error {
                ui.separator();
                ui.colored_label(egui::Color32::RED, err);
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(document) = &self.document {
                let rows = build_rows(document, &self.tree_state);
                self.draw_tree(ui, &rows);
            } else {
                ui.centered_and_justified(|ui| {
                    ui.heading("Open a JSON file to start exploring");
                });
            }
        });
    }
}
