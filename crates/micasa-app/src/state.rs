// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use crate::{AppMode, FormKind, TabKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatVisibility {
    Hidden,
    Visible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppState {
    pub mode: AppMode,
    pub active_tab: TabKind,
    pub show_deleted: bool,
    pub chat: ChatVisibility,
    pub status_line: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            mode: AppMode::Nav,
            active_tab: TabKind::Dashboard,
            show_deleted: false,
            chat: ChatVisibility::Hidden,
            status_line: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommand {
    NextTab,
    PrevTab,
    EnterEditMode,
    ExitToNav,
    OpenForm(FormKind),
    ToggleDeleted,
    OpenChat,
    CloseChat,
    ClearStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    ModeChanged(AppMode),
    TabChanged(TabKind),
    ChatVisibilityChanged(ChatVisibility),
    DeletedFilterChanged(bool),
    StatusUpdated(String),
    StatusCleared,
}

impl AppState {
    pub fn dispatch(&mut self, command: AppCommand) -> Vec<AppEvent> {
        match command {
            AppCommand::NextTab => self.rotate_tab(1),
            AppCommand::PrevTab => self.rotate_tab(-1),
            AppCommand::EnterEditMode => {
                self.mode = AppMode::Edit;
                vec![AppEvent::ModeChanged(self.mode)]
            }
            AppCommand::ExitToNav => {
                self.mode = AppMode::Nav;
                vec![AppEvent::ModeChanged(self.mode), self.set_status("nav")]
            }
            AppCommand::OpenForm(kind) => {
                self.mode = AppMode::Form(kind);
                vec![AppEvent::ModeChanged(self.mode)]
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
        }
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
    use crate::{AppMode, FormKind, TabKind};

    #[test]
    fn tab_rotation_wraps() {
        let mut state = AppState {
            active_tab: TabKind::Documents,
            ..AppState::default()
        };

        let events = state.dispatch(AppCommand::NextTab);
        assert_eq!(state.active_tab, TabKind::Dashboard);
        assert_eq!(events, vec![AppEvent::TabChanged(TabKind::Dashboard)]);
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
}
