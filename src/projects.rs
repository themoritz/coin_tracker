use std::sync::{
    mpsc::{channel, Receiver, Sender},
    Arc,
};

use chrono::{DateTime, Local, Utc};
use egui::{mutex::Mutex, Button, Context, Id, Label, TextEdit, Ui};
use egui_extras::{Column, TableBuilder};
use uuid::Uuid;

use crate::{export, modal, notifications::NotifyExt, style};

pub struct Projects {
    sender: Sender<Msg>,
    receiver: Arc<Mutex<Receiver<Msg>>>,
    projects: Vec<Project>,
    open_project: Uuid,
    window_open: bool,
    input_new_name: Option<String>,
    input_import_json: Option<String>,
    input_rename: Option<String>,
    input_confirm_delete: bool,
}

impl Projects {
    pub fn new(ctx: &Context) -> Self {
        let (sender, receiver) = channel();
        ctx.data_mut(|d| d.insert_temp(Id::NULL, ProjectsSender(sender.clone())));

        let project = Project::new("Unnamed".to_string());
        let open_project = project.id;

        Self {
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
            projects: vec![project],
            open_project,
            window_open: true,
            input_new_name: None,
            input_import_json: None,
            input_rename: None,
            input_confirm_delete: false,
        }
    }

    fn with_current(&mut self, f: impl FnOnce(&mut Project)) {
        let i = self
            .projects
            .iter()
            .position(|p| p.id == self.open_project)
            .unwrap();
        f(&mut self.projects[i]);
    }

    fn current(&self) -> &Project {
        &self
            .projects
            .iter()
            .find(|p| p.id == self.open_project)
            .unwrap()
    }

    fn apply_update(&mut self, msg: Msg) {
        match msg {
            Msg::New { name, data } => {
                let mut p = Project::new(name);
                if let Some(data) = data {
                    p.data = data;
                }
                self.open_project = p.id;
                self.projects.push(p);
            }
            Msg::UpdateData { data } => {
                self.with_current(|p| p.data = data);
            }
            Msg::Select { id } => {
                self.open_project = id;
            }
            Msg::Rename { name } => {
                self.with_current(|p| p.name = name);
            }
            Msg::TogglePublic => {
                self.with_current(|p| p.is_public = !p.is_public);
            }
            Msg::Delete => {
                self.projects.retain(|p| p.id != self.open_project);
                if let Some(p) = self.projects.first() {
                    self.open_project = p.id;
                } else {
                    self.apply_update(Msg::New {
                        name: "Unnamed".to_string(),
                        data: None,
                    });
                }
            }
        }
    }

    pub fn show_toggle(&mut self, ui: &mut egui::Ui) {
        if ui.selectable_label(self.window_open, "Projects").clicked() {
            self.window_open = !self.window_open;
        }
    }

    pub fn show_window(&mut self, ctx: &Context) {
        let mut open = self.window_open;
        egui::Window::new("Projects")
            .open(&mut open)
            .show(ctx, |ui| self.show_ui(ui));
        self.window_open = open;
    }

    fn show_ui(&mut self, ui: &mut Ui) {
        let receiver = self.receiver.clone();
        for msg in receiver.lock().try_iter() {
            self.apply_update(msg);
        }

        TableBuilder::new(ui)
            .striped(true)
            .resizable(false)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(
                Column::remainder()
                    .at_least(60.0)
                    .clip(true)
                    .resizable(false),
            )
            .column(Column::auto())
            .column(Column::auto().at_least(10.0))
            .sense(egui::Sense::click())
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Name");
                });
                header.col(|ui| {
                    ui.strong("Created");
                });
                header.col(|ui| {
                    ui.strong("Public?");
                });
            })
            .body(|mut body| {
                for project in &self.projects {
                    body.row(20.0, |mut row| {
                        row.set_selected(project.id == self.open_project);

                        row.col(|ui| {
                            ui.add(Label::new(project.name.clone()).selectable(false));
                        });
                        row.col(|ui| {
                            ui.add(
                                Label::new(
                                    project
                                        .created_at
                                        .with_timezone(&Local)
                                        .format("%Y-%m-%d %H:%M")
                                        .to_string(),
                                )
                                .selectable(false),
                            );
                        });
                        row.col(|ui| {
                            if project.is_public {
                                ui.add(Label::new("✔").selectable(false));
                            } else {
                                ui.add(Label::new("").selectable(false));
                            }
                        });

                        if row.response().clicked() {
                            self.sender.send(Msg::Select { id: project.id }).unwrap();
                        }
                    });
                }
            });

        ui.add_space(3.0);

        ui.horizontal(|ui| {
            if ui.button("New Project").clicked() {
                self.input_new_name = Some("".to_string());
            }
            if let Some(name) = &self.input_new_name {
                let old_name = name.clone();
                let mut new_name = name.clone();
                modal::show(&ui.ctx(), "New Project", |ui| {
                    ui.add(TextEdit::singleline(&mut new_name).hint_text("Project name..."));

                    ui.add_space(3.0);

                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.input_new_name = None;
                        }
                        if ui
                            .add_enabled(!new_name.is_empty(), Button::new("Create"))
                            .clicked()
                        {
                            self.sender
                                .send(Msg::New {
                                    name: new_name.clone(),
                                    data: None,
                                })
                                .unwrap();
                            self.input_new_name = None;
                        }
                    });
                });
                if new_name != old_name {
                    self.input_new_name = Some(new_name);
                }
            }

            if ui.button("Import JSON").clicked() {
                self.input_import_json = Some("".to_string());
            }
            if let Some(json) = &self.input_import_json {
                let old_json = json.clone();
                let mut new_json = json.clone();
                modal::show(&ui.ctx(), "Import Project", |ui| {

                    let theme = egui_extras::syntax_highlighting::CodeTheme::from_style(ui.style());

                    let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
                        let mut layout_job = egui_extras::syntax_highlighting::highlight(
                            ui.ctx(),
                            &theme,
                            string,
                            "toml",
                        );
                        layout_job.wrap.max_width = wrap_width;
                        ui.fonts(|f| f.layout_job(layout_job))
                    };

                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut new_json)
                                .font(style::get(ui).font_id())
                                .desired_rows(10)
                                .lock_focus(true)
                                .desired_width(f32::INFINITY)
                                .layouter(&mut layouter),
                        );
                    });

                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.input_import_json = None;
                        }
                        if ui
                            .add_enabled(!new_json.is_empty(), Button::new("Import"))
                            .clicked()
                        {
                            match export::Project::import(&new_json) {
                                Ok(data) => {
                                    self.sender
                                        .send(Msg::New {
                                            name: "JSON import".to_string(),
                                            data: Some(data),
                                        })
                                        .unwrap();
                                    self.input_import_json = None;
                                }
                                Err(e) => {
                                    ui.ctx().notify_error("Could not import JSON", Some(e));
                                }
                            }
                        }
                    });
                });
                if new_json != old_json {
                    self.input_import_json = Some(new_json);
                }
            }
        });

        ui.separator();
        ui.strong("Current Project:");

        ui.horizontal(|ui| {
            if ui.button("Rename").clicked() {
                self.input_rename = Some(self.current().name.to_string());
            }
            if let Some(name) = &self.input_rename {
                let old_name = name.clone();
                let mut new_name = name.clone();
                modal::show(&ui.ctx(), "Rename Project", |ui| {
                    ui.add(TextEdit::singleline(&mut new_name).hint_text("Project name..."));

                    ui.add_space(3.0);

                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.input_rename = None;
                        }
                        if ui
                            .add_enabled(!new_name.is_empty(), Button::new("Rename"))
                            .clicked()
                        {
                            self.sender
                                .send(Msg::Rename {
                                    name: new_name.clone(),
                                })
                                .unwrap();
                            self.input_rename = None;
                        }
                    });
                });
                if new_name != old_name {
                    self.input_rename = Some(new_name);
                }
            }

            if ui.button("Delete").clicked() {
                self.input_confirm_delete = true;
            }
            if self.input_confirm_delete {
                modal::show(&ui.ctx(), "Delete Project", |ui| {
                    ui.label("Are you sure you want to delete the current project?");

                    ui.add_space(3.0);

                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.input_confirm_delete = false;
                        }
                        if ui.button("Delete").clicked() {
                            self.sender.send(Msg::Delete).unwrap();
                            self.input_confirm_delete = false;
                        }
                    });
                });
            }

            let mut is_public = self.current().is_public;
            if ui.checkbox(&mut is_public, "Public").clicked() {
                self.sender.send(Msg::TogglePublic).unwrap();
            }

            if ui.button("Export JSON").clicked() {
                let current = self.current();
                ui.output_mut(|o| o.copied_text = current.data.export());
                ui.ctx()
                    .notify_success(format!("Exported project `{}` to clipboard.", current.name));
            }
        });
    }
}

enum Msg {
    New {
        name: String,
        data: Option<export::Project>,
    },
    UpdateData {
        data: export::Project,
    },
    Select {
        id: Uuid,
    },
    Rename {
        name: String,
    },
    TogglePublic,
    Delete,
}

#[derive(Clone)]
struct Project {
    is_owned: bool,
    is_public: bool,
    data: export::Project,
    id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
}

impl Project {
    fn new(name: String) -> Self {
        Project {
            is_owned: true,
            is_public: false,
            data: export::Project::default(),
            id: Uuid::now_v7(),
            name,
            created_at: Utc::now(),
        }
    }
}

#[derive(Clone)]
struct ProjectsSender(Sender<Msg>);

struct ProjectsHandle;

impl ProjectsHandle {
    pub fn update_project(ctx: &Context, data: export::Project) {
        if let Some(ProjectsSender(sender)) = ctx.data(|d| d.get_temp(Id::NULL)) {
            sender.send(Msg::UpdateData { data }).unwrap();
        }
    }
}
