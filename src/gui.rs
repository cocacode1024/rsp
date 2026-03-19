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
            .with_min_inner_size([900.0, 560.0])
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
                .fill(Color32::from_rgb(246, 247, 249))
                .stroke(Stroke::new(1.0, Color32::from_rgb(226, 228, 233)))
                .inner_margin(Margin::symmetric(20, 16))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new("RSP")
                                    .size(24.0)
                                    .strong()
                                    .color(Color32::from_rgb(30, 34, 42)),
                            );
                            ui.label(
                                RichText::new("SSH port forwarding, shaped like a native desktop tool")
                                    .color(Color32::from_rgb(108, 114, 126)),
                            );
                        });
                        ui.add_space(12.0);
                        let status_text = if self.pending_jobs > 0 {
                            format!("{} task running", self.pending_jobs)
                        } else {
                            format!("{} rules", self.rules.len())
                        };
                        Frame::new()
                            .fill(Color32::from_rgb(235, 239, 245))
                            .corner_radius(CornerRadius::same(255))
                            .inner_margin(Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(status_text)
                                        .size(12.0)
                                        .color(Color32::from_rgb(83, 91, 107)),
                                );
                            });
                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add(
                                    Button::new("New Rule")
                                        .fill(Color32::from_rgb(55, 96, 208))
                                        .stroke(Stroke::NONE)
                                        .corner_radius(CornerRadius::same(10)),
                                )
                                .clicked()
                            {
                                self.clear_form();
                                ctx.request_repaint();
                            }
                            if ui
                                .add(
                                    Button::new("Refresh")
                                        .fill(Color32::from_rgb(239, 241, 245))
                                        .stroke(Stroke::new(1.0, Color32::from_rgb(221, 224, 230)))
                                        .corner_radius(CornerRadius::same(10)),
                                )
                                .clicked()
                            {
                                self.request_refresh(Some("Rules refreshed".to_string()));
                                ctx.request_repaint();
                            }
                        });
                    });
                });
        });

        egui::SidePanel::right("editor")
            .resizable(true)
            .default_width(372.0)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(Color32::from_rgb(252, 252, 253))
                    .corner_radius(CornerRadius::same(20))
                    .stroke(Stroke::new(1.0, Color32::from_rgb(226, 228, 233)))
                    .inner_margin(Margin::same(20))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(if self.form.original_name.is_some() {
                                "Rule Editor"
                            } else {
                                "New Rule"
                            })
                            .size(22.0)
                            .strong()
                            .color(Color32::from_rgb(34, 38, 46)),
                        );
                        ui.label(
                            RichText::new("Create or adjust a tunnel without leaving the desktop window")
                                .color(Color32::from_rgb(108, 114, 126)),
                        );
                        ui.add_space(18.0);

                        Frame::new()
                            .fill(Color32::from_rgb(246, 247, 249))
                            .corner_radius(CornerRadius::same(16))
                            .stroke(Stroke::new(1.0, Color32::from_rgb(229, 231, 236)))
                            .inner_margin(Margin::same(16))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new("Connection")
                                        .size(12.0)
                                        .color(Color32::from_rgb(117, 123, 135)),
                                );
                                ui.add_space(8.0);

                                ui.label(RichText::new("Rule Name").color(Color32::from_rgb(70, 76, 89)));
                                ui.text_edit_singleline(&mut self.form.name);
                                ui.add_space(10.0);

                                ui.label(RichText::new("Remote Host").color(Color32::from_rgb(70, 76, 89)));
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
                                ui.add_space(10.0);

                                ui.columns(2, |columns| {
                                    columns[0].label(
                                        RichText::new("Local Port").color(Color32::from_rgb(70, 76, 89)),
                                    );
                                    columns[0].text_edit_singleline(&mut self.form.local_port);
                                    columns[1].label(
                                        RichText::new("Remote Port").color(Color32::from_rgb(70, 76, 89)),
                                    );
                                    columns[1].text_edit_singleline(&mut self.form.remote_port);
                                });
                            });

                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            let save_label = if self.form.original_name.is_some() {
                                "Update Rule"
                            } else {
                                "Create Rule"
                            };
                            if ui
                                .add(
                                    Button::new(save_label)
                                        .fill(Color32::from_rgb(55, 96, 208))
                                        .stroke(Stroke::NONE)
                                        .corner_radius(CornerRadius::same(10)),
                                )
                                .clicked()
                            {
                                self.save_form();
                            }
                            if ui
                                .add(
                                    Button::new("Reset")
                                        .fill(Color32::from_rgb(239, 241, 245))
                                        .stroke(Stroke::new(1.0, Color32::from_rgb(221, 224, 230)))
                                        .corner_radius(CornerRadius::same(10)),
                                )
                                .clicked()
                            {
                                self.clear_form();
                            }
                        });

                        ui.add_space(18.0);
                        ui.separator();
                        ui.add_space(10.0);
                        if let Some(message) = &self.message {
                            let color = if message.error {
                                Color32::from_rgb(184, 52, 48)
                            } else {
                                Color32::from_rgb(41, 124, 85)
                            };
                            Frame::new()
                                .fill(if message.error {
                                    Color32::from_rgb(255, 242, 241)
                                } else {
                                    Color32::from_rgb(239, 249, 244)
                                })
                                .corner_radius(CornerRadius::same(12))
                                .inner_margin(Margin::same(12))
                                .show(ui, |ui| {
                                    ui.colored_label(color, &message.text);
                                });
                            ui.add_space(10.0);
                        }
                        ui.label(
                            RichText::new("Data Source")
                                .size(12.0)
                                .color(Color32::from_rgb(117, 123, 135)),
                        );
                        ui.label(
                            RichText::new("Hosts: ~/.ssh/config").color(Color32::from_rgb(108, 114, 126)),
                        );
                        ui.label(
                            RichText::new("Rules: ~/.rsp.json").color(Color32::from_rgb(108, 114, 126)),
                        );
                    });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            Frame::new()
                .fill(Color32::from_rgb(252, 252, 253))
                .corner_radius(CornerRadius::same(20))
                .stroke(Stroke::new(1.0, Color32::from_rgb(226, 228, 233)))
                .inner_margin(Margin::same(18))
                .show(ui, |ui| {
                    let rows = self.rules.clone();
                    let rows_is_empty = rows.is_empty();

                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Rules")
                                .size(22.0)
                                .strong()
                                .color(Color32::from_rgb(34, 38, 46)),
                        );
                        ui.label(
                            RichText::new(format!("{} visible", rows.len()))
                                .color(Color32::from_rgb(108, 114, 126)),
                        );
                    });
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Each tunnel keeps its own state, PID and action controls.")
                            .color(Color32::from_rgb(125, 130, 141)),
                    );
                    ui.add_space(14.0);

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (name, rule) in rows {
                            let is_selected = self.selected.as_deref() == Some(name.as_str());
                            let pending = self.pending_state(&name);
                            let (status, fill, text) = match pending {
                                Some(PendingRuleState::Starting) => (
                                    "Starting...",
                                    Color32::from_rgb(233, 241, 255),
                                    Color32::from_rgb(56, 93, 177),
                                ),
                                Some(PendingRuleState::Stopping) => (
                                    "Stopping...",
                                    Color32::from_rgb(239, 240, 243),
                                    Color32::from_rgb(109, 114, 124),
                                ),
                                None if rule.status => (
                                    "Running",
                                    Color32::from_rgb(231, 246, 237),
                                    Color32::from_rgb(39, 125, 82),
                                ),
                                None => (
                                    "Stopped",
                                    Color32::from_rgb(248, 241, 222),
                                    Color32::from_rgb(151, 106, 28),
                                ),
                            };

                            Frame::new()
                                .fill(if is_selected {
                                    Color32::from_rgb(245, 248, 255)
                                } else {
                                    Color32::from_rgb(248, 249, 251)
                                })
                                .corner_radius(CornerRadius::same(18))
                                .stroke(Stroke::new(
                                    1.0,
                                    if is_selected {
                                        Color32::from_rgb(185, 205, 255)
                                    } else {
                                        Color32::from_rgb(229, 231, 236)
                                    },
                                ))
                                .inner_margin(Margin::same(16))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            let label = if is_selected {
                                                RichText::new(&name)
                                                    .size(17.0)
                                                    .strong()
                                                    .color(Color32::from_rgb(36, 67, 126))
                                            } else {
                                                RichText::new(&name)
                                                    .size(17.0)
                                                    .strong()
                                                    .color(Color32::from_rgb(34, 38, 46))
                                            };
                                            if ui.selectable_label(is_selected, label).clicked() {
                                                self.load_into_form(&name, &rule);
                                            }
                                            ui.add_space(4.0);
                                            ui.horizontal_wrapped(|ui| {
                                                ui.label(
                                                    RichText::new(format!("Host {}", rule.remote_host))
                                                        .color(Color32::from_rgb(108, 114, 126)),
                                                );
                                                ui.label(
                                                    RichText::new(format!("Local {}", rule.local_port))
                                                        .color(Color32::from_rgb(108, 114, 126)),
                                                );
                                                ui.label(
                                                    RichText::new(format!("Remote {}", rule.remote_port))
                                                        .color(Color32::from_rgb(108, 114, 126)),
                                                );
                                                ui.label(
                                                    RichText::new(format!(
                                                        "PID {}",
                                                        rule.pid
                                                            .map(|pid| pid.to_string())
                                                            .unwrap_or_else(|| "-".to_string())
                                                    ))
                                                    .color(Color32::from_rgb(108, 114, 126)),
                                                );
                                            });
                                        });

                                        ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                            if ui
                                                .add(
                                                    Button::new("Delete")
                                                        .fill(Color32::from_rgb(246, 238, 238))
                                                        .stroke(Stroke::new(1.0, Color32::from_rgb(235, 212, 212)))
                                                        .corner_radius(CornerRadius::same(10)),
                                                )
                                                .clicked()
                                            {
                                                self.delete_rule(&name);
                                                ctx.request_repaint();
                                            }
                                            if ui
                                                .add(
                                                    Button::new("Edit")
                                                        .fill(Color32::from_rgb(239, 241, 245))
                                                        .stroke(Stroke::new(1.0, Color32::from_rgb(221, 224, 230)))
                                                        .corner_radius(CornerRadius::same(10)),
                                                )
                                                .clicked()
                                            {
                                                self.load_into_form(&name, &rule);
                                                ctx.request_repaint();
                                            }

                                            let action = match pending {
                                                Some(PendingRuleState::Starting) => "Starting...",
                                                Some(PendingRuleState::Stopping) => "Stopping...",
                                                None if rule.status => "Stop",
                                                None => "Start",
                                            };
                                            let action_button = if rule.status {
                                                Button::new(action)
                                                    .fill(Color32::from_rgb(244, 236, 236))
                                                    .stroke(Stroke::new(1.0, Color32::from_rgb(235, 217, 217)))
                                                    .corner_radius(CornerRadius::same(10))
                                            } else {
                                                Button::new(action)
                                                    .fill(Color32::from_rgb(55, 96, 208))
                                                    .stroke(Stroke::NONE)
                                                    .corner_radius(CornerRadius::same(10))
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

                                            Frame::new()
                                                .fill(fill)
                                                .corner_radius(CornerRadius::same(255))
                                                .inner_margin(Margin::symmetric(10, 6))
                                                .show(ui, |ui| {
                                                    ui.label(
                                                        RichText::new(status)
                                                            .size(12.0)
                                                            .strong()
                                                            .color(text),
                                                    );
                                                });
                                        });
                                    });
                                });
                            ui.add_space(10.0);
                        }

                        if rows_is_empty {
                            ui.add_space(20.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    RichText::new("No matching rules")
                                        .size(18.0)
                                        .color(Color32::from_rgb(108, 114, 126)),
                                );
                                ui.label("Create a new rule from the editor on the right.");
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
    visuals.window_fill = Color32::from_rgb(241, 243, 247);
    visuals.panel_fill = Color32::from_rgb(241, 243, 247);
    visuals.faint_bg_color = Color32::from_rgb(247, 248, 250);
    visuals.extreme_bg_color = Color32::from_rgb(255, 255, 255);
    visuals.widgets.active.bg_fill = Color32::from_rgb(55, 96, 208);
    visuals.widgets.active.fg_stroke.color = Color32::WHITE;
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(73, 113, 222);
    visuals.widgets.hovered.fg_stroke.color = Color32::WHITE;
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(246, 247, 249);
    visuals.widgets.inactive.fg_stroke.color = Color32::from_rgb(54, 59, 69);
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(252, 252, 253);
    visuals.widgets.noninteractive.fg_stroke.color = Color32::from_rgb(54, 59, 69);
    visuals.selection.bg_fill = Color32::from_rgb(214, 228, 255);
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
