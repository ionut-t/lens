use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationKind {
    Error,
    Info,
}

#[derive(Debug)]
pub struct Notification {
    pub message: String,
    pub kind: NotificationKind,
    pub expires_at: std::time::Instant,
}

pub struct Notifier {
    notifications: VecDeque<Notification>,
}

impl Notifier {
    pub fn new() -> Self {
        Self {
            notifications: VecDeque::new(),
        }
    }

    pub fn info(&mut self, message: impl Into<String>, duration_secs: u64) {
        self.add(message.into(), NotificationKind::Info, duration_secs);
    }

    pub fn error(&mut self, message: impl Into<String>) {
        self.add(message.into(), NotificationKind::Error, 3);
    }

    pub fn recent(&self) -> Option<&Notification> {
        self.notifications.back()
    }

    pub fn prune_expired(&mut self) {
        let now = std::time::Instant::now();
        self.notifications.retain(|n| n.expires_at > now);
    }

    fn add(&mut self, message: String, kind: NotificationKind, duration_secs: u64) {
        let expires_at = std::time::Instant::now() + std::time::Duration::from_secs(duration_secs);
        self.notifications.push_back(Notification {
            message,
            kind,
            expires_at,
        });
    }
}
