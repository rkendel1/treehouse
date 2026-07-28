use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use eframe::egui;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use treehouse_core::Document;
use treehouse_diff::{diff_documents, DiffEntry, DiffKind};
use treehouse_graph::{GraphEdgeKind, GraphSource, UniversalDataGraph};
use treehouse_parser::{parse_structured_file, DocumentFormat, ParsedDocument};
use treehouse_query::{query_json_path, value_at_path, QueryMatch};
use treehouse_scan::{run_scan, ScanOutputFormat, ScanRequest, ScanSummary};
use treehouse_search::{search_document, SearchMatch};
use treehouse_stats::{analyze, DocumentStats};
use treehouse_tree::{build_rows, TreeRow, TreeState};

const MAX_RECENT_FILES: usize = 12;
const TREE_ROW_HEIGHT: f32 = 22.0;
const GRAPH_ROW_HEIGHT: f32 = 22.0;
const DIFF_ROW_HEIGHT: f32 = 24.0;
const SYSTEM_DIFF_ROW_HEIGHT: f32 = 22.0;
const GIT_STATUS_SEPARATOR_INDEX: usize = 2;
const GIT_STATUS_PATH_OFFSET: usize = 3;
// Default graph view hides relationships below 70% confidence to reduce visual noise.
const GRAPH_CONFIDENCE_THRESHOLD: u8 = 70;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Treehouse",
        options,
        Box::new(|_cc| Ok(Box::new(TreehouseApp::load()))),
    )
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct PersistedState {
    recent_files: Vec<String>,
    bookmarks: Vec<String>,
    monitor_targets: Vec<String>,
    workspace_layouts: BTreeMap<String, PersistedLayout>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
struct PersistedLayout {
    explorer_view: ExplorerView,
    bottom_tab: BottomTab,
    show_navigation: bool,
    show_inspector: bool,
    show_bottom: bool,
    focus_mode: bool,
    show_all_graph_relationships: bool,
}

impl Default for PersistedLayout {
    fn default() -> Self {
        Self {
            explorer_view: ExplorerView::Overview,
            bottom_tab: BottomTab::SystemDiff,
            show_navigation: true,
            show_inspector: true,
            show_bottom: true,
            focus_mode: false,
            show_all_graph_relationships: false,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct LiveSystemDiff {
    summary: String,
    changed_files: Vec<String>,
    entities_added: Vec<String>,
    relationships_added: Vec<String>,
    api_added: Vec<String>,
    workflows_added: Vec<String>,
    new_capabilities: Vec<String>,
    potential_breaks: Vec<String>,
    architecture_drift: Vec<String>,
    architecture_confidence: u8,
}

#[derive(Default)]
struct TreehouseApp {
    current_file: Option<PathBuf>,
    current_format: Option<DocumentFormat>,
    document: Option<Document>,
    comparison_file: Option<PathBuf>,
    comparison_format: Option<DocumentFormat>,
    comparison_document: Option<Document>,
    diff_entries: Vec<DiffEntry>,
    tree_state: TreeState,
    search_query: String,
    search_results: Vec<SearchMatch>,
    stats: Option<DocumentStats>,
    jsonpath_query: String,
    jsonpath_results: Vec<QueryMatch>,
    graph: Option<UniversalDataGraph>,
    selected_entity: Option<String>,
    explorer_view: ExplorerView,
    bookmarks: Vec<String>,
    recent_files: Vec<PathBuf>,
    show_command_palette: bool,
    command_filter: String,
    bottom_tab: BottomTab,
    show_navigation: bool,
    show_inspector: bool,
    show_bottom: bool,
    focus_mode: bool,
    show_help_panel: bool,
    show_all_graph_relationships: bool,
    workspace_layouts: BTreeMap<String, PersistedLayout>,
    live_system_diff: Option<LiveSystemDiff>,
    live_system_diff_path: Option<PathBuf>,
    live_system_diff_workspace: Option<PathBuf>,
    live_system_diff_filter: String,
    monitor_targets: Vec<PathBuf>,
    monitor_target_input: String,
    selected_monitor_target: Option<usize>,
    live_system_diff_mtime_unix: Option<u64>,
    scan_repo_path: String,
    scan_target_path: String,
    scan_output_path: String,
    scan_use_local_llm: bool,
    scan_local_llm_backend: String,
    scan_status: Option<String>,
    scan_summary: Option<ScanSummary>,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
enum ExplorerView {
    #[default]
    Overview,
    Tree,
    Graph,
    Scan,
    Diff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
enum BottomTab {
    Search,
    JsonPath,
    Stats,
    #[default]
    SystemDiff,
}

impl TreehouseApp {
    fn load() -> Self {
        let mut app = Self {
            explorer_view: ExplorerView::Overview,
            bottom_tab: BottomTab::SystemDiff,
            show_navigation: true,
            show_inspector: true,
            show_bottom: true,
            ..Self::default()
        };
        if let Some(state) = load_persisted_state() {
            app.recent_files = state.recent_files.into_iter().map(PathBuf::from).collect();
            app.bookmarks = state.bookmarks;
            app.monitor_targets = state.monitor_targets.into_iter().map(PathBuf::from).collect();
            app.workspace_layouts = state.workspace_layouts;
            if let Some(layout) = app.workspace_layouts.get("default").cloned() {
                app.apply_layout(layout);
            }
        }
        app
    }

    fn open_file_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Structured",
                &[
                    "json", "jsonl", "ndjson", "yaml", "yml", "toml", "xml", "csv",
                ],
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
        self.load_workspace_layout_for_path(&parsed.path);
        let source_name = parsed.path.display().to_string();
        let graph = UniversalDataGraph::build(&[GraphSource {
            name: &source_name,
            document: &parsed.document,
        }]);
        self.selected_entity = graph
            .intelligence
            .first()
            .map(|profile| profile.name.clone());
        self.graph = Some(graph);
        self.current_file = Some(parsed.path.clone());
        self.current_format = Some(parsed.format);
        self.stats = Some(analyze(&parsed.document));
        self.document = Some(parsed.document);
        self.explorer_view = ExplorerView::Overview;
        self.refresh_diff();
        self.tree_state = TreeState::default();
        self.error = None;
        self.refresh_search();
        self.run_jsonpath_query();
        self.try_connect_system_diff(false);
        self.push_recent(parsed.path);
        self.save_state();
    }

    fn push_recent(&mut self, path: PathBuf) {
        self.recent_files.retain(|p| p != &path);
        self.recent_files.insert(0, path);
        self.recent_files.truncate(MAX_RECENT_FILES);
    }

    fn refresh_search(&mut self) {
        self.search_results = self
            .document
            .as_ref()
            .map(|doc| search_document(doc, &self.search_query))
            .unwrap_or_default();
    }

    fn open_compare_file_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(
                "Structured",
                &[
                    "json", "jsonl", "ndjson", "yaml", "yml", "toml", "xml", "csv",
                ],
            )
            .pick_file()
        else {
            return;
        };

        self.open_compare_file_path(path);
    }

    fn open_compare_file_path(&mut self, path: PathBuf) {
        match parse_structured_file(&path) {
            Ok(parsed) => {
                self.comparison_file = Some(parsed.path.clone());
                self.comparison_format = Some(parsed.format);
                self.comparison_document = Some(parsed.document);
                self.refresh_diff();
                self.explorer_view = ExplorerView::Diff;
                self.error = None;
            }
            Err(err) => {
                self.error = Some(err.to_string());
            }
        }
    }

    fn refresh_diff(&mut self) {
        self.diff_entries = match (&self.document, &self.comparison_document) {
            (Some(left), Some(right)) => diff_documents(left, right),
            _ => Vec::new(),
        };
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
            PaletteCommand::CompareFile => self.open_compare_file_dialog(),
            PaletteCommand::ShowOverview => self.explorer_view = ExplorerView::Overview,
            PaletteCommand::ShowTree => self.explorer_view = ExplorerView::Tree,
            PaletteCommand::ShowGraph => self.explorer_view = ExplorerView::Graph,
            PaletteCommand::ShowScan => self.explorer_view = ExplorerView::Scan,
            PaletteCommand::ShowDiff => self.explorer_view = ExplorerView::Diff,
            PaletteCommand::ToggleFocusMode => {
                self.focus_mode = !self.focus_mode;
                if self.focus_mode {
                    self.explorer_view = ExplorerView::Tree;
                }
            }
            PaletteCommand::ToggleSystemDiffPanel => self.show_bottom = !self.show_bottom,
            PaletteCommand::ConnectSystemDiff => self.try_connect_system_diff(true),
            PaletteCommand::DisconnectSystemDiff => {
                self.live_system_diff = None;
                self.live_system_diff_path = None;
                self.live_system_diff_workspace = None;
                self.live_system_diff_mtime_unix = None;
            }
            PaletteCommand::ResetLayout => self.reset_layout(),
            PaletteCommand::ToggleHelpPanel => self.show_help_panel = !self.show_help_panel,
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
            PaletteCommand::ClearComparison => {
                self.comparison_file = None;
                self.comparison_format = None;
                self.comparison_document = None;
                self.refresh_diff();
            }
        }
        self.show_command_palette = false;
        self.save_state();
    }

    fn save_state(&self) {
        let mut workspace_layouts = self.workspace_layouts.clone();
        workspace_layouts.insert(self.workspace_key(), self.current_layout());
        workspace_layouts.insert("default".to_string(), self.current_layout());
        let state = PersistedState {
            recent_files: self
                .recent_files
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            bookmarks: self.bookmarks.clone(),
            monitor_targets: self
                .monitor_targets
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            workspace_layouts,
        };
        save_persisted_state(&state);
    }

    fn add_monitor_target(&mut self, path: PathBuf) {
        if !path.exists() {
            self.error = Some(format!("Target does not exist: {}", path.display()));
            return;
        }
        let normalized = if path.is_absolute() {
            path
        } else if let Ok(cwd) = env::current_dir() {
            cwd.join(path)
        } else {
            path
        };
        if !self.monitor_targets.iter().any(|existing| existing == &normalized) {
            self.monitor_targets.push(normalized);
            self.selected_monitor_target = Some(self.monitor_targets.len().saturating_sub(1));
            self.save_state();
        }
    }

    fn connect_selected_monitor_target(&mut self, set_error_if_missing: bool) -> bool {
        let Some(index) = self.selected_monitor_target else {
            if set_error_if_missing {
                self.error = Some("Select a monitor target first.".to_string());
            }
            return false;
        };
        let Some(workspace) = self.monitor_targets.get(index).cloned() else {
            if set_error_if_missing {
                self.error = Some("Selected monitor target is invalid.".to_string());
            }
            return false;
        };

        let path = workspace.join(".treehouse/system-diff.json");
        if !path.is_file() {
            if set_error_if_missing {
                self.error = Some(format!(
                    "Could not find {}. Run `treehouse watch {}` first.",
                    path.display(),
                    workspace.display()
                ));
            }
            return false;
        }

        self.live_system_diff_path = Some(path);
        self.live_system_diff_workspace = Some(workspace);
        self.refresh_system_diff(true);
        true
    }

    fn discover_desktop_targets(&mut self) {
        let discovered = discover_desktop_git_repos();
        let mut added = 0usize;
        for repo in discovered {
            if !self.monitor_targets.iter().any(|existing| existing == &repo) {
                self.monitor_targets.push(repo);
                added += 1;
            }
        }
        if self.selected_monitor_target.is_none() && !self.monitor_targets.is_empty() {
            self.selected_monitor_target = Some(0);
        }
        if added > 0 {
            self.save_state();
        }
    }

    fn load_workspace_layout_for_path(&mut self, path: &Path) {
        if let Some(layout) = self
            .workspace_layouts
            .get(&workspace_key_from_path(path))
            .cloned()
        {
            self.apply_layout(layout);
        }
    }

    fn current_layout(&self) -> PersistedLayout {
        PersistedLayout {
            explorer_view: self.explorer_view,
            bottom_tab: self.bottom_tab,
            show_navigation: self.show_navigation,
            show_inspector: self.show_inspector,
            show_bottom: self.show_bottom,
            focus_mode: self.focus_mode,
            show_all_graph_relationships: self.show_all_graph_relationships,
        }
    }

    fn apply_layout(&mut self, layout: PersistedLayout) {
        self.explorer_view = layout.explorer_view;
        self.bottom_tab = layout.bottom_tab;
        self.show_navigation = layout.show_navigation;
        self.show_inspector = layout.show_inspector;
        self.show_bottom = layout.show_bottom;
        self.focus_mode = layout.focus_mode;
        self.show_all_graph_relationships = layout.show_all_graph_relationships;
    }

    fn reset_layout(&mut self) {
        self.apply_layout(PersistedLayout::default());
    }

    fn workspace_key(&self) -> String {
        self.current_file
            .as_deref()
            .map(workspace_key_from_path)
            .unwrap_or_else(|| "default".to_string())
    }

    fn draw_tree(&mut self, ui: &mut egui::Ui, rows: &[TreeRow]) {
        egui::ScrollArea::vertical().show_rows(ui, TREE_ROW_HEIGHT, rows.len(), |ui, row_range| {
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

                    ui.small(format!("[{:?}]", row.node_type));
                    if let Some((entity, confidence)) = row_inference_badge(row, &self.graph) {
                        ui.small(format!("{entity} · {confidence}%"));
                    }
                    if row_changed_since_snapshot(row, &self.diff_entries) {
                        ui.colored_label(egui::Color32::from_rgb(205, 140, 0), "changed");
                    }
                });
            }
        });
    }

    fn draw_graph(
        ui: &mut egui::Ui,
        graph: &UniversalDataGraph,
        selected_entity: &mut Option<String>,
        show_all_relationships: &mut bool,
    ) {
        ui.heading("Graph View");
        ui.checkbox(
            show_all_relationships,
            "Show all evidence (include low-confidence relationships)",
        );
        ui.label("Detected Entities");
        for profile in &graph.intelligence {
            if ui
                .selectable_label(
                    selected_entity.as_deref() == Some(profile.name.as_str()),
                    format!(
                        "{} ({}, {:.0}% confidence)",
                        profile.name,
                        profile.instances,
                        profile.confidence * 100.0
                    ),
                )
                .clicked()
            {
                *selected_entity = Some(profile.name.clone());
            }
        }

        ui.separator();
        ui.label("Detected Relationships");
        let relationships: Vec<&_> = if *show_all_relationships {
            graph.relationships.iter().collect()
        } else {
            graph
                .relationships
                .iter()
                .filter(|relationship| relationship.confidence >= GRAPH_CONFIDENCE_THRESHOLD)
                .collect()
        };
        if relationships.is_empty() {
            ui.label("No relationships match the current confidence filter.");
            return;
        }
        egui::ScrollArea::vertical().show_rows(
            ui,
            GRAPH_ROW_HEIGHT,
            relationships.len(),
            |ui, row_range| {
                for row_index in row_range {
                    let relationship = relationships[row_index];
                    let kind = match relationship.kind {
                        GraphEdgeKind::HasMany => "has many",
                        GraphEdgeKind::BelongsTo => "belongs to",
                        GraphEdgeKind::Related => "related to",
                        GraphEdgeKind::HasField => "has field",
                        GraphEdgeKind::DerivedFrom => "derived from",
                    };
                    if ui
                        .button(format!(
                            "{} --{}--> {} ({}%)",
                            relationship.from, kind, relationship.to, relationship.confidence
                        ))
                        .clicked()
                    {
                        *selected_entity = Some(relationship.from.clone());
                    }
                }
            },
        );
    }

    fn draw_diff(&mut self, ui: &mut egui::Ui) {
        ui.heading("Diff View");
        if self.document.is_none() {
            ui.label("Open a base file first.");
            return;
        }
        if self.comparison_document.is_none() {
            ui.label("Choose a comparison file.");
            return;
        }

        if self.diff_entries.is_empty() {
            ui.label("No structural differences detected.");
            return;
        }

        egui::ScrollArea::vertical().show_rows(
            ui,
            DIFF_ROW_HEIGHT,
            self.diff_entries.len(),
            |ui, row_range| {
                for row_index in row_range {
                    let entry = &self.diff_entries[row_index];
                    ui.horizontal_wrapped(|ui| {
                        let badge = match entry.kind {
                            DiffKind::Added => "+",
                            DiffKind::Removed => "-",
                            DiffKind::Changed => "~",
                            DiffKind::TypeChanged => "±",
                        };
                        if ui.button(entry.path.clone()).clicked() {
                            self.tree_state.select_path(entry.path.clone());
                            self.explorer_view = ExplorerView::Tree;
                        }
                        ui.label(format!("{badge} {}", diff_summary(entry)));
                    });
                }
            },
        );
    }

    fn draw_overview(&mut self, ui: &mut egui::Ui) {
        ui.heading("Overview");
        ui.label(
            "Raw data is shown in neutral views. Inferred model details are highlighted with confidence.",
        );
        if let Some(graph) = &self.graph {
            let entities = graph.intelligence.len();
            let relationships = graph.relationships.len();
            let avg_confidence = if entities == 0 {
                0.0
            } else {
                graph.intelligence.iter().map(|p| p.confidence).sum::<f32>() / entities as f32
            };

            ui.separator();
            ui.label(format!("Inferred entities: {entities}"));
            ui.label(format!("Inferred relationships: {relationships}"));
            ui.label(format!(
                "Average model confidence: {:.0}%",
                avg_confidence * 100.0
            ));
            if let Some(diff) = &self.live_system_diff {
                ui.label(format!(
                    "Live system confidence: {}%",
                    diff.architecture_confidence
                ));
            }

            ui.separator();
            ui.label("Drill into inferred entities:");
            egui::ScrollArea::vertical().show_rows(
                ui,
                GRAPH_ROW_HEIGHT,
                graph.intelligence.len(),
                |ui, row_range| {
                    for row_index in row_range {
                        let profile = &graph.intelligence[row_index];
                        if ui
                            .button(format!(
                                "{} · {} instances · {:.0}% confidence",
                                profile.name,
                                profile.instances,
                                profile.confidence * 100.0
                            ))
                            .clicked()
                        {
                            self.selected_entity = Some(profile.name.clone());
                            self.explorer_view = ExplorerView::Graph;
                        }
                    }
                },
            );
        } else {
            ui.label("Drop or open a JSON/YAML/XML/TOML/CSV document to begin.");
        }
    }

    fn draw_scan(&mut self, ui: &mut egui::Ui) {
        ui.heading("Scan");
        ui.label("Run a target-driven architecture scan.");

        ui.horizontal(|ui| {
            ui.label("Repository:");
            ui.add(egui::TextEdit::singleline(&mut self.scan_repo_path).desired_width(420.0));
            if ui.button("Select").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.scan_repo_path = path.display().to_string();
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Target:");
            ui.add(egui::TextEdit::singleline(&mut self.scan_target_path).desired_width(420.0));
            if ui.button("Select").clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .add_filter("Markdown", &["md"])
                    .pick_file()
                {
                    self.scan_target_path = path.display().to_string();
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("Output:");
            ui.add(egui::TextEdit::singleline(&mut self.scan_output_path).desired_width(420.0));
            if ui.button("Select").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.scan_output_path = path.display().to_string();
                }
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.scan_use_local_llm, "Use local LLM");
            if self.scan_use_local_llm {
                ui.label("Backend:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.scan_local_llm_backend)
                        .hint_text("heuristic or ollama:<model>")
                        .desired_width(220.0),
                );
            }
        });

        if ui.button("Start Scan").clicked() {
            let repo_path = PathBuf::from(self.scan_repo_path.trim());
            if self.scan_repo_path.trim().is_empty() {
                self.error = Some("Scan repository path is required".to_string());
            } else if self.scan_target_path.trim().is_empty() {
                self.error = Some("Scan target path or name is required".to_string());
            } else {
                let output = if self.scan_output_path.trim().is_empty() {
                    None
                } else {
                    Some(PathBuf::from(self.scan_output_path.trim()))
                };
                let local_llm = if self.scan_use_local_llm {
                    if self.scan_local_llm_backend.trim().is_empty() {
                        Some("heuristic".to_string())
                    } else {
                        Some(self.scan_local_llm_backend.trim().to_string())
                    }
                } else {
                    None
                };
                let request = ScanRequest {
                    repo_path,
                    target: Some(self.scan_target_path.trim().to_string()),
                    output,
                    local_llm,
                    baseline_only: false,
                    goals_only: false,
                    format: ScanOutputFormat::Json,
                };
                match run_scan(&request) {
                    Ok(result) => {
                        self.scan_summary = Some(result.summary.clone());
                        self.scan_status =
                            Some(format!("Scan completed: {}", result.output_dir.display()));
                        self.error = None;
                    }
                    Err(err) => {
                        self.scan_status = None;
                        self.error = Some(format!("Scan failed: {err}"));
                    }
                }
            }
        }

        if let Some(status) = &self.scan_status {
            ui.separator();
            ui.label(status);
        }
        if let Some(summary) = &self.scan_summary {
            ui.separator();
            ui.label(format!("Baseline entities: {}", summary.baseline_entities));
            ui.label(format!("Target entities: {}", summary.target_entities));
            ui.label(format!("Goals: {}", summary.goals));
            ui.label(format!("Missing files: {}", summary.missing_files));
            ui.label(format!("Missing contracts: {}", summary.missing_contracts));
            ui.label(format!(
                "Missing migrations: {}",
                summary.missing_migrations
            ));
            ui.label(format!("API gaps: {}", summary.api_gaps));
        }
    }

    fn draw_bottom_panel(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut self.bottom_tab, BottomTab::SystemDiff, "System Diff");
            ui.selectable_value(&mut self.bottom_tab, BottomTab::Search, "Search");
            ui.selectable_value(&mut self.bottom_tab, BottomTab::JsonPath, "JSONPath");
            ui.selectable_value(&mut self.bottom_tab, BottomTab::Stats, "Stats");
        });
        ui.separator();
        match self.bottom_tab {
            BottomTab::SystemDiff => self.draw_system_diff(ui),
            BottomTab::Search => self.draw_search_results(ui),
            BottomTab::JsonPath => self.draw_jsonpath_results(ui),
            BottomTab::Stats => self.draw_stats(ui),
        }
    }

    fn draw_search_results(&mut self, ui: &mut egui::Ui) {
        ui.heading("Search Results");
        egui::ScrollArea::vertical().show(ui, |ui| {
            for m in &self.search_results {
                if ui.button(format!("{} → {}", m.path, m.snippet)).clicked() {
                    self.tree_state.select_path(m.path.clone());
                    self.explorer_view = ExplorerView::Tree;
                }
            }
            if self.search_results.is_empty() {
                ui.label("No matches");
            }
        });
    }

    fn draw_jsonpath_results(&mut self, ui: &mut egui::Ui) {
        ui.heading("JSONPath Results");
        egui::ScrollArea::vertical().show(ui, |ui| {
            for m in &self.jsonpath_results {
                let snippet = summarize_for_results(&m.value);
                if ui.button(format!("{} → {}", m.path, snippet)).clicked() {
                    self.tree_state.select_path(m.path.clone());
                    self.explorer_view = ExplorerView::Tree;
                }
            }
            if self.jsonpath_results.is_empty() {
                ui.label("No results");
            }
        });
    }

    fn draw_stats(&self, ui: &mut egui::Ui) {
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
    }

    fn draw_system_diff(&mut self, ui: &mut egui::Ui) {
        ui.heading("Live System Diff");
        ui.horizontal_wrapped(|ui| {
            if ui.button("Connect").clicked() {
                self.try_connect_system_diff(true);
            }
            if ui.button("Refresh").clicked() {
                self.refresh_system_diff(true);
            }
            if ui.button("Disconnect").clicked() {
                self.live_system_diff = None;
                self.live_system_diff_path = None;
                self.live_system_diff_workspace = None;
                self.live_system_diff_mtime_unix = None;
            }
            if let Some(path) = &self.live_system_diff_path {
                ui.monospace(path.display().to_string());
            } else {
                ui.label("Not connected");
            }
        });

        ui.separator();
        ui.label("Monitor targets");
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.monitor_target_input)
                    .hint_text("/path/to/repository"),
            );
            if ui.button("Browse").clicked() {
                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.monitor_target_input = path.display().to_string();
                }
            }
            if ui.button("Add Target").clicked() {
                let trimmed = self.monitor_target_input.trim();
                if trimmed.is_empty() {
                    self.error = Some("Enter a repository path to add.".to_string());
                } else {
                    self.add_monitor_target(PathBuf::from(trimmed));
                }
            }
            if ui.button("Discover Desktop").clicked() {
                self.discover_desktop_targets();
            }
            if ui.button("Connect Selected").clicked() {
                self.connect_selected_monitor_target(true);
            }
        });

        let mut remove_target: Option<usize> = None;
        egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
            if self.monitor_targets.is_empty() {
                ui.label("No monitor targets saved.");
            }
            for (idx, path) in self.monitor_targets.iter().enumerate() {
                let selected = self.selected_monitor_target == Some(idx);
                ui.horizontal(|ui| {
                    if ui.selectable_label(selected, path.display().to_string()).clicked() {
                        self.selected_monitor_target = Some(idx);
                    }
                    if ui.small_button("Remove").clicked() {
                        remove_target = Some(idx);
                    }
                });
            }
        });
        if let Some(idx) = remove_target {
            self.monitor_targets.remove(idx);
            self.selected_monitor_target = match self.selected_monitor_target {
                Some(sel) if sel == idx => None,
                Some(sel) if sel > idx => Some(sel - 1),
                other => other,
            };
            self.save_state();
        }

        ui.horizontal(|ui| {
            ui.label("Filter:");
            ui.add(
                egui::TextEdit::singleline(&mut self.live_system_diff_filter)
                    .hint_text("entity/api/workflow/drift text"),
            );
        });

        let Some(diff) = self.live_system_diff.clone() else {
            ui.label("Connect a repository to surface live architecture drift.");
            return;
        };

        ui.label(&diff.summary);
        ui.label(format!(
            "Architecture confidence: {}%",
            diff.architecture_confidence
        ));

        self.draw_system_diff_section(ui, "New capabilities", &diff.new_capabilities);
        if let Some(rel) =
            self.draw_system_diff_section(ui, "Relationship deltas", &diff.relationships_added)
        {
            self.search_query = rel;
            self.refresh_search();
            self.bottom_tab = BottomTab::Search;
        }
        if let Some(api) = self.draw_system_diff_section(ui, "API deltas", &diff.api_added) {
            self.search_query = api;
            self.refresh_search();
            self.bottom_tab = BottomTab::Search;
        }
        if let Some(workflow) =
            self.draw_system_diff_section(ui, "Workflow deltas", &diff.workflows_added)
        {
            self.search_query = workflow;
            self.refresh_search();
            self.bottom_tab = BottomTab::Search;
        }
        self.draw_system_diff_section(ui, "Potential breakage", &diff.potential_breaks);
        self.draw_system_diff_section(ui, "Architecture drift alerts", &diff.architecture_drift);

        ui.separator();
        ui.label("Changed files");
        egui::ScrollArea::vertical().show_rows(
            ui,
            SYSTEM_DIFF_ROW_HEIGHT,
            diff.changed_files.len(),
            |ui, row_range| {
                for row_index in row_range {
                    let changed = &diff.changed_files[row_index];
                    if ui.button(changed).clicked() {
                        self.open_system_diff_file(changed);
                    }
                }
            },
        );
    }

    fn draw_system_diff_section(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        entries: &[String],
    ) -> Option<String> {
        let filter = self.live_system_diff_filter.trim().to_ascii_lowercase();
        let filtered: Vec<&String> = if filter.is_empty() {
            entries.iter().collect()
        } else {
            entries
                .iter()
                .filter(|entry| entry.to_ascii_lowercase().contains(&filter))
                .collect()
        };
        if filtered.is_empty() {
            return None;
        }
        ui.separator();
        ui.label(title);
        let mut clicked: Option<String> = None;
        for entry in filtered {
            if ui.button(entry).clicked() {
                clicked = Some(entry.clone());
            }
        }
        clicked
    }

    fn open_system_diff_file(&mut self, changed: &str) {
        let Some(root) = &self.live_system_diff_workspace else {
            return;
        };
        let Some(rel) = parse_changed_file_path(changed) else {
            return;
        };
        let path = root.join(rel);
        if path.is_file() {
            self.open_file_path(path);
        }
    }

    fn try_connect_system_diff(&mut self, set_error_if_missing: bool) {
        if self.selected_monitor_target.is_some() {
            if self.connect_selected_monitor_target(set_error_if_missing) {
                return;
            }
        }
        let Some((path, workspace)) = discover_system_diff_path(self.current_file.as_deref())
        else {
            if set_error_if_missing {
                self.error = Some(
                    "Could not find .treehouse/system-diff.json. Run `treehouse watch` or connect to a repository first."
                        .to_string(),
                );
            }
            return;
        };
        self.live_system_diff_path = Some(path);
        self.live_system_diff_workspace = Some(workspace);
        self.refresh_system_diff(true);
    }

    fn refresh_system_diff(&mut self, force: bool) {
        let Some(path) = &self.live_system_diff_path else {
            return;
        };
        let Ok(metadata) = fs::metadata(path) else {
            self.live_system_diff = None;
            return;
        };
        let modified_unix = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs());
        if !force && modified_unix == self.live_system_diff_mtime_unix {
            return;
        }
        self.live_system_diff_mtime_unix = modified_unix;
        match fs::read_to_string(path).and_then(|content| {
            serde_json::from_str::<LiveSystemDiff>(&content)
                .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))
        }) {
            Ok(report) => {
                self.live_system_diff = Some(report);
                self.error = None;
            }
            Err(err) => {
                self.error = Some(format!(
                    "Failed to read system diff {}: {err}",
                    path.display()
                ));
            }
        }
    }
}

impl eframe::App for TreehouseApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let previous_layout = self.current_layout();
        let rows = self
            .document
            .as_ref()
            .map(|document| build_rows(document, &self.tree_state))
            .unwrap_or_default();

        if ctx.input(|i| i.key_pressed(egui::Key::K) && i.modifiers.command) {
            self.show_command_palette = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::D) && i.modifiers.command) {
            self.explorer_view = if self.explorer_view == ExplorerView::Diff {
                ExplorerView::Overview
            } else {
                ExplorerView::Diff
            };
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if let Some(parent) = self.tree_state.selected_path().and_then(parent_path) {
                self.tree_state.select_path(parent);
            } else {
                self.tree_state.clear_selection();
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            select_adjacent_tree_row(&mut self.tree_state, &rows, 1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            select_adjacent_tree_row(&mut self.tree_state, &rows, -1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
            if let Some(path) = self.tree_state.selected_path().map(str::to_string) {
                self.tree_state.toggle(&path);
            }
        }
        self.refresh_system_diff(false);

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
                if ui.button("Compare File").clicked() {
                    self.open_compare_file_dialog();
                }

                if ui.button("Connect").clicked() {
                    self.try_connect_system_diff(true);
                }
                if ui.button("Command Palette").clicked() {
                    self.show_command_palette = true;
                }
                if ui
                    .button(if self.focus_mode {
                        "Exit Focus"
                    } else {
                        "Focus Mode"
                    })
                    .clicked()
                {
                    self.focus_mode = !self.focus_mode;
                    if self.focus_mode {
                        self.explorer_view = ExplorerView::Tree;
                    }
                }
                if ui.button("What am I looking at?").clicked() {
                    self.show_help_panel = true;
                }

                if ui.button("Add Bookmark").clicked() {
                    self.add_selected_bookmark();
                }

                ui.separator();
                ui.selectable_value(&mut self.explorer_view, ExplorerView::Overview, "Overview");
                ui.selectable_value(&mut self.explorer_view, ExplorerView::Tree, "Tree View");
                ui.selectable_value(&mut self.explorer_view, ExplorerView::Graph, "Graph View");
                ui.selectable_value(&mut self.explorer_view, ExplorerView::Scan, "Scan");
                ui.selectable_value(&mut self.explorer_view, ExplorerView::Diff, "Diff View");

                if let Some(path) = &self.current_file {
                    ui.separator();
                    ui.label(path.display().to_string());
                    if let Some(format) = self.current_format {
                        ui.small(format!("({format:?})"));
                    }
                }
                if let Some(path) = &self.comparison_file {
                    ui.separator();
                    ui.label(format!("↔ {}", path.display()));
                    if let Some(format) = self.comparison_format {
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

        if self.show_navigation && !self.focus_mode {
            egui::SidePanel::left("navigation")
                .resizable(true)
                .show(ctx, |ui| {
                    ui.heading("Document Tree");
                    if self.document.is_some() {
                        self.draw_tree(ui, &rows);
                    } else {
                        ui.label("Drop a JSON/YAML file or folder here.");
                    }

                    ui.separator();
                    ui.heading("Bookmarks");
                    if self.bookmarks.is_empty() {
                        ui.label("No bookmarks");
                    }

                    let mut remove_bookmark: Option<usize> = None;
                    let mut open_bookmark: Option<String> = None;
                    for (idx, path) in self.bookmarks.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.button("Go").clicked() {
                                open_bookmark = Some(path.clone());
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
                    if let Some(path) = open_bookmark {
                        self.tree_state.select_path(path);
                    }

                    ui.separator();
                    ui.heading("Recent Files");
                    if self.recent_files.is_empty() {
                        ui.label("No recent files");
                    }
                    let mut open_recent: Option<PathBuf> = None;
                    for path in &self.recent_files {
                        if ui.button(path.display().to_string()).clicked() {
                            open_recent = Some(path.clone());
                        }
                    }
                    if let Some(path) = open_recent {
                        self.open_file_path(path);
                    }
                });
        }

        if self.show_inspector || self.focus_mode {
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
                ui.heading("Data Intelligence");
                if let Some(graph) = &self.graph {
                    let selected = self.selected_entity.clone().or_else(|| {
                        graph
                            .intelligence
                            .first()
                            .map(|profile| profile.name.clone())
                    });

                    if let Some(name) = selected {
                        if let Some(profile) = graph
                            .intelligence
                            .iter()
                            .find(|profile| profile.name == name)
                        {
                            let schema = graph.schemas.iter().find(|schema| schema.name == name);
                            let observation = graph
                                .observations
                                .iter()
                                .find(|observation| observation.entity == name);
                            let relationships: Vec<String> = graph
                                .relationships
                                .iter()
                                .filter(|relationship| {
                                    relationship.from == name || relationship.to == name
                                })
                                .map(|relationship| {
                                    let label = match relationship.kind {
                                        GraphEdgeKind::HasMany => "has many",
                                        GraphEdgeKind::BelongsTo => "belongs to",
                                        GraphEdgeKind::Related => "related to",
                                        GraphEdgeKind::HasField => "has field",
                                        GraphEdgeKind::DerivedFrom => "derived from",
                                    };
                                    format!(
                                        "{} --{}--> {} ({}%)",
                                        relationship.from,
                                        label,
                                        relationship.to,
                                        relationship.confidence
                                    )
                                })
                                .collect();

                            ui.label(format!("Entity: {}", profile.name));
                            ui.label(format!("Instances: {}", profile.instances));
                            ui.label(format!("Fields: {}", profile.fields));
                            ui.label(format!(
                                "Primary Identifier: {}",
                                profile
                                    .primary_identifier
                                    .as_deref()
                                    .unwrap_or("not detected")
                            ));
                            ui.label(format!("Required: {:.0}%", profile.required_ratio * 100.0));
                            ui.label(format!("Nullable: {:.0}%", profile.nullable_ratio * 100.0));
                            ui.label(format!("Confidence: {:.0}%", profile.confidence * 100.0));
                            ui.label(format!(
                                "Related: {}",
                                if profile.related.is_empty() {
                                    "none".to_string()
                                } else {
                                    profile.related.join(", ")
                                }
                            ));
                            ui.label(format!(
                                "Detected PII: {}",
                                if profile.detected_pii.is_empty() {
                                    "none".to_string()
                                } else {
                                    profile.detected_pii.join(", ")
                                }
                            ));
                            ui.label(format!("Sources: {}", profile.sources.join(", ")));

                            ui.separator();
                            ui.label("Field Definitions");
                            if let Some(schema) = schema {
                                for field in &schema.properties {
                                    ui.label(format!(
                                        "{}: {:?} (required {:.0}%, nullable {:.0}%, confidence {:.0}%)",
                                        field.name,
                                        field.kind,
                                        field.required_ratio * 100.0,
                                        field.nullable_ratio * 100.0,
                                        field.confidence * 100.0
                                    ));
                                }
                            } else {
                                ui.label("No schema details available.");
                            }

                            ui.separator();
                            ui.label("Relationships");
                            if relationships.is_empty() {
                                ui.label("none");
                            } else {
                                for relationship in relationships {
                                    ui.label(relationship);
                                }
                            }

                            ui.separator();
                            ui.label("Samples");
                            if let Some(observation) = observation {
                                ui.label(format!(
                                    "Observation Confidence: {:.0}%",
                                    observation.confidence * 100.0
                                ));
                                ui.label(format!(
                                    "Evidence Signals: {} (instances {}, sources {}, paths {}, temporal markers {})",
                                    observation.evidence.total_signals,
                                    observation.evidence.sample_instances,
                                    observation.evidence.source_signals,
                                    observation.evidence.sample_path_signals,
                                    observation.evidence.temporal_markers
                                ));
                                ui.label(format!(
                                    "Trend: {:?} (transitions {}, distinct markers {}, duplicate markers {})",
                                    observation.trend.direction,
                                    observation.trend.transitions,
                                    observation.trend.distinct_markers,
                                    observation.trend.duplicate_markers
                                ));
                                ui.label(format!(
                                    "Timeline: {} -> {}",
                                    observation.first_seen.as_deref().unwrap_or("unknown"),
                                    observation.last_seen.as_deref().unwrap_or("unknown")
                                ));

                                if observation.sample_paths.is_empty() {
                                    ui.label("none");
                                } else {
                                    for path in &observation.sample_paths {
                                        ui.monospace(path);
                                    }
                                }
                            } else {
                                ui.label("No sample paths available.");
                            }
                        }
                    } else {
                        ui.label("No entity profiles detected yet.");
                    }
                } else {
                    ui.label("Open a file to build graph intelligence.");
                }

                if let Some(err) = &self.error {
                    ui.separator();
                    ui.colored_label(egui::Color32::RED, err);
                }
            });
        }

        if self.show_bottom && !self.focus_mode {
            egui::TopBottomPanel::bottom("bottom_panel")
                .resizable(true)
                .show(ctx, |ui| {
                    self.draw_bottom_panel(ui);
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| match self.explorer_view {
            ExplorerView::Overview => self.draw_overview(ui),
            ExplorerView::Tree => {
                if self.document.is_some() {
                    self.draw_tree(ui, &rows);
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.heading("Open a structured file to start exploring");
                    });
                }
            }
            ExplorerView::Graph => {
                if let Some(graph) = &self.graph {
                    Self::draw_graph(
                        ui,
                        graph,
                        &mut self.selected_entity,
                        &mut self.show_all_graph_relationships,
                    );
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.heading("Open a file to generate the universal data graph");
                    });
                }
            }
            ExplorerView::Scan => self.draw_scan(ui),
            ExplorerView::Diff => self.draw_diff(ui),
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
                        if ui
                            .button(format!(
                                "[{}] {} — {}",
                                command.category(),
                                command.label(),
                                command.description()
                            ))
                            .clicked()
                        {
                            self.execute_palette_command(command);
                        }
                    }

                    if ui.button("Close").clicked() {
                        self.show_command_palette = false;
                    }
                });
        }

        if self.show_help_panel {
            egui::Window::new("What am I looking at?")
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("Overview: high-level inferred entities and relationships with confidence.");
                    ui.label("Tree View: raw source structure and exact data paths.");
                    ui.label("Graph View: inferred model and relationship evidence.");
                    ui.label("Scan: choose repository, target, and output location, then start an architecture scan.");
                    ui.label("Diff View: structural changes between base and comparison documents.");
                    ui.label("Bottom panel: Search, JSONPath, Stats, and live System Diff from `.treehouse/system-diff.json`.");
                    if ui.button("Close").clicked() {
                        self.show_help_panel = false;
                    }
                });
        }

        if self.current_layout() != previous_layout {
            self.save_state();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PaletteCommand {
    OpenFile,
    CompareFile,
    ConnectSystemDiff,
    DisconnectSystemDiff,
    ShowOverview,
    ShowTree,
    ShowGraph,
    ShowScan,
    ShowDiff,
    ToggleFocusMode,
    ToggleSystemDiffPanel,
    ToggleHelpPanel,
    ResetLayout,
    AddBookmark,
    ClearBookmarks,
    ClearRecentFiles,
    ClearSelection,
    ClearComparison,
}

impl PaletteCommand {
    fn label(self) -> &'static str {
        match self {
            PaletteCommand::OpenFile => "Open File",
            PaletteCommand::CompareFile => "Compare File",
            PaletteCommand::ConnectSystemDiff => "Connect Live System Diff",
            PaletteCommand::DisconnectSystemDiff => "Disconnect Live System Diff",
            PaletteCommand::ShowOverview => "Show Inferred Model Overview",
            PaletteCommand::ShowTree => "Show Raw Tree",
            PaletteCommand::ShowGraph => "Show Inferred Graph",
            PaletteCommand::ShowScan => "Show Architecture Scan",
            PaletteCommand::ShowDiff => "Compare With Base (Diff)",
            PaletteCommand::ToggleFocusMode => "Toggle Focus Mode",
            PaletteCommand::ToggleSystemDiffPanel => "Toggle Bottom Panel",
            PaletteCommand::ToggleHelpPanel => "Toggle View Help",
            PaletteCommand::ResetLayout => "Reset Layout",
            PaletteCommand::AddBookmark => "Add Bookmark",
            PaletteCommand::ClearBookmarks => "Clear Bookmarks",
            PaletteCommand::ClearRecentFiles => "Clear Recent Files",
            PaletteCommand::ClearSelection => "Clear Selection",
            PaletteCommand::ClearComparison => "Clear Comparison",
        }
    }

    fn description(self) -> &'static str {
        match self {
            PaletteCommand::OpenFile => "Open a structured document.",
            PaletteCommand::CompareFile => "Choose a comparison file for structural diff.",
            PaletteCommand::ConnectSystemDiff => {
                "Attach `.treehouse/system-diff.json` for live architecture deltas."
            }
            PaletteCommand::DisconnectSystemDiff => "Detach the active system diff feed.",
            PaletteCommand::ShowOverview => "View inferred entities/relationships first.",
            PaletteCommand::ShowTree => "Inspect raw source data paths and values.",
            PaletteCommand::ShowGraph => "Inspect inferred entities and relationship confidence.",
            PaletteCommand::ShowScan => "Configure scan location/target and start the scan.",
            PaletteCommand::ShowDiff => "Switch to the structural diff view.",
            PaletteCommand::ToggleFocusMode => "Hide extra panes for tree + inspector focus.",
            PaletteCommand::ToggleSystemDiffPanel => {
                "Show or hide bottom Search/JSONPath/Stats/System Diff panes."
            }
            PaletteCommand::ToggleHelpPanel => {
                "Explain the currently visible view in plain language."
            }
            PaletteCommand::ResetLayout => "Restore default pane visibility and tabs.",
            PaletteCommand::AddBookmark => "Bookmark the currently selected path.",
            PaletteCommand::ClearBookmarks => "Remove all saved bookmarks.",
            PaletteCommand::ClearRecentFiles => "Clear the recent files list.",
            PaletteCommand::ClearSelection => "Unselect the active tree path.",
            PaletteCommand::ClearComparison => "Clear the comparison document and diff.",
        }
    }

    fn category(self) -> &'static str {
        match self {
            PaletteCommand::OpenFile | PaletteCommand::CompareFile => "File",
            PaletteCommand::ConnectSystemDiff | PaletteCommand::DisconnectSystemDiff => {
                "System Diff"
            }
            PaletteCommand::ShowOverview
            | PaletteCommand::ShowTree
            | PaletteCommand::ShowGraph
            | PaletteCommand::ShowScan
            | PaletteCommand::ShowDiff
            | PaletteCommand::ToggleFocusMode
            | PaletteCommand::ToggleSystemDiffPanel
            | PaletteCommand::ToggleHelpPanel
            | PaletteCommand::ResetLayout => "View",
            PaletteCommand::AddBookmark
            | PaletteCommand::ClearBookmarks
            | PaletteCommand::ClearRecentFiles
            | PaletteCommand::ClearSelection
            | PaletteCommand::ClearComparison => "Manage",
        }
    }
}

fn filtered_commands(filter: &str) -> Vec<PaletteCommand> {
    let all = [
        PaletteCommand::OpenFile,
        PaletteCommand::CompareFile,
        PaletteCommand::ConnectSystemDiff,
        PaletteCommand::DisconnectSystemDiff,
        PaletteCommand::ShowOverview,
        PaletteCommand::ShowTree,
        PaletteCommand::ShowGraph,
        PaletteCommand::ShowScan,
        PaletteCommand::ShowDiff,
        PaletteCommand::ToggleFocusMode,
        PaletteCommand::ToggleSystemDiffPanel,
        PaletteCommand::ToggleHelpPanel,
        PaletteCommand::ResetLayout,
        PaletteCommand::AddBookmark,
        PaletteCommand::ClearBookmarks,
        PaletteCommand::ClearRecentFiles,
        PaletteCommand::ClearSelection,
        PaletteCommand::ClearComparison,
    ];

    let filter = filter.trim().to_lowercase();
    if filter.is_empty() {
        return all.to_vec();
    }

    all.into_iter()
        .filter(|command| {
            command.label().to_lowercase().contains(&filter)
                || command.description().to_lowercase().contains(&filter)
                || command.category().to_lowercase().contains(&filter)
        })
        .collect()
}

fn workspace_key_from_path(path: &Path) -> String {
    for ancestor in path.ancestors() {
        if ancestor.join(".git").exists() {
            return ancestor.display().to_string();
        }
    }
    path.parent()
        .map(|parent| parent.display().to_string())
        .unwrap_or_else(|| "default".to_string())
}

fn discover_system_diff_path(current_file: Option<&Path>) -> Option<(PathBuf, PathBuf)> {
    if let Some(file) = current_file {
        for ancestor in file.ancestors() {
            let candidate = ancestor.join(".treehouse/system-diff.json");
            if candidate.is_file() {
                return Some((candidate, ancestor.to_path_buf()));
            }
        }
    }
    if let Ok(cwd) = env::current_dir() {
        for ancestor in cwd.ancestors() {
            let candidate = ancestor.join(".treehouse/system-diff.json");
            if candidate.is_file() {
                return Some((candidate, ancestor.to_path_buf()));
            }
        }
    }
    None
}

fn discover_desktop_git_repos() -> Vec<PathBuf> {
    let mut repos = Vec::new();
    let desktop = match dirs::home_dir() {
        Some(home) => home.join("Desktop"),
        None => return repos,
    };
    if !desktop.is_dir() {
        return repos;
    }

    if desktop.join(".git").is_dir() {
        repos.push(desktop.clone());
    }

    let mut level_one = Vec::new();
    if let Ok(entries) = fs::read_dir(&desktop) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.join(".git").is_dir() {
                    repos.push(path.clone());
                }
                level_one.push(path);
            }
        }
    }

    for parent in level_one {
        if let Ok(entries) = fs::read_dir(parent) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join(".git").is_dir() {
                    repos.push(path);
                }
            }
        }
    }

    repos.sort();
    repos.dedup();
    repos
}

fn parse_changed_file_path(changed: &str) -> Option<&str> {
    let trimmed = changed.trim();
    if trimmed.is_empty() {
        return None;
    }
    let bytes = trimmed.as_bytes();
    if bytes.len() > GIT_STATUS_PATH_OFFSET
        && bytes[GIT_STATUS_SEPARATOR_INDEX] == b' '
        && bytes[0].is_ascii()
        && bytes[1].is_ascii()
    {
        return trimmed.get(GIT_STATUS_PATH_OFFSET..).map(str::trim);
    }
    Some(trimmed)
}

fn row_inference_badge(row: &TreeRow, graph: &Option<UniversalDataGraph>) -> Option<(String, u8)> {
    let graph = graph.as_ref()?;
    for profile in &graph.intelligence {
        let needle = profile.name.to_ascii_lowercase();
        if row.label.to_ascii_lowercase().contains(&needle)
            || row.path.to_ascii_lowercase().contains(&needle)
        {
            return Some((
                profile.name.clone(),
                (profile.confidence * 100.0).round() as u8,
            ));
        }
    }
    None
}

fn row_changed_since_snapshot(row: &TreeRow, diff_entries: &[DiffEntry]) -> bool {
    diff_entries.iter().any(|entry| {
        if entry.path == row.path {
            return true;
        }
        entry
            .path
            .strip_prefix(&row.path)
            .is_some_and(|suffix| suffix.starts_with('.'))
    })
}

fn select_adjacent_tree_row(tree_state: &mut TreeState, rows: &[TreeRow], delta: isize) {
    if rows.is_empty() {
        return;
    }

    let current_idx = tree_state
        .selected_path()
        .and_then(|selected| rows.iter().position(|row| row.path == selected));
    let next_idx = match current_idx {
        Some(idx) if delta >= 0 => idx
            .saturating_add(delta as usize)
            .min(rows.len().saturating_sub(1)),
        Some(idx) => idx.saturating_sub(delta.unsigned_abs()),
        None => {
            if delta < 0 {
                rows.len().saturating_sub(1)
            } else {
                0
            }
        }
    };
    tree_state.select_path(rows[next_idx].path.clone());
}

fn parent_path(path: &str) -> Option<String> {
    if path == "$" {
        return None;
    }
    if !path.ends_with(']') {
        if let Some(idx) = path.rfind('.') {
            if idx == 0 {
                return Some("$".to_string());
            }
            return Some(path[..idx].to_string());
        }
    }
    if let Some(idx) = path.rfind(']') {
        if let Some(start) = path[..idx].rfind('[') {
            if start == 0 {
                return Some("$".to_string());
            }
            return Some(path[..start].to_string());
        }
    }
    if let Some(idx) = path.rfind('.') {
        if idx == 0 {
            Some("$".to_string())
        } else {
            Some(path[..idx].to_string())
        }
    } else {
        Some("$".to_string())
    }
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

fn diff_summary(entry: &DiffEntry) -> String {
    match entry.kind {
        DiffKind::Added => format!(
            "added {}",
            entry
                .right
                .as_ref()
                .map(summarize_for_results)
                .unwrap_or_else(|| "value".to_string())
        ),
        DiffKind::Removed => format!(
            "removed {}",
            entry
                .left
                .as_ref()
                .map(summarize_for_results)
                .unwrap_or_else(|| "value".to_string())
        ),
        DiffKind::Changed => {
            let left = entry
                .left
                .as_ref()
                .map(summarize_for_results)
                .unwrap_or_else(|| "value".to_string());
            let right = entry
                .right
                .as_ref()
                .map(summarize_for_results)
                .unwrap_or_else(|| "value".to_string());
            format!("changed {left} → {right}")
        }
        DiffKind::TypeChanged => {
            let left = entry
                .left
                .as_ref()
                .map(summarize_for_results)
                .unwrap_or_else(|| "value".to_string());
            let right = entry
                .right
                .as_ref()
                .map(summarize_for_results)
                .unwrap_or_else(|| "value".to_string());
            format!("type changed {left} → {right}")
        }
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

    if let Err(err) = fs::create_dir_all(parent) {
        eprintln!(
            "warning: failed to create state directory {}: {err}",
            parent.display()
        );
        return;
    }

    let Ok(serialized) = serde_json::to_string_pretty(state) else {
        eprintln!("warning: failed to serialize persisted Treehouse state");
        return;
    };

    if let Err(err) = fs::write(&path, serialized) {
        eprintln!(
            "warning: failed to write persisted Treehouse state {}: {err}",
            path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_path_navigates_up() {
        assert_eq!(
            parent_path("$.orders[0].items[2].sku"),
            Some("$.orders[0].items[2]".to_string())
        );
        assert_eq!(
            parent_path("$.orders[0].items[2]"),
            Some("$.orders[0].items".to_string())
        );
        assert_eq!(parent_path("$.orders"), Some("$".to_string()));
        assert_eq!(parent_path("$"), None);
    }

    #[test]
    fn parses_git_status_changed_file_paths() {
        assert_eq!(
            parse_changed_file_path("M  crates/treehouse-app/src/main.rs"),
            Some("crates/treehouse-app/src/main.rs")
        );
        assert_eq!(parse_changed_file_path("?? README.md"), Some("README.md"));
        assert_eq!(parse_changed_file_path("README.md"), Some("README.md"));
    }

    #[test]
    fn command_filter_matches_categories_and_descriptions() {
        let commands = filtered_commands("system diff");
        assert!(commands.contains(&PaletteCommand::ConnectSystemDiff));
        assert!(commands.contains(&PaletteCommand::DisconnectSystemDiff));
        assert!(filtered_commands("scan").contains(&PaletteCommand::ShowScan));
    }
}
