// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use anyhow::{Result, bail};

use crate::{AppMode, FormKind, FormPayload, TabKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatVisibility {
    Hidden,
    Visible,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub mode: AppMode,
    pub active_tab: TabKind,
    pub show_deleted: bool,
    pub chat: ChatVisibility,
    pub status_line: Option<String>,
    pub form_payload: Option<FormPayload>,
    pub form_submission_count: usize,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            mode: AppMode::Nav,
            active_tab: TabKind::Dashboard,
            show_deleted: false,
            chat: ChatVisibility::Hidden,
            status_line: None,
            form_payload: None,
            form_submission_count: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppCommand {
    NextTab,
    PrevTab,
    FirstTab,
    LastTab,
    SetActiveTab(TabKind),
    EnterEditMode,
    ExitToNav,
    OpenForm(FormKind),
    SetFormPayload(FormPayload),
    SubmitForm,
    CancelForm,
    ToggleDeleted,
    OpenChat,
    CloseChat,
    SetStatus(String),
    ClearStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    ModeChanged(AppMode),
    TabChanged(TabKind),
    ChatVisibilityChanged(ChatVisibility),
    DeletedFilterChanged(bool),
    FormPayloadSet(FormKind),
    FormSubmitted(FormKind),
    FormCanceled(FormKind),
    StatusUpdated(String),
    StatusCleared,
}

impl AppState {
    pub fn dispatch(&mut self, command: AppCommand) -> Vec<AppEvent> {
        match command {
            AppCommand::NextTab => self.rotate_tab(1),
            AppCommand::PrevTab => self.rotate_tab(-1),
            AppCommand::FirstTab => {
                self.active_tab = TabKind::ALL[0];
                vec![AppEvent::TabChanged(self.active_tab)]
            }
            AppCommand::LastTab => {
                self.active_tab = *TabKind::ALL.last().expect("tabs array is non-empty");
                vec![AppEvent::TabChanged(self.active_tab)]
            }
            AppCommand::SetActiveTab(tab) => {
                self.active_tab = tab;
                vec![AppEvent::TabChanged(self.active_tab)]
            }
            AppCommand::EnterEditMode => {
                self.mode = AppMode::Edit;
                self.form_payload = None;
                vec![AppEvent::ModeChanged(self.mode)]
            }
            AppCommand::ExitToNav => {
                self.mode = AppMode::Nav;
                self.form_payload = None;
                vec![AppEvent::ModeChanged(self.mode), self.set_status("nav")]
            }
            AppCommand::OpenForm(kind) => {
                self.mode = AppMode::Form(kind);
                self.form_payload = FormPayload::blank_for(kind);
                let mut events = vec![AppEvent::ModeChanged(self.mode)];
                if self.form_payload.is_some() {
                    events.push(AppEvent::FormPayloadSet(kind));
                }
                events
            }
            AppCommand::SetFormPayload(payload) => {
                let kind = payload.kind();
                match self.mode {
                    AppMode::Form(active_kind) if active_kind == kind => {
                        self.form_payload = Some(payload);
                        vec![AppEvent::FormPayloadSet(kind)]
                    }
                    AppMode::Form(_) => {
                        vec![self.set_status("form kind mismatch")]
                    }
                    _ => {
                        vec![self.set_status("form not open")]
                    }
                }
            }
            AppCommand::SubmitForm => self.submit_form(),
            AppCommand::CancelForm => {
                if let AppMode::Form(kind) = self.mode {
                    self.mode = AppMode::Nav;
                    self.form_payload = None;
                    vec![
                        AppEvent::ModeChanged(self.mode),
                        AppEvent::FormCanceled(kind),
                        self.set_status("form canceled"),
                    ]
                } else {
                    vec![self.set_status("form not open")]
                }
            }
            AppCommand::ToggleDeleted => {
                self.show_deleted = !self.show_deleted;
                let label = if self.show_deleted {
                    "deleted shown"
                } else {
                    "deleted hidden"
                };
                vec![
                    AppEvent::DeletedFilterChanged(self.show_deleted),
                    self.set_status(label),
                ]
            }
            AppCommand::OpenChat => {
                self.chat = ChatVisibility::Visible;
                vec![
                    AppEvent::ChatVisibilityChanged(self.chat),
                    self.set_status("chat open"),
                ]
            }
            AppCommand::CloseChat => {
                self.chat = ChatVisibility::Hidden;
                vec![
                    AppEvent::ChatVisibilityChanged(self.chat),
                    self.set_status("chat hidden"),
                ]
            }
            AppCommand::ClearStatus => {
                self.status_line = None;
                vec![AppEvent::StatusCleared]
            }
            AppCommand::SetStatus(message) => {
                self.status_line = Some(message.clone());
                vec![AppEvent::StatusUpdated(message)]
            }
        }
    }

    pub fn validated_form_payload(&self) -> Result<FormPayload> {
        let AppMode::Form(active_kind) = self.mode else {
            bail!("form not open -- press `a` on a tab that supports forms");
        };

        let Some(payload) = self.form_payload.clone() else {
            bail!("form payload missing -- fill out form fields and retry");
        };

        if payload.kind() != active_kind {
            bail!("form payload does not match active form -- reopen the form and retry");
        }

        payload.validate()?;
        Ok(payload)
    }

    fn submit_form(&mut self) -> Vec<AppEvent> {
        let AppMode::Form(kind) = self.mode else {
            return vec![self.set_status("form not open")];
        };

        let Some(payload) = &self.form_payload else {
            return vec![self.set_status("form payload missing")];
        };

        if payload.kind() != kind {
            return vec![self.set_status("form payload does not match active form")];
        }

        if let Err(error) = payload.validate() {
            return vec![self.set_status(&format!("form invalid: {}", error))];
        }

        self.form_submission_count += 1;
        self.mode = AppMode::Nav;
        self.form_payload = None;
        vec![
            AppEvent::ModeChanged(self.mode),
            AppEvent::FormSubmitted(kind),
            self.set_status("form submitted"),
        ]
    }

    fn rotate_tab(&mut self, delta: isize) -> Vec<AppEvent> {
        let tabs = TabKind::ALL;
        let current = tabs
            .iter()
            .position(|tab| *tab == self.active_tab)
            .unwrap_or(0) as isize;
        let len = tabs.len() as isize;
        let next = (current + delta).rem_euclid(len) as usize;
        self.active_tab = tabs[next];
        vec![AppEvent::TabChanged(self.active_tab)]
    }

    fn set_status(&mut self, message: &str) -> AppEvent {
        self.status_line = Some(message.to_owned());
        AppEvent::StatusUpdated(message.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::{AppCommand, AppEvent, AppState, ChatVisibility};
    use crate::{
        AppMode, FormKind, FormPayload, MaintenanceCategoryId, ProjectFormInput, ProjectStatus,
        ProjectTypeId, TabKind,
    };

    #[test]
    fn tab_rotation_wraps() {
        let mut state = AppState {
            active_tab: TabKind::Settings,
            ..AppState::default()
        };

        let events = state.dispatch(AppCommand::NextTab);
        assert_eq!(state.active_tab, TabKind::Dashboard);
        assert_eq!(events, vec![AppEvent::TabChanged(TabKind::Dashboard)]);
    }

    #[test]
    fn first_last_and_set_active_tab_commands_update_active_tab() {
        let mut state = AppState::default();

        let first = state.dispatch(AppCommand::FirstTab);
        assert_eq!(state.active_tab, TabKind::Dashboard);
        assert_eq!(first, vec![AppEvent::TabChanged(TabKind::Dashboard)]);

        let last = state.dispatch(AppCommand::LastTab);
        assert_eq!(state.active_tab, TabKind::Settings);
        assert_eq!(last, vec![AppEvent::TabChanged(TabKind::Settings)]);

        let set = state.dispatch(AppCommand::SetActiveTab(TabKind::Maintenance));
        assert_eq!(state.active_tab, TabKind::Maintenance);
        assert_eq!(set, vec![AppEvent::TabChanged(TabKind::Maintenance)]);
    }

    #[test]
    fn toggle_deleted_updates_status() {
        let mut state = AppState::default();

        let events = state.dispatch(AppCommand::ToggleDeleted);
        assert!(state.show_deleted);
        assert_eq!(
            events,
            vec![
                AppEvent::DeletedFilterChanged(true),
                AppEvent::StatusUpdated("deleted shown".to_owned()),
            ],
        );
    }

    #[test]
    fn open_and_close_chat() {
        let mut state = AppState::default();

        let opened = state.dispatch(AppCommand::OpenChat);
        assert_eq!(state.chat, ChatVisibility::Visible);
        assert_eq!(
            opened,
            vec![
                AppEvent::ChatVisibilityChanged(ChatVisibility::Visible),
                AppEvent::StatusUpdated("chat open".to_owned()),
            ],
        );

        let closed = state.dispatch(AppCommand::CloseChat);
        assert_eq!(state.chat, ChatVisibility::Hidden);
        assert_eq!(
            closed,
            vec![
                AppEvent::ChatVisibilityChanged(ChatVisibility::Hidden),
                AppEvent::StatusUpdated("chat hidden".to_owned()),
            ],
        );
    }

    #[test]
    fn mode_transitions() {
        let mut state = AppState::default();

        state.dispatch(AppCommand::EnterEditMode);
        assert_eq!(state.mode, AppMode::Edit);

        state.dispatch(AppCommand::OpenForm(FormKind::Project));
        assert_eq!(state.mode, AppMode::Form(FormKind::Project));

        state.dispatch(AppCommand::ExitToNav);
        assert_eq!(state.mode, AppMode::Nav);
    }

    #[test]
    fn submit_form_transitions_to_nav() {
        let mut state = AppState::default();
        state.dispatch(AppCommand::OpenForm(FormKind::Project));
        let payload = FormPayload::Project(ProjectFormInput {
            title: "Kitchen refresh".to_owned(),
            project_type_id: ProjectTypeId::new(1),
            status: ProjectStatus::Planned,
            description: String::new(),
            start_date: None,
            end_date: None,
            budget_cents: Some(1_000_000),
            actual_cents: None,
        });
        state.dispatch(AppCommand::SetFormPayload(payload));

        let events = state.dispatch(AppCommand::SubmitForm);
        assert_eq!(state.mode, AppMode::Nav);
        assert!(state.form_payload.is_none());
        assert_eq!(state.form_submission_count, 1);
        assert!(events.contains(&AppEvent::FormSubmitted(FormKind::Project)));
    }

    #[test]
    fn submit_form_reports_validation_error() {
        let mut state = AppState::default();
        state.dispatch(AppCommand::OpenForm(FormKind::Project));
        let payload = FormPayload::Project(ProjectFormInput {
            title: String::new(),
            project_type_id: ProjectTypeId::new(0),
            status: ProjectStatus::Planned,
            description: String::new(),
            start_date: None,
            end_date: None,
            budget_cents: None,
            actual_cents: None,
        });
        state.dispatch(AppCommand::SetFormPayload(payload));

        let events = state.dispatch(AppCommand::SubmitForm);
        assert_eq!(state.mode, AppMode::Form(FormKind::Project));
        assert!(events
            .iter()
            .any(|event| matches!(event, AppEvent::StatusUpdated(message) if message.contains("form invalid"))));
    }

    #[test]
    fn cancel_form_returns_to_nav() {
        let mut state = AppState::default();
        state.dispatch(AppCommand::OpenForm(FormKind::MaintenanceItem));
        state.dispatch(AppCommand::SetFormPayload(FormPayload::Maintenance(
            crate::MaintenanceItemFormInput {
                name: "Filter".to_owned(),
                category_id: MaintenanceCategoryId::new(1),
                appliance_id: None,
                last_serviced_at: None,
                interval_months: 3,
                manual_url: String::new(),
                manual_text: String::new(),
                notes: String::new(),
                cost_cents: None,
            },
        )));

        let events = state.dispatch(AppCommand::CancelForm);
        assert_eq!(state.mode, AppMode::Nav);
        assert!(events.contains(&AppEvent::FormCanceled(FormKind::MaintenanceItem)));
    }

    #[test]
    fn set_status_command_updates_status_line() {
        let mut state = AppState::default();
        let events = state.dispatch(AppCommand::SetStatus("db loaded".to_owned()));
        assert_eq!(state.status_line.as_deref(), Some("db loaded"));
        assert_eq!(
            events,
            vec![AppEvent::StatusUpdated("db loaded".to_owned())]
        );
    }

    #[test]
    fn validated_form_payload_returns_error_when_form_not_open() {
        let state = AppState::default();
        let error = state
            .validated_form_payload()
            .expect_err("no open form should fail");
        assert!(error.to_string().contains("form not open"));
    }
}
