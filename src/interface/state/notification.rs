use std::collections::VecDeque;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug)]
pub struct Notification {
    pub message: String,
    pub level: NotificationLevel,
    pub timestamp: Instant,
    pub duration: Duration,
}

impl Notification {
    pub fn new(message: String, level: NotificationLevel) -> Self {
        let duration = match level {
            NotificationLevel::Error => Duration::from_secs(8),
            _ => Duration::from_secs(4),
        };
        Self { message, level, timestamp: Instant::now(), duration }
    }

    pub fn icon(&self) -> &'static str {
        match self.level {
            NotificationLevel::Info => "ℹ",
            NotificationLevel::Success => "✓",
            NotificationLevel::Warning => "⚠",
            NotificationLevel::Error => "✕",
        }
    }

    pub fn expired(&self) -> bool {
        self.timestamp.elapsed() > self.duration
    }
}

#[derive(Clone, Debug)]
pub struct NotificationState {
    notifications: VecDeque<Notification>,
}

impl NotificationState {
    pub fn new() -> Self {
        Self { notifications: VecDeque::new() }
    }

    pub fn push(&mut self, message: String, level: NotificationLevel) {
        self.notifications.push_back(Notification::new(message, level));
        if self.notifications.len() > 5 {
            self.notifications.pop_front();
        }
    }

    pub fn dismiss_old(&mut self) {
        self.notifications.retain(|n| !n.expired());
    }

    pub fn active(&self) -> impl Iterator<Item = &Notification> {
        self.notifications.iter().filter(|n| !n.expired())
    }
}

impl Default for NotificationState {
    fn default() -> Self {
        Self::new()
    }
}
