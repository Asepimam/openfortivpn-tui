use crate::config::VpnProfile;

#[derive(Debug, Clone, Default)]
pub struct ProfileForm {
    pub name: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub password: String,
    pub sudo_password: String,
    pub save_password: bool,
    pub use_sudo_password: bool,
    pub editing_profile_name: Option<String>,
}

impl ProfileForm {
    pub fn new() -> Self {
        Self {
            port: String::from("443"),
            ..Self::default()
        }
    }

    pub fn reset_for_new(&mut self) {
        *self = Self::new();
    }

    pub fn load_from_profile(&mut self, profile: &VpnProfile) {
        self.name = profile.name.clone();
        self.host = profile.host.clone();
        self.port = profile.port.to_string();
        self.username = profile.username.clone();
        self.password = profile.password.clone();
        self.sudo_password = profile.sudo_password.clone();
        self.save_password = profile.save_password;
        self.use_sudo_password = profile.use_sudo_password;
        self.editing_profile_name = Some(profile.name.clone());
    }

    pub fn to_profile(&self) -> VpnProfile {
        VpnProfile {
            name: self.name.clone(),
            host: self.host.clone(),
            port: self.port.parse().unwrap_or(443),
            username: self.username.clone(),
            save_password: self.save_password,
            password: if self.save_password {
                self.password.clone()
            } else {
                String::new()
            },
            trusted_cert: None,
            use_sudo_password: self.use_sudo_password,
            sudo_password: if self.use_sudo_password {
                self.sudo_password.clone()
            } else {
                String::new()
            },
        }
    }
}
