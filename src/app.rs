use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

// ─── VPN Connection State ─────────────────────────────────────────────────────
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
            VpnState::Disconnected   => "DISCONNECTED",
            VpnState::Connecting     => "CONNECTING...",
            VpnState::WaitingCert    => "CERT UNTRUSTED",
            VpnState::WaitingToken   => "WAITING TOKEN",
            VpnState::Connected      => "CONNECTED",
            VpnState::Disconnecting  => "DISCONNECTING...",
            VpnState::Error(_)       => "ERROR",
        }
    }
}

// ─── Certificate Info ─────────────────────────────────────────────────────────
#[derive(Debug, Clone, Default)]
pub struct CertInfo {
    pub hash: String,
    pub subject_cn: String,
    pub subject_org: String,
    pub issuer_cn: String,
    pub raw_lines: Vec<String>,
}

// ─── UI Mode ─────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, PartialEq)]
pub enum UiMode {
    ProfileList,
    NewProfile,
    EditProfile,
    Connect,
    Help,
}

// ─── Focus Tracking ───────────────────────────────────────────────────────────
#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    // Profile list mode
    ProfileList,
    ProfileItem(usize),
    
    // Form mode
    ProfileName,
    Host,
    Port,
    Username,
    Password,
    SudoPassword,
    SavePassword,
    UseSudoPassword,
    
    // Action buttons
    Connect,
    Disconnect,
    Logs,
    
    // Modal dialogs
    CertAccept,
    CertDeny,
    TokenInput,
    HelpPopup,
}

// ─── Events ───────────────────────────────────────────────────────────────────
#[derive(Debug)]
pub enum AppEvent {
    LogLine(String),
    DebugLog(String),
    StateChanged(VpnState),
    NeedToken,
    CertError(CertInfo),
    Quit,
}

// ─── App State ────────────────────────────────────────────────────────────────
pub struct App {
    // Current connection fields
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub sudo_password: String,
    pub token_input: String,

    // UI state
    pub ui_mode: UiMode,
    pub previous_ui_mode: Option<UiMode>,
    pub focus: Focus,
    pub show_password: bool,
    pub vpn_state: VpnState,
    pub logs: Vec<String>,
    pub log_scroll: usize,
    pub notification: Option<(String, NotifLevel)>,
    pub notification_ttl: u8,

    // Profile management
    pub profiles: Vec<crate::config::VpnProfile>,
    pub selected_profile_index: usize,
    pub profile_scroll: usize,
    pub delete_confirmation: Option<String>,
    
    // Form for new/edit profile
    pub profile_name: String,
    pub profile_host: String,
    pub profile_port: String,
    pub profile_username: String,
    pub profile_password: String,
    pub profile_sudo_password: String,
    pub profile_save_password: bool,
    pub profile_use_sudo_password: bool,
    pub editing_profile_name: Option<String>,

    // Cert popup state
    pub pending_cert: Option<CertInfo>,
    pub trusted_cert: Option<String>,

    // Channel
    pub event_tx: mpsc::UnboundedSender<AppEvent>,
    pub event_rx: mpsc::UnboundedReceiver<AppEvent>,

    // VPN PID
    pub vpn_pid: Arc<Mutex<Option<u32>>>,
    pub waiting_for_input_flag: Arc<Mutex<bool>>,
    pub debug_enabled: bool,

    pub should_quit: bool,
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
        let waiting_for_input_flag = Arc::new(Mutex::new(false));

        Self {
            host: String::new(),
            port: 443,
            username: String::new(),
            password: String::new(),
            sudo_password: String::new(),
            token_input: String::new(),
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
            profile_scroll: 0,
            delete_confirmation: None,
            profile_name: String::new(),
            profile_host: String::new(),
            profile_port: String::from("443"),
            profile_username: String::new(),
            profile_password: String::new(),
            profile_sudo_password: String::new(),
            profile_save_password: false,
            profile_use_sudo_password: false,
            editing_profile_name: None,
            pending_cert: None,
            trusted_cert: None,
            event_tx,
            event_rx,
            vpn_pid: Arc::new(Mutex::new(None)),
            waiting_for_input_flag,
            debug_enabled,
            should_quit: false,
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
        if self.debug_enabled {
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
        matches!(
            self.vpn_state,
            VpnState::WaitingToken | VpnState::WaitingCert
        ) || self.ui_mode == UiMode::Help
    }
    
    pub fn cycle_focus_forward(&mut self) {
        if self.vpn_state == VpnState::WaitingCert {
            self.focus = match self.focus {
                Focus::CertAccept => Focus::CertDeny,
                _ => Focus::CertAccept,
            };
            return;
        }
        if self.vpn_state == VpnState::WaitingToken { return; }

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
            return;
        }
    }

    pub fn cycle_focus_backward(&mut self) {
        if self.vpn_state == VpnState::WaitingCert {
            self.focus = match self.focus {
                Focus::CertDeny => Focus::CertAccept,
                _ => Focus::CertDeny,
            };
            return;
        }
        if self.vpn_state == VpnState::WaitingToken { return; }

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
            return;
        }
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
            self.focus = Focus::ProfileList;
        } else {
            self.ui_mode = UiMode::ProfileList;
            self.focus = Focus::ProfileList;
        }
    }
    
    pub fn load_profiles(&mut self, profiles: Vec<crate::config::VpnProfile>) {
        self.profiles = profiles;
    }
    
    pub fn select_profile(&mut self, index: usize) {
        if index < self.profiles.len() {
            self.selected_profile_index = index;
            let profile = self.profiles[index].clone();
            self.host = profile.host;
            self.port = profile.port;
            self.username = profile.username;
            self.password = profile.password;
            self.sudo_password = profile.sudo_password;
            self.trusted_cert = profile.trusted_cert;
        }
    }
    
    pub fn get_current_profile(&self) -> Option<&crate::config::VpnProfile> {
        self.profiles.get(self.selected_profile_index)
    }
    
    pub fn apply_current_profile(&mut self) {
        let idx = self.selected_profile_index;
        if idx < self.profiles.len() {
            let profile = self.profiles[idx].clone();
            self.host = profile.host;
            self.port = profile.port;
            self.username = profile.username;
            self.password = profile.password;
            self.sudo_password = profile.sudo_password;
            self.trusted_cert = profile.trusted_cert;
        }
    }
    
    pub fn update_profile_trusted_cert(&mut self, profile_name: &str, cert_hash: &str) {
        for profile in self.profiles.iter_mut() {
            if profile.name == profile_name {
                profile.trusted_cert = Some(cert_hash.to_string());
                break;
            }
        }
        if let Some(profile) = self.get_current_profile() {
            if profile.name == profile_name {
                self.trusted_cert = Some(cert_hash.to_string());
            }
        }
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
                _ => {
                    self.notify("Putuskan VPN terlebih dahulu sebelum keluar", NotifLevel::Warning);
                }
            }
        } else if self.ui_mode == UiMode::NewProfile || self.ui_mode == UiMode::EditProfile {
            self.ui_mode = UiMode::ProfileList;
            self.focus = Focus::ProfileList;
            self.delete_confirmation = None;
            self.push_log("[APP] Kembali ke daftar profile");
        }
    }
}
