use crate::cmd::common::PortForwardRule;
use crate::services;
use anyhow::Result;
use eframe::egui::{self, Align, Button, Color32, CornerRadius, Frame, Margin, RichText, Stroke, Vec2};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;

pub fn run() -> Result<()> {
        let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 720.0])
            .with_min_inner_size([720.0, 520.0])
            .with_title("RSP"),
        ..Default::default()
    };

    eframe::run_native(
        "RSP - SSH Port Forward Manager",
        options,
        Box::new(|cc| Ok(Box::new(RspGuiApp::new(cc)))),
    )
    .map_err(|err| anyhow::anyhow!(err.to_string()))
}

struct DashboardData {
    rules: Vec<(String, PortForwardRule)>,
    hosts: Vec<String>,
}

struct WorkerHandle {
    tx: Sender<WorkerCommand>,
    rx: Receiver<WorkerEvent>,
}

enum WorkerCommand {
    Refresh(Option<String>),
    Start(Vec<String>, String),
    Stop(Vec<String>, String),
}

struct WorkerEvent {
    dashboard: Option<DashboardData>,
    message: Option<UiMessage>,
    completed_rules: Vec<String>,
}

struct RspGuiApp {
    rules: Vec<(String, PortForwardRule)>,
    hosts: Vec<String>,
    form: RuleForm,
    selected: Option<String>,
    scroll_to_rule: Option<String>,
    message: Option<UiMessage>,
    last_refresh_at: Instant,
    pending_jobs: usize,
    pending_rules: HashMap<String, PendingRuleState>,
    theme_mode: ThemeMode,
    worker: WorkerHandle,
}

struct UiMessage {
    text: String,
    error: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PendingRuleState {
    Starting,
    Stopping,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ThemeMode {
    Light,
    Dark,
}

#[derive(Default)]
struct RuleForm {
    original_name: Option<String>,
    name: String,
    local_port: String,
    remote_port: String,
    remote_host: String,
}

impl RspGuiApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let theme_mode = ThemeMode::Light;
        configure_theme(&cc.egui_ctx, theme_mode);

        let worker = spawn_worker(cc.egui_ctx.clone());
        let mut app = Self {
            rules: Vec::new(),
            hosts: Vec::new(),
            form: RuleForm::default(),
            selected: None,
            scroll_to_rule: None,
            message: None,
            last_refresh_at: Instant::now(),
            pending_jobs: 0,
            pending_rules: HashMap::new(),
            theme_mode,
            worker,
        };
        app.request_refresh(None);
        app
    }

    fn apply_dashboard(&mut self, dashboard: DashboardData) {
        self.rules = dashboard.rules;
        self.hosts = dashboard.hosts;
        self.last_refresh_at = Instant::now();
        self.sort_rules();
        if self.form.remote_host.is_empty() {
            self.form.remote_host = self.hosts.first().cloned().unwrap_or_default();
        }
    }

    fn request_refresh(&mut self, summary: Option<String>) {
        if self.worker.tx.send(WorkerCommand::Refresh(summary)).is_ok() {
            self.pending_jobs += 1;
        } else {
            self.set_error("Background worker is not available".to_string());
        }
    }

    fn request_start(&mut self, names: Vec<String>, summary: String) {
        if names.is_empty() {
            return;
        }
        if let Some(name) = names.first() {
            self.selected = Some(name.clone());
            self.scroll_to_rule = Some(name.clone());
        }
        if self
            .worker
            .tx
            .send(WorkerCommand::Start(names.clone(), summary))
            .is_ok()
        {
            self.pending_jobs += 1;
            for name in names {
                self.pending_rules.insert(name, PendingRuleState::Starting);
            }
        } else {
            self.set_error("Background worker is not available".to_string());
        }
    }

    fn request_stop(&mut self, names: Vec<String>, summary: String) {
        if names.is_empty() {
            return;
        }
        if self
            .worker
            .tx
            .send(WorkerCommand::Stop(names.clone(), summary))
            .is_ok()
        {
            self.pending_jobs += 1;
            for name in names {
                self.pending_rules.insert(name, PendingRuleState::Stopping);
            }
        } else {
            self.set_error("Background worker is not available".to_string());
        }
    }

    fn process_worker_events(&mut self) {
        while let Ok(event) = self.worker.rx.try_recv() {
            self.pending_jobs = self.pending_jobs.saturating_sub(1);
            for name in event.completed_rules {
                self.pending_rules.remove(&name);
            }
            if let Some(dashboard) = event.dashboard {
                self.apply_dashboard(dashboard);
            }
            if let Some(message) = event.message {
                if message.error {
                    self.message = Some(message);
                } else {
                    self.message = None;
                }
            }
        }
    }

    fn is_busy(&self) -> bool {
        self.pending_jobs > 0
    }

    fn pending_state(&self, name: &str) -> Option<PendingRuleState> {
        self.pending_rules.get(name).copied()
    }

    fn set_theme_mode(&mut self, ctx: &egui::Context, theme_mode: ThemeMode) {
        if self.theme_mode != theme_mode {
            self.theme_mode = theme_mode;
            configure_theme(ctx, self.theme_mode);
            ctx.request_repaint();
        }
    }

    fn set_error(&mut self, text: String) {
        self.message = Some(UiMessage { text, error: true });
    }

    fn clear_form(&mut self) {
        self.form = RuleForm::default();
        self.form.remote_host = self.hosts.first().cloned().unwrap_or_default();
        self.selected = None;
        self.scroll_to_rule = None;
    }

    fn sort_rules(&mut self) {
        self.rules.sort_by(|a, b| {
            let a_running = a.1.status;
            let b_running = b.1.status;
            b_running
                .cmp(&a_running)
                .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
        });
    }

    fn upsert_rule_in_view(&mut self, name: String, rule: PortForwardRule) {
        if let Some((_, current_rule)) = self
            .rules
            .iter_mut()
            .find(|(rule_name, _)| rule_name == &name)
        {
            *current_rule = rule;
        } else {
            self.rules.push((name, rule));
        }
        self.sort_rules();
    }

    fn remove_rule_from_view(&mut self, name: &str) {
        self.rules.retain(|(rule_name, _)| rule_name != name);
        if self.selected.as_deref() == Some(name) {
            self.clear_form();
        }
    }

    fn load_into_form(&mut self, name: &str, rule: &PortForwardRule) {
        self.form.original_name = Some(name.to_string());
        self.form.name = name.to_string();
        self.form.local_port = rule.local_port.to_string();
        self.form.remote_port = rule.remote_port.to_string();
        self.form.remote_host = rule.remote_host.clone();
        self.selected = Some(name.to_string());
    }

    fn save_form(&mut self) {
        let name = self.form.name.trim().to_string();
        let remote_host = self.form.remote_host.trim().to_string();
        if name.is_empty() {
            self.set_error("Rule name cannot be empty".to_string());
            return;
        }
        if remote_host.is_empty() {
            self.set_error("Remote host cannot be empty".to_string());
            return;
        }

        let local_port = match self.form.local_port.trim().parse::<u16>() {
            Ok(value) => value,
            Err(_) => {
                self.set_error("Local port must be a valid number".to_string());
                return;
            }
        };
        let remote_port = match self.form.remote_port.trim().parse::<u16>() {
            Ok(value) => value,
            Err(_) => {
                self.set_error("Remote port must be a valid number".to_string());
                return;
            }
        };

        let rule = services::make_rule(local_port, remote_port, remote_host, false, None);
        let original_name = self.form.original_name.clone();

        let result = if let Some(original_name) = original_name.clone() {
            services::update_rule(&original_name, name, rule)
                .map(|_| format!("Rule '{}' updated", original_name))
        } else {
            services::add_rule(name.clone(), rule).map(|_| format!("Rule '{}' created", name))
        };

        match result {
            Ok(_) => {
                let updated_name = self.form.name.trim().to_string();
                let updated_rule = services::make_rule(
                    local_port,
                    remote_port,
                    self.form.remote_host.trim().to_string(),
                    false,
                    None,
                );
                if let Some(old_name) = original_name {
                    if old_name != updated_name {
                        self.remove_rule_from_view(&old_name);
                    }
                }
                self.upsert_rule_in_view(updated_name.clone(), updated_rule);
                self.selected = Some(updated_name);
                self.request_refresh(None);
                self.clear_form();
            }
            Err(err) => self.set_error(err.to_string()),
        }
    }

    fn delete_rule(&mut self, name: &str) {
        match services::remove_rule(name) {
            Ok(_) => {
                self.remove_rule_from_view(name);
                self.request_refresh(None);
            }
            Err(err) => self.set_error(err.to_string()),
        }
    }

}

impl eframe::App for RspGuiApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        configure_theme(ctx, self.theme_mode);
        self.process_worker_events();

        if self.is_busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            Frame::new()
                .fill(panel_fill(self.theme_mode))
                .stroke(Stroke::new(1.0, border_color(self.theme_mode)))
                .corner_radius(CornerRadius::same(22))
                .inner_margin(Margin::symmetric(20, 14))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                Button::new(RichText::new("New Rule").color(button_text(self.theme_mode, true)))
                                    .fill(primary_button_fill(self.theme_mode))
                                    .stroke(Stroke::NONE)
                                    .corner_radius(CornerRadius::same(16)),
                            )
                            .clicked()
                        {
                            self.clear_form();
                            ctx.request_repaint();
                        }
                        if ui
                            .add(
                                Button::new(RichText::new("Refresh").color(button_text(self.theme_mode, false)))
                                    .fill(secondary_button_fill(self.theme_mode))
                                    .stroke(Stroke::new(1.0, button_stroke(self.theme_mode)))
                                    .corner_radius(CornerRadius::same(16)),
                            )
                            .clicked()
                        {
                            self.request_refresh(Some("Rules refreshed".to_string()));
                            ctx.request_repaint();
                        }

                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            theme_segment(ui, self.theme_mode, "Dark", self.theme_mode == ThemeMode::Dark)
                                .clicked()
                                .then(|| self.set_theme_mode(ctx, ThemeMode::Dark));
                            theme_segment(ui, self.theme_mode, "Light", self.theme_mode == ThemeMode::Light)
                                .clicked()
                                .then(|| self.set_theme_mode(ctx, ThemeMode::Light));
                        });
                    });
                });
        });

        egui::SidePanel::right("editor")
            .resizable(true)
            .default_width(380.0)
            .min_width(300.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        Frame::new()
                            .fill(card_fill(self.theme_mode))
                            .corner_radius(CornerRadius::same(28))
                            .stroke(Stroke::new(1.0, border_color(self.theme_mode)))
                            .inner_margin(Margin::same(24))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(if self.form.original_name.is_some() {
                                        "Rule Detail"
                                    } else {
                                        "Compose New Tunnel"
                                    })
                                        .size(20.0)
                                        .strong()
                                        .color(primary_text(self.theme_mode)),
                                );
                                ui.label(
                                    RichText::new(if self.form.original_name.is_some() {
                                        "Secondary inspector for editing the selected rule without losing the full directory view."
                                    } else {
                                        "Compose a tunnel here, then return to the main directory to run or revise it."
                                    })
                                        .size(12.0)
                                        .color(secondary_text(self.theme_mode)),
                                );
                                ui.add_space(16.0);
                                Frame::new()
                                    .fill(subtle_fill(self.theme_mode))
                                    .corner_radius(CornerRadius::same(20))
                                    .stroke(Stroke::new(1.0, border_color(self.theme_mode)))
                                    .inner_margin(Margin::same(18))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new("Identity")
                                                .size(12.0)
                                                .color(secondary_text(self.theme_mode)),
                                        );
                                        ui.add_space(10.0);
                                        ui.label(RichText::new("Rule Name").color(primary_text(self.theme_mode)));
                                        ui.text_edit_singleline(&mut self.form.name);
                                        ui.add_space(14.0);
                                        ui.label(RichText::new("Remote Host").color(primary_text(self.theme_mode)));
                                        if self.hosts.is_empty() {
                                            ui.text_edit_singleline(&mut self.form.remote_host);
                                        } else {
                                            egui::ComboBox::from_id_salt("remote-host-main")
                                                .width(ui.available_width())
                                                .selected_text(if self.form.remote_host.is_empty() {
                                                    "Select a host"
                                                } else {
                                                    &self.form.remote_host
                                                })
                                                .show_ui(ui, |ui| {
                                                    for host in &self.hosts {
                                                        ui.selectable_value(
                                                            &mut self.form.remote_host,
                                                            host.clone(),
                                                            host,
                                                        );
                                                    }
                                                });
                                        }
                                    });

                                ui.add_space(16.0);

                                Frame::new()
                                    .fill(subtle_fill(self.theme_mode))
                                    .corner_radius(CornerRadius::same(20))
                                    .stroke(Stroke::new(1.0, border_color(self.theme_mode)))
                                    .inner_margin(Margin::same(18))
                                    .show(ui, |ui| {
                                        let compact = ui.available_width() < 340.0;
                                        ui.label(
                                            RichText::new("Port Mapping")
                                                .size(12.0)
                                                .color(secondary_text(self.theme_mode)),
                                        );
                                        ui.add_space(10.0);
                                        if compact {
                                            ui.label(RichText::new("Local Port").color(primary_text(self.theme_mode)));
                                            ui.text_edit_singleline(&mut self.form.local_port);
                                            ui.add_space(10.0);
                                            ui.label(RichText::new("Remote Port").color(primary_text(self.theme_mode)));
                                            ui.text_edit_singleline(&mut self.form.remote_port);
                                        } else {
                                            ui.columns(2, |columns| {
                                                columns[0].label(RichText::new("Local Port").color(primary_text(self.theme_mode)));
                                                columns[0].text_edit_singleline(&mut self.form.local_port);
                                                columns[1].label(RichText::new("Remote Port").color(primary_text(self.theme_mode)));
                                                columns[1].text_edit_singleline(&mut self.form.remote_port);
                                            });
                                        }
                                    });

                                ui.add_space(16.0);
                                ui.horizontal_wrapped(|ui| {
                                    let save_label = if self.form.original_name.is_some() {
                                        "Save Changes"
                                    } else {
                                        "Create Rule"
                                    };
                                    if ui
                                        .add(
                                            Button::new(RichText::new(save_label).color(button_text(self.theme_mode, true)))
                                                .fill(primary_button_fill(self.theme_mode))
                                                .stroke(Stroke::NONE)
                                                .corner_radius(CornerRadius::same(18))
                                                .min_size(egui::vec2(140.0, 40.0)),
                                        )
                                        .clicked()
                                    {
                                        self.save_form();
                                    }
                                    if ui
                                        .add(
                                            Button::new(RichText::new("Reset").color(button_text(self.theme_mode, false)))
                                                .fill(secondary_button_fill(self.theme_mode))
                                                .stroke(Stroke::new(1.0, button_stroke(self.theme_mode)))
                                                .corner_radius(CornerRadius::same(18))
                                                .min_size(egui::vec2(112.0, 40.0)),
                                        )
                                        .clicked()
                                    {
                                        self.clear_form();
                                    }
                                });

                                ui.add_space(10.0);
                                if let Some(message) = &self.message {
                                    let color = if message.error {
                                        error_text(self.theme_mode)
                                    } else {
                                        success_text(self.theme_mode)
                                    };
                                    Frame::new()
                                        .fill(if message.error {
                                            error_fill(self.theme_mode)
                                        } else {
                                            success_fill(self.theme_mode)
                                        })
                                        .corner_radius(CornerRadius::same(16))
                                        .inner_margin(Margin::same(14))
                                        .show(ui, |ui| {
                                            ui.colored_label(color, &message.text);
                                        });
                                }

                                ui.add_space(20.0);
                                Frame::new()
                                    .fill(list_item_fill(self.theme_mode))
                                    .corner_radius(CornerRadius::same(18))
                                    .inner_margin(Margin::same(16))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new("Storage")
                                                .size(12.0)
                                                .color(secondary_text(self.theme_mode)),
                                        );
                                        ui.add_space(6.0);
                                        ui.label(RichText::new("Hosts are read from ~/.ssh/config").color(secondary_text(self.theme_mode)));
                                        ui.label(RichText::new("Rules are persisted to ~/.rsp.json").color(secondary_text(self.theme_mode)));
                                    });
                            });
                        });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            Frame::new()
                .fill(card_fill(self.theme_mode))
                .corner_radius(CornerRadius::same(28))
                .stroke(Stroke::new(1.0, border_color(self.theme_mode)))
                .inner_margin(Margin::same(28))
                .show(ui, |ui| {
                    let rows = self.rules.clone();
                    let rows_is_empty = rows.is_empty();

                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("Rules")
                                    .size(28.0)
                                    .strong()
                                    .color(primary_text(self.theme_mode)),
                            );
                            ui.label(
                                RichText::new("The main stage: scan, start, stop, inspect and select a rule to edit.")
                                    .size(14.0)
                                    .color(secondary_text(self.theme_mode)),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            Frame::new()
                                .fill(subtle_fill(self.theme_mode))
                                .corner_radius(CornerRadius::same(255))
                                .inner_margin(Margin::symmetric(12, 8))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(format!("{} visible", rows.len()))
                                            .size(12.0)
                                            .color(secondary_text(self.theme_mode)),
                                    );
                                });
                        });
                    });

                    ui.add_space(20.0);

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (name, rule) in rows {
                            let is_selected = self.selected.as_deref() == Some(name.as_str());
                            let pending = self.pending_state(&name);
                            let (status, fill, text) = match pending {
                                Some(PendingRuleState::Starting) => (
                                    "Starting...",
                                    pending_fill(self.theme_mode),
                                    pending_text(self.theme_mode),
                                ),
                                Some(PendingRuleState::Stopping) => (
                                    "Stopping...",
                                    neutral_badge_fill(self.theme_mode),
                                    neutral_badge_text(self.theme_mode),
                                ),
                                None if rule.status => (
                                    "Running",
                                    success_fill(self.theme_mode),
                                    success_text(self.theme_mode),
                                ),
                                None => (
                                    "Stopped",
                                    warning_fill(self.theme_mode),
                                    warning_text(self.theme_mode),
                                ),
                            };

                            let card_response = Frame::new()
                                .fill(if is_selected {
                                    selected_fill(self.theme_mode)
                                } else {
                                    card_fill(self.theme_mode)
                                })
                                .corner_radius(CornerRadius::same(24))
                                .stroke(Stroke::new(
                                    1.0,
                                    if is_selected {
                                        selected_stroke(self.theme_mode)
                                    } else {
                                        border_color(self.theme_mode)
                                    },
                                ))
                                .inner_margin(Margin::symmetric(18, 16))
                                .show(ui, |ui| {
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.vertical(|ui| {
                                                let title = RichText::new(&name)
                                                    .size(20.0)
                                                    .strong()
                                                    .color(primary_text(self.theme_mode));
                                                let title_button = Button::new(title)
                                                    .fill(Color32::TRANSPARENT)
                                                    .stroke(Stroke::NONE)
                                                    .corner_radius(CornerRadius::same(0));
                                                if ui.add(title_button).clicked() {
                                                    self.load_into_form(&name, &rule);
                                                }
                                            });

                                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                                badge(ui, status, fill, text);
                                            });
                                        });

                                        ui.add_space(8.0);
                                        Frame::new()
                                            .fill(list_item_fill(self.theme_mode))
                                            .corner_radius(CornerRadius::same(14))
                                            .inner_margin(Margin::symmetric(12, 6))
                                            .show(ui, |ui| {
                                                ui.horizontal_wrapped(|ui| {
                                                    meta_label(ui, self.theme_mode, "Local", &rule.local_port.to_string());
                                                    meta_label(ui, self.theme_mode, "Remote", &rule.remote_port.to_string());
                                                    meta_label(ui, self.theme_mode, "PID", &rule.pid.map(|pid| pid.to_string()).unwrap_or_else(|| "-".to_string()));
                                                });
                                            });

                                        ui.add_space(10.0);
                                        ui.horizontal_wrapped(|ui| {
                                            let action = match pending {
                                                Some(PendingRuleState::Starting) => "Starting...",
                                                Some(PendingRuleState::Stopping) => "Stopping...",
                                                None if rule.status => "Stop",
                                                None => "Start",
                                            };
                                            let action_button = if rule.status {
                                                Button::new(RichText::new(action).color(button_text(self.theme_mode, false)))
                                                    .fill(danger_button_fill(self.theme_mode))
                                                    .stroke(Stroke::new(1.0, danger_button_stroke(self.theme_mode)))
                                                    .corner_radius(CornerRadius::same(18))
                                                    .min_size(egui::vec2(88.0, 34.0))
                                            } else {
                                                Button::new(RichText::new(action).color(button_text(self.theme_mode, true)))
                                                    .fill(primary_button_fill(self.theme_mode))
                                                    .stroke(Stroke::NONE)
                                                    .corner_radius(CornerRadius::same(18))
                                                    .min_size(egui::vec2(88.0, 34.0))
                                            };
                                            let clicked = ui.add_enabled(pending.is_none(), action_button).clicked();
                                            if clicked {
                                                if rule.status {
                                                    self.request_stop(
                                                        vec![name.clone()],
                                                        format!("Rule '{}' stopped", name),
                                                    );
                                                } else {
                                                    self.request_start(
                                                        vec![name.clone()],
                                                        format!("Rule '{}' started", name),
                                                    );
                                                }
                                                ctx.request_repaint();
                                            }

                                            if ui
                                                .add(
                                                    Button::new(RichText::new("Edit").color(button_text(self.theme_mode, false)))
                                                        .fill(secondary_button_fill(self.theme_mode))
                                                        .stroke(Stroke::new(1.0, button_stroke(self.theme_mode)))
                                                        .corner_radius(CornerRadius::same(18))
                                                        .min_size(egui::vec2(72.0, 34.0)),
                                                )
                                                .clicked()
                                            {
                                                self.load_into_form(&name, &rule);
                                                ctx.request_repaint();
                                            }

                                            if ui
                                                .add(
                                                    Button::new(RichText::new("Delete").color(button_text(self.theme_mode, false)))
                                                        .fill(danger_button_fill(self.theme_mode))
                                                        .stroke(Stroke::new(1.0, danger_button_stroke(self.theme_mode)))
                                                        .corner_radius(CornerRadius::same(18))
                                                        .min_size(egui::vec2(76.0, 34.0)),
                                                )
                                                .clicked()
                                            {
                                                self.delete_rule(&name);
                                                ctx.request_repaint();
                                            }
                                        });
                                    });
                                });
                            if self.scroll_to_rule.as_deref() == Some(name.as_str()) && rule.status {
                                ui.scroll_to_rect(card_response.response.rect, Some(Align::TOP));
                                self.scroll_to_rule = None;
                            }
                            ui.add_space(10.0);
                        }

                        if rows_is_empty {
                            ui.add_space(40.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("No saved rules")
                                        .size(20.0)
                                        .color(secondary_text(self.theme_mode)),
                                );
                                ui.label(
                                    RichText::new("Use the inspector on the right to create your first tunnel.")
                                        .color(secondary_text(self.theme_mode)),
                                );
                            });
                        }
                    });
                });
        });
    }
}

fn spawn_worker(ctx: egui::Context) -> WorkerHandle {
    let (command_tx, command_rx) = mpsc::channel::<WorkerCommand>();
    let (event_tx, event_rx) = mpsc::channel::<WorkerEvent>();

    thread::spawn(move || {
        while let Ok(command) = command_rx.recv() {
            let event = match command {
                WorkerCommand::Refresh(summary) => match services::load_dashboard() {
                    Ok((rules, hosts)) => WorkerEvent {
                        dashboard: Some(DashboardData { rules, hosts }),
                        message: summary.map(|text| UiMessage { text, error: false }),
                        completed_rules: Vec::new(),
                    },
                    Err(err) => WorkerEvent {
                        dashboard: None,
                        message: Some(UiMessage {
                            text: err.to_string(),
                            error: true,
                        }),
                        completed_rules: Vec::new(),
                    },
                },
                WorkerCommand::Start(names, summary) => match services::start_rules(&names)
                    .and_then(|_| services::load_dashboard())
                {
                    Ok((rules, hosts)) => WorkerEvent {
                        dashboard: Some(DashboardData { rules, hosts }),
                        message: Some(UiMessage {
                            text: summary,
                            error: false,
                        }),
                        completed_rules: names,
                    },
                    Err(err) => WorkerEvent {
                        dashboard: services::load_dashboard()
                            .ok()
                            .map(|(rules, hosts)| DashboardData { rules, hosts }),
                        message: Some(UiMessage {
                            text: err.to_string(),
                            error: true,
                        }),
                        completed_rules: names,
                    },
                },
                WorkerCommand::Stop(names, summary) => match services::stop_rules(&names)
                    .and_then(|_| services::load_dashboard())
                {
                    Ok((rules, hosts)) => WorkerEvent {
                        dashboard: Some(DashboardData { rules, hosts }),
                        message: Some(UiMessage {
                            text: summary,
                            error: false,
                        }),
                        completed_rules: names,
                    },
                    Err(err) => WorkerEvent {
                        dashboard: services::load_dashboard()
                            .ok()
                            .map(|(rules, hosts)| DashboardData { rules, hosts }),
                        message: Some(UiMessage {
                            text: err.to_string(),
                            error: true,
                        }),
                        completed_rules: names,
                    },
                },
            };

            if event_tx.send(event).is_err() {
                break;
            }
            ctx.request_repaint();
        }
    });

    WorkerHandle {
        tx: command_tx,
        rx: event_rx,
    }
}

fn configure_theme(ctx: &egui::Context, theme_mode: ThemeMode) {
    let mut visuals = match theme_mode {
        ThemeMode::Light => egui::Visuals::light(),
        ThemeMode::Dark => egui::Visuals::dark(),
    };
    visuals.window_fill = theme_color(theme_mode, 243, 244, 247, 28, 31, 36);
    visuals.panel_fill = theme_color(theme_mode, 243, 244, 247, 28, 31, 36);
    visuals.faint_bg_color = theme_color(theme_mode, 248, 249, 250, 36, 39, 44);
    visuals.extreme_bg_color = theme_color(theme_mode, 255, 255, 255, 20, 23, 27);
    visuals.widgets.active.bg_fill = theme_color(theme_mode, 79, 86, 92, 228, 232, 238);
    visuals.widgets.active.fg_stroke.color = theme_color(theme_mode, 255, 255, 255, 22, 26, 31);
    visuals.widgets.hovered.bg_fill = theme_color(theme_mode, 96, 104, 110, 208, 214, 222);
    visuals.widgets.hovered.fg_stroke.color = theme_color(theme_mode, 255, 255, 255, 20, 24, 28);
    visuals.widgets.inactive.bg_fill = theme_color(theme_mode, 245, 246, 248, 40, 44, 50);
    visuals.widgets.inactive.fg_stroke.color = theme_color(theme_mode, 56, 61, 70, 226, 229, 234);
    visuals.widgets.noninteractive.bg_fill = theme_color(theme_mode, 252, 252, 253, 24, 27, 31);
    visuals.widgets.noninteractive.fg_stroke.color = theme_color(theme_mode, 56, 61, 70, 226, 229, 234);
    visuals.selection.bg_fill = theme_color(theme_mode, 227, 231, 229, 62, 72, 77);
    visuals.window_corner_radius = CornerRadius::same(20);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(12.0, 12.0);
    style.spacing.button_padding = Vec2::new(14.0, 9.0);
    style.spacing.window_margin = Margin::same(14);
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(10);
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(10);
    style.visuals.widgets.active.corner_radius = CornerRadius::same(10);
    ctx.set_style(style);
}

fn theme_color(theme_mode: ThemeMode, lr: u8, lg: u8, lb: u8, dr: u8, dg: u8, db: u8) -> Color32 {
    match theme_mode {
        ThemeMode::Light => Color32::from_rgb(lr, lg, lb),
        ThemeMode::Dark => Color32::from_rgb(dr, dg, db),
    }
}

fn theme_segment(ui: &mut egui::Ui, theme_mode: ThemeMode, label: &str, selected: bool) -> egui::Response {
    ui.add(
        Button::new(RichText::new(label).color(button_text(theme_mode, selected)))
            .fill(if selected {
                primary_button_fill(theme_mode)
            } else {
                secondary_button_fill(theme_mode)
            })
            .stroke(Stroke::new(1.0, button_stroke(theme_mode)))
            .corner_radius(CornerRadius::same(16)),
    )
}

fn badge(ui: &mut egui::Ui, label: &str, fill: Color32, text: Color32) {
    Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(255))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.label(
                RichText::new(label)
                    .size(12.0)
                    .strong()
                    .color(text),
            );
        });
}

fn meta_label(ui: &mut egui::Ui, theme_mode: ThemeMode, key: &str, value: &str) {
    ui.label(
        RichText::new(format!("{key} {value}"))
            .size(12.0)
            .color(secondary_text(theme_mode)),
    );
}

fn panel_fill(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 247, 248, 250, 30, 33, 38)
}

fn card_fill(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 252, 252, 252, 24, 27, 31)
}

fn subtle_fill(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 240, 242, 246, 36, 40, 46)
}

fn list_item_fill(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 248, 249, 250, 32, 35, 40)
}

fn selected_fill(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 246, 247, 248, 40, 44, 50)
}

fn border_color(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 228, 230, 235, 54, 59, 66)
}

fn selected_stroke(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 210, 214, 218, 84, 92, 101)
}

fn primary_text(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 28, 32, 38, 236, 239, 243)
}

fn secondary_text(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 92, 99, 110, 176, 182, 191)
}

fn primary_button_fill(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 79, 86, 92, 222, 227, 233)
}

fn secondary_button_fill(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 242, 244, 247, 46, 50, 57)
}

fn button_stroke(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 225, 227, 232, 68, 74, 82)
}

fn button_text(theme_mode: ThemeMode, primary: bool) -> Color32 {
    if primary {
        theme_color(theme_mode, 250, 250, 249, 18, 22, 26)
    } else {
        theme_color(theme_mode, 54, 60, 68, 232, 236, 241)
    }
}

fn danger_button_fill(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 244, 240, 240, 68, 44, 46)
}

fn danger_button_stroke(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 233, 224, 224, 102, 64, 68)
}

fn success_fill(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 236, 246, 240, 31, 55, 42)
}

fn success_text(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 43, 115, 79, 154, 216, 180)
}

fn warning_fill(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 246, 242, 231, 61, 52, 33)
}

fn warning_text(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 136, 101, 38, 225, 198, 138)
}

fn error_fill(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 252, 243, 242, 67, 35, 37)
}

fn error_text(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 184, 52, 48, 246, 167, 164)
}

fn neutral_badge_fill(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 241, 242, 244, 52, 56, 62)
}

fn neutral_badge_text(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 105, 111, 121, 196, 201, 210)
}

fn pending_fill(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 236, 242, 238, 39, 53, 46)
}

fn pending_text(theme_mode: ThemeMode) -> Color32 {
    theme_color(theme_mode, 82, 109, 89, 172, 202, 180)
}
