use crate::cmd::common::PortForwardRule;
use crate::services;
use anyhow::Result;
use eframe::egui::{self, Align, Color32, CornerRadius, Frame, Margin, RichText, Stroke, Vec2};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;

pub fn run() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 720.0])
            .with_min_inner_size([900.0, 560.0])
            .with_title("RSP - SSH Port Forward Manager"),
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
    message: Option<UiMessage>,
    last_refresh_at: Instant,
    pending_jobs: usize,
    pending_rules: HashMap<String, PendingRuleState>,
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
        configure_theme(&cc.egui_ctx);

        let worker = spawn_worker(cc.egui_ctx.clone());
        let mut app = Self {
            rules: Vec::new(),
            hosts: Vec::new(),
            form: RuleForm::default(),
            selected: None,
            message: None,
            last_refresh_at: Instant::now(),
            pending_jobs: 0,
            pending_rules: HashMap::new(),
            worker,
        };
        app.request_refresh(None);
        app
    }

    fn apply_dashboard(&mut self, dashboard: DashboardData) {
        self.rules = dashboard.rules;
        self.hosts = dashboard.hosts;
        self.last_refresh_at = Instant::now();
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

    fn set_error(&mut self, text: String) {
        self.message = Some(UiMessage { text, error: true });
    }

    fn clear_form(&mut self) {
        self.form = RuleForm::default();
        self.form.remote_host = self.hosts.first().cloned().unwrap_or_default();
        self.selected = None;
    }

    fn sort_rules(&mut self) {
        self.rules.sort_by(|a, b| a.0.cmp(&b.0));
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
        configure_theme(ctx);
        self.process_worker_events();

        if self.is_busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            Frame::new()
                .fill(Color32::from_rgb(244, 239, 229))
                .inner_margin(Margin::symmetric(18, 16))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("RSP")
                                    .size(28.0)
                                    .strong()
                                    .color(Color32::from_rgb(34, 59, 45)),
                            );
                            ui.label(
                                RichText::new("SSH port forwarding with a real desktop control plane")
                                    .color(Color32::from_rgb(97, 91, 78)),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Stop All").clicked() {
                                let names = services::all_rule_names(&self.rules);
                                self.request_stop(names, "All rules stopped".to_string());
                                ctx.request_repaint();
                            }
                            if ui.button("Start All").clicked() {
                                let names = services::all_rule_names(&self.rules);
                                self.request_start(names, "All rules started".to_string());
                                ctx.request_repaint();
                            }
                            if ui.button("New Rule").clicked() {
                                self.clear_form();
                                ctx.request_repaint();
                            }
                            if ui.button("Refresh").clicked() {
                                self.request_refresh(Some("Rules refreshed".to_string()));
                                ctx.request_repaint();
                            }
                        });
                    });
                });
        });

        egui::SidePanel::right("editor")
            .resizable(true)
            .default_width(360.0)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(Color32::from_rgb(247, 244, 237))
                    .corner_radius(CornerRadius::same(18))
                    .inner_margin(Margin::same(18))
                    .show(ui, |ui| {
                        ui.heading(if self.form.original_name.is_some() {
                            "Edit Rule"
                        } else {
                            "Create Rule"
                        });
                        ui.label(
                            RichText::new("Create or adjust a tunnel without leaving the desktop window")
                                .color(Color32::from_rgb(97, 91, 78)),
                        );
                        ui.add_space(12.0);

                        ui.label("Rule Name");
                        ui.text_edit_singleline(&mut self.form.name);
                        ui.add_space(8.0);

                        ui.label("Remote Host");
                        if self.hosts.is_empty() {
                            ui.text_edit_singleline(&mut self.form.remote_host);
                        } else {
                            egui::ComboBox::from_id_salt("remote-host")
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
                        ui.add_space(8.0);

                        ui.columns(2, |columns| {
                            columns[0].label("Local Port");
                            columns[0].text_edit_singleline(&mut self.form.local_port);
                            columns[1].label("Remote Port");
                            columns[1].text_edit_singleline(&mut self.form.remote_port);
                        });

                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            let save_label = if self.form.original_name.is_some() {
                                "Update Rule"
                            } else {
                                "Create Rule"
                            };
                            if ui.button(save_label).clicked() {
                                self.save_form();
                            }
                            if ui.button("Reset").clicked() {
                                self.clear_form();
                            }
                        });

                        ui.add_space(18.0);
                        ui.separator();
                        ui.add_space(10.0);
                        if let Some(message) = &self.message {
                            let color = if message.error {
                                Color32::from_rgb(196, 47, 47)
                            } else {
                                Color32::from_rgb(42, 125, 73)
                            };
                            ui.colored_label(color, &message.text);
                            ui.add_space(10.0);
                        }
                        ui.label("Data source");
                        ui.label(
                            RichText::new("Hosts: ~/.ssh/config").color(Color32::from_rgb(97, 91, 78)),
                        );
                        ui.label(
                            RichText::new("Rules: ~/.rsp.json").color(Color32::from_rgb(97, 91, 78)),
                        );
                    });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            Frame::new()
                .fill(Color32::from_rgb(252, 250, 246))
                .corner_radius(CornerRadius::same(18))
                .stroke(Stroke::new(1.0, Color32::from_rgb(225, 219, 208)))
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    let rows = self.rules.clone();
                    let rows_is_empty = rows.is_empty();

                    ui.horizontal(|ui| {
                        ui.heading("Rules");
                        ui.label(
                            RichText::new(format!("{} visible", rows.len()))
                                .color(Color32::from_rgb(97, 91, 78)),
                        );
                    });
                    ui.add_space(10.0);

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        egui::Grid::new("rules-grid")
                            .num_columns(7)
                            .min_col_width(70.0)
                            .spacing([12.0, 12.0])
                            .show(ui, |ui| {
                                ui.strong("Rule");
                                ui.strong("Local");
                                ui.strong("Remote");
                                ui.strong("Host");
                                ui.strong("Status");
                                ui.strong("PID");
                                ui.strong("Actions");
                                ui.end_row();

                                for (name, rule) in rows {
                                    let is_selected = self.selected.as_deref() == Some(name.as_str());
                                    let label = if is_selected {
                                        RichText::new(&name)
                                            .strong()
                                            .color(Color32::from_rgb(32, 62, 99))
                                    } else {
                                        RichText::new(&name)
                                    };
                                    if ui.selectable_label(is_selected, label).clicked() {
                                        self.load_into_form(&name, &rule);
                                    }
                                    ui.label(rule.local_port.to_string());
                                    ui.label(rule.remote_port.to_string());
                                    ui.label(rule.remote_host.clone());

                                    let (status, fill, text) = match self.pending_state(&name) {
                                        Some(PendingRuleState::Starting) => (
                                            "Starting...",
                                            Color32::from_rgb(225, 235, 248),
                                            Color32::from_rgb(48, 92, 150),
                                        ),
                                        Some(PendingRuleState::Stopping) => (
                                            "Stopping...",
                                            Color32::from_rgb(236, 235, 232),
                                            Color32::from_rgb(108, 105, 98),
                                        ),
                                        None if rule.status => (
                                            "Running",
                                            Color32::from_rgb(226, 242, 231),
                                            Color32::from_rgb(42, 125, 73),
                                        ),
                                        None => (
                                            "Stopped",
                                            Color32::from_rgb(248, 234, 207),
                                            Color32::from_rgb(145, 98, 24),
                                        ),
                                    };
                                    Frame::new()
                                        .fill(fill)
                                        .corner_radius(CornerRadius::same(255))
                                        .inner_margin(Margin::symmetric(8, 4))
                                        .show(ui, |ui| {
                                            ui.label(RichText::new(status).color(text));
                                        });

                                    ui.label(
                                        rule.pid
                                            .map(|pid| pid.to_string())
                                            .unwrap_or_else(|| "-".to_string()),
                                    );

                                    ui.horizontal(|ui| {
                                        let pending = self.pending_state(&name);
                                        let action = match pending {
                                            Some(PendingRuleState::Starting) => "Starting...",
                                            Some(PendingRuleState::Stopping) => "Stopping...",
                                            None if rule.status => "Stop",
                                            None => "Start",
                                        };
                                        let clicked = ui
                                            .add_enabled(pending.is_none(), egui::Button::new(action))
                                            .clicked();
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

                                        if ui.button("Edit").clicked() {
                                            self.load_into_form(&name, &rule);
                                            ctx.request_repaint();
                                        }
                                        if ui.button("Delete").clicked() {
                                            self.delete_rule(&name);
                                            ctx.request_repaint();
                                        }
                                    });
                                    ui.end_row();
                                }
                            });

                        if rows_is_empty {
                            ui.add_space(20.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("No matching rules")
                                        .size(18.0)
                                        .color(Color32::from_rgb(97, 91, 78)),
                                );
                                ui.label("Try another keyword or create a new rule.");
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

fn configure_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.window_fill = Color32::from_rgb(243, 239, 232);
    visuals.panel_fill = Color32::from_rgb(243, 239, 232);
    visuals.faint_bg_color = Color32::from_rgb(235, 229, 219);
    visuals.extreme_bg_color = Color32::from_rgb(252, 250, 246);
    visuals.widgets.active.bg_fill = Color32::from_rgb(70, 106, 145);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(87, 122, 160);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(233, 227, 217);
    visuals.widgets.noninteractive.fg_stroke.color = Color32::from_rgb(52, 51, 48);
    visuals.selection.bg_fill = Color32::from_rgb(198, 220, 238);
    visuals.window_corner_radius = CornerRadius::same(18);
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(10.0, 10.0);
    style.spacing.button_padding = Vec2::new(12.0, 8.0);
    style.spacing.window_margin = Margin::same(14);
    ctx.set_style(style);
}
