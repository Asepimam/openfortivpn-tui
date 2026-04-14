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
}

// ─── Events ───────────────────────────────────────────────────────────────────
#[derive(Debug)]
pub enum AppEvent {
    LogLine(String),
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
    pub fn new() -> Self {
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
        )
    }
    
    pub fn cycle_focus_forward(&mut self) {
        // Saat modal aktif, cycle di dalam modal saja
        if self.vpn_state == VpnState::WaitingCert {
            self.focus = match self.focus {
                Focus::CertAccept => Focus::CertDeny,
                _ => Focus::CertAccept,
            };
            return;
        }
        // WaitingToken — tidak ada cycling, fokus tetap di TokenInput
        if self.vpn_state == VpnState::WaitingToken { return; }

        // Di Connect mode
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
        // Saat modal aktif, cycle di dalam modal saja
        if self.vpn_state == VpnState::WaitingCert {
            self.focus = match self.focus {
                Focus::CertDeny => Focus::CertAccept,
                _ => Focus::CertDeny,
            };
            return;
        }
        if self.vpn_state == VpnState::WaitingToken { return; }

        // Di Connect mode
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
    
    pub fn load_profiles(&mut self, profiles: Vec<crate::config::VpnProfile>) {
        self.profiles = profiles;
    }
    
    pub fn select_profile(&mut self, index: usize) {
        if index < self.profiles.len() {
            self.selected_profile_index = index;
            let profile = &self.profiles[index];
            self.host = profile.host.clone();
            self.port = profile.port;
            self.username = profile.username.clone();
            self.password = profile.password.clone();
            self.sudo_password = profile.sudo_password.clone();
            self.trusted_cert = profile.trusted_cert.clone();
        }
    }
    
    pub fn get_current_profile(&self) -> Option<&crate::config::VpnProfile> {
        self.profiles.get(self.selected_profile_index)
    }
    
    pub fn apply_current_profile(&mut self) {
        if let Some(profile) = self.get_current_profile() {
            let host = profile.host.clone();
            let port = profile.port;
            let username = profile.username.clone();
            let password = profile.password.clone();
            let sudo_password = profile.sudo_password.clone();
            let trusted_cert = profile.trusted_cert.clone();
            drop(profile);
            self.host = host;
            self.port = port;
            self.username = username;
            self.password = password;
            self.sudo_password = sudo_password;
            self.trusted_cert = trusted_cert;
        }
    }
    
    pub fn update_profile_trusted_cert(&mut self, profile_name: &str, cert_hash: String) {
        // Update di profiles vector
        if let Some(profile) = self.profiles.iter_mut().find(|p| p.name == profile_name) {
            profile.trusted_cert = Some(cert_hash.clone());
        }
        // Update di current profile
        if let Some(profile) = self.get_current_profile() {
            if profile.name == profile_name {
                self.trusted_cert = Some(cert_hash);
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
}