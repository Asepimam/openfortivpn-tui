use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::config::VpnProfile;

use super::ProfileForm;

#[derive(Debug, Clone, PartialEq)]
pub enum VpnState {
    Disconnected,
    Connecting,
    WaitingCert,
    WaitingToken,
    Connected,
    Disconnecting,
    Error(String),
}

impl VpnState {
    pub fn label(&self) -> &str {
        match self {
            VpnState::Disconnected => "DISCONNECTED",
            VpnState::Connecting => "CONNECTING...",
            VpnState::WaitingCert => "CERT UNTRUSTED",
            VpnState::WaitingToken => "WAITING TOKEN",
            VpnState::Connected => "CONNECTED",
            VpnState::Disconnecting => "DISCONNECTING...",
            VpnState::Error(_) => "ERROR",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CertInfo {
    pub hash: String,
    pub subject_cn: String,
    pub subject_org: String,
    pub issuer_cn: String,
    pub raw_lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiMode {
    ProfileList,
    NewProfile,
    EditProfile,
    Connect,
    Help,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    ProfileList,
    ProfileName,
    Host,
    Port,
    Username,
    Password,
    SudoPassword,
    SavePassword,
    UseSudoPassword,
    Connect,
    Disconnect,
    Logs,
    CertAccept,
    CertDeny,
    TokenInput,
    HelpPopup,
}

#[derive(Debug)]
pub enum AppEvent {
    LogLine(String),
    DebugLog(String),
    StateChanged(VpnState),
    NeedToken,
    CertError(CertInfo),
}

#[derive(Debug, Clone, Default)]
pub struct ConnectionForm {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub sudo_password: String,
    pub token_input: String,
    pub trusted_cert: Option<String>,
}

impl ConnectionForm {
    pub fn new() -> Self {
        Self {
            port: 443,
            ..Self::default()
        }
    }

    pub fn host_port_display(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    pub fn update_host_port(&mut self, host_port: &str) {
        let (host, port) = parse_host_port(host_port);
        self.host = host;
        self.port = port;
    }

    pub fn apply_profile(&mut self, profile: &VpnProfile) {
        self.host = profile.host.clone();
        self.port = profile.port;
        self.username = profile.username.clone();
        self.password = profile.password.clone();
        self.sudo_password = profile.sudo_password.clone();
        self.trusted_cert = profile.trusted_cert.clone();
    }

    pub fn is_ready_for_connect(&self) -> bool {
        !self.host.is_empty() && !self.username.is_empty() && !self.password.is_empty()
    }

    pub fn sudo_password_option(&self) -> Option<String> {
        (!self.sudo_password.is_empty()).then(|| self.sudo_password.clone())
    }
}

#[derive(Debug)]
pub struct RuntimeState {
    pub vpn_pid: Arc<Mutex<Option<u32>>>,
    pub waiting_for_input_flag: Arc<Mutex<bool>>,
    pub debug_enabled: bool,
    pub should_quit: bool,
}

#[derive(Debug)]
pub struct App {
    pub connection: ConnectionForm,
    pub profile_form: ProfileForm,
    pub ui_mode: UiMode,
    pub previous_ui_mode: Option<UiMode>,
    pub focus: Focus,
    pub show_password: bool,
    pub vpn_state: VpnState,
    pub logs: Vec<String>,
    pub log_scroll: usize,
    pub notification: Option<(String, NotifLevel)>,
    pub notification_ttl: u8,
    pub profiles: Vec<VpnProfile>,
    pub selected_profile_index: usize,
    pub delete_confirmation: Option<String>,
    pub pending_cert: Option<CertInfo>,
    pub event_tx: mpsc::UnboundedSender<AppEvent>,
    pub event_rx: mpsc::UnboundedReceiver<AppEvent>,
    pub runtime: RuntimeState,
}

#[derive(Debug, Clone)]
pub enum NotifLevel {
    Info,
    Success,
    Warning,
    Error,
}

impl App {
    pub fn new(debug_enabled: bool) -> Self {
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        Self {
            connection: ConnectionForm::new(),
            profile_form: ProfileForm::new(),
            ui_mode: UiMode::ProfileList,
            previous_ui_mode: None,
            focus: Focus::ProfileList,
            show_password: false,
            vpn_state: VpnState::Disconnected,
            logs: Vec::new(),
            log_scroll: 0,
            notification: None,
            notification_ttl: 0,
            profiles: Vec::new(),
            selected_profile_index: 0,
            delete_confirmation: None,
            pending_cert: None,
            event_tx,
            event_rx,
            runtime: RuntimeState {
                vpn_pid: Arc::new(Mutex::new(None)),
                waiting_for_input_flag: Arc::new(Mutex::new(false)),
                debug_enabled,
                should_quit: false,
            },
        }
    }

    pub fn push_log(&mut self, line: impl Into<String>) {
        let line = line.into();
        tracing::info!("{}", line);
        self.logs.push(line);
        if !self.logs.is_empty() {
            self.log_scroll = self.logs.len().saturating_sub(1);
        }
    }

    pub fn push_debug_log(&self, line: impl Into<String>) {
        if self.runtime.debug_enabled {
            tracing::info!("{}", line.into());
        }
    }

    pub fn notify(&mut self, msg: impl Into<String>, level: NotifLevel) {
        self.notification = Some((msg.into(), level));
        self.notification_ttl = 60;
    }

    pub fn tick_notification(&mut self) {
        if self.notification_ttl > 0 {
            self.notification_ttl -= 1;
            if self.notification_ttl == 0 {
                self.notification = None;
            }
        }
    }

    pub fn has_modal(&self) -> bool {
        matches!(self.vpn_state, VpnState::WaitingToken | VpnState::WaitingCert)
            || self.ui_mode == UiMode::Help
    }

    pub fn show_help(&mut self) {
        if self.ui_mode != UiMode::Help {
            self.previous_ui_mode = Some(self.ui_mode.clone());
            self.ui_mode = UiMode::Help;
            self.focus = Focus::HelpPopup;
        }
    }

    pub fn hide_help(&mut self) {
        if let Some(prev_mode) = self.previous_ui_mode.take() {
            self.ui_mode = prev_mode;
        } else {
            self.ui_mode = UiMode::ProfileList;
        }
        self.focus = Focus::ProfileList;
    }

    pub fn load_profiles(&mut self, profiles: Vec<VpnProfile>) {
        self.profiles = profiles;
    }

    pub fn get_current_profile(&self) -> Option<&VpnProfile> {
        self.profiles.get(self.selected_profile_index)
    }

    pub fn select_profile(&mut self, index: usize) {
        if index < self.profiles.len() {
            self.selected_profile_index = index;
            let profile = self.profiles[index].clone();
            self.connection.apply_profile(&profile);
        }
    }

    pub fn apply_current_profile(&mut self) {
        if let Some(profile) = self.get_current_profile().cloned() {
            self.connection.apply_profile(&profile);
        }
    }

    pub fn update_profile_trusted_cert(&mut self, profile_name: &str, cert_hash: &str) {
        for profile in &mut self.profiles {
            if profile.name == profile_name {
                profile.trusted_cert = Some(cert_hash.to_string());
                break;
            }
        }
        if self
            .get_current_profile()
            .is_some_and(|profile| profile.name == profile_name)
        {
            self.connection.trusted_cert = Some(cert_hash.to_string());
        }
    }

    pub fn open_new_profile_form(&mut self) {
        self.ui_mode = UiMode::NewProfile;
        self.profile_form.reset_for_new();
        self.focus = Focus::ProfileName;
        self.push_log("[APP] Mode: New Profile (F2/N)");
    }

    pub fn open_edit_profile_form(&mut self) {
        if let Some(profile) = self.get_current_profile().cloned() {
            let profile_name = profile.name.clone();
            self.ui_mode = UiMode::EditProfile;
            self.profile_form.load_from_profile(&profile);
            self.focus = Focus::ProfileName;
            self.push_log(format!("[APP] Mode: Edit Profile '{}' (F3/E)", profile_name));
        }
    }

    pub fn replace_profiles(&mut self, profiles: Vec<VpnProfile>) {
        self.profiles = profiles;
        self.selected_profile_index = self
            .selected_profile_index
            .min(self.profiles.len().saturating_sub(1));
        if !self.profiles.is_empty() {
            self.apply_current_profile();
        }
    }

    pub fn cycle_connect_focus_forward(&mut self) {
        if self.vpn_state == VpnState::WaitingCert {
            self.focus = match self.focus {
                Focus::CertAccept => Focus::CertDeny,
                _ => Focus::CertAccept,
            };
            return;
        }

        if self.vpn_state == VpnState::WaitingToken {
            return;
        }

        if self.ui_mode == UiMode::Connect {
            self.focus = match self.focus {
                Focus::Host => Focus::Username,
                Focus::Username => Focus::Password,
                Focus::Password => Focus::SudoPassword,
                Focus::SudoPassword => Focus::Connect,
                Focus::Connect => Focus::Disconnect,
                Focus::Disconnect => Focus::Logs,
                Focus::Logs => Focus::Host,
                _ => Focus::Host,
            };
        }
    }

    pub fn cycle_connect_focus_backward(&mut self) {
        if self.vpn_state == VpnState::WaitingCert {
            self.focus = match self.focus {
                Focus::CertDeny => Focus::CertAccept,
                _ => Focus::CertDeny,
            };
            return;
        }

        if self.vpn_state == VpnState::WaitingToken {
            return;
        }

        if self.ui_mode == UiMode::Connect {
            self.focus = match self.focus {
                Focus::Host => Focus::Logs,
                Focus::Username => Focus::Host,
                Focus::Password => Focus::Username,
                Focus::SudoPassword => Focus::Password,
                Focus::Connect => Focus::SudoPassword,
                Focus::Disconnect => Focus::Connect,
                Focus::Logs => Focus::Disconnect,
                _ => Focus::Host,
            };
        }
    }

    pub fn cycle_profile_form_focus_forward(&mut self) {
        self.focus = match self.focus {
            Focus::ProfileName => Focus::Host,
            Focus::Host => Focus::Port,
            Focus::Port => Focus::Username,
            Focus::Username => Focus::Password,
            Focus::Password => Focus::SudoPassword,
            Focus::SudoPassword => Focus::SavePassword,
            Focus::SavePassword => Focus::UseSudoPassword,
            Focus::UseSudoPassword => Focus::ProfileName,
            _ => Focus::ProfileName,
        };
    }

    pub fn cycle_profile_form_focus_backward(&mut self) {
        self.focus = match self.focus {
            Focus::ProfileName => Focus::UseSudoPassword,
            Focus::Host => Focus::ProfileName,
            Focus::Port => Focus::Host,
            Focus::Username => Focus::Port,
            Focus::Password => Focus::Username,
            Focus::SudoPassword => Focus::Password,
            Focus::SavePassword => Focus::SudoPassword,
            Focus::UseSudoPassword => Focus::SavePassword,
            _ => Focus::ProfileName,
        };
    }

    pub fn scroll_logs_up(&mut self) {
        self.log_scroll = self.log_scroll.saturating_sub(1);
    }

    pub fn scroll_logs_down(&mut self) {
        if !self.logs.is_empty() {
            let max = self.logs.len().saturating_sub(1);
            if self.log_scroll < max {
                self.log_scroll += 1;
            }
        }
    }

    pub fn back_to_profile_list(&mut self) {
        if self.ui_mode == UiMode::Connect && !self.has_modal() {
            match self.vpn_state {
                VpnState::Disconnected | VpnState::Error(_) => {
                    self.ui_mode = UiMode::ProfileList;
                    self.focus = Focus::ProfileList;
                    self.push_log("[APP] Kembali ke daftar profile");
                }
                _ => self.notify(
                    "Putuskan VPN terlebih dahulu sebelum keluar",
                    NotifLevel::Warning,
                ),
            }
        } else if matches!(self.ui_mode, UiMode::NewProfile | UiMode::EditProfile) {
            self.ui_mode = UiMode::ProfileList;
            self.focus = Focus::ProfileList;
            self.delete_confirmation = None;
            self.push_log("[APP] Kembali ke daftar profile");
        }
    }
}

fn parse_host_port(host_port: &str) -> (String, u16) {
    if let Some(colon) = host_port.rfind(':') {
        let host = host_port[..colon].to_string();
        let port = host_port[colon + 1..].parse().unwrap_or(443);
        (host, port)
    } else {
        (host_port.to_string(), 443)
    }
}
