#![allow(deprecated)]
use std::time::{SystemTime, UNIX_EPOCH};
use ws::Sender;
use ws::util::{Token, Timeout};

pub const RESPONSE_TIMEOUT: TimeoutWindow = TimeoutWindow { min: 3000, max: 5500 };
pub const PING_TIMEOUT: TimeoutWindow = TimeoutWindow { min: 12000, max: 16000 };

#[derive(Copy, Clone)]
pub struct TimeoutWindow {
    min: u64,
    max: u64
}

pub struct AbsoluteTimeoutWindow {
    min: u64,
    max: u64
}


impl AbsoluteTimeoutWindow {
    fn new(timeout_window: &TimeoutWindow) -> AbsoluteTimeoutWindow {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() * 1000;
        AbsoluteTimeoutWindow {
            min: now + timeout_window.min,
            max: now + timeout_window.max
        }
    }
}

#[derive(Copy, Clone)]
pub enum TimeoutState {
    Deathline,
    Normal
}

pub struct TimeoutManager {
    window: AbsoluteTimeoutWindow,
    state: TimeoutState,
    timeout: Option<Timeout>,
    token: Token
}

impl TimeoutManager {
    pub fn new(sender: &Sender, window: TimeoutWindow, state: TimeoutState) -> TimeoutManager {
        let absolute_window = AbsoluteTimeoutWindow::new(&window);
        let token = Token(2);

        sender.timeout(window.max, token).unwrap();
        TimeoutManager {
            window: absolute_window,
            state,
            timeout: None,
            token
        }
    }


    pub fn arm(&mut self, sender: &Sender, new_window: TimeoutWindow, new_state: TimeoutState) {
        self.state = new_state;

        let new_absolute_window = AbsoluteTimeoutWindow::new(&new_window);
        if self.window.max < new_absolute_window.min || self.window.max > new_absolute_window.max {
            self.window = new_absolute_window;
            self.timeout.take().map(|timeout| sender.cancel(timeout));

            self.token = Token(self.token.0 + 1);
            sender.timeout(new_window.max, self.token).unwrap();
        }
    }

    pub fn disarm(&mut self) {
        self.timeout = None;
    }

    pub fn on_new_timeout(&mut self, token: Token, timeout: Timeout) {
        if token == self.token {
            self.timeout = Some(timeout);
        }
    }

    pub fn on_timeout(&mut self, token: Token) -> Option<TimeoutState> {
        if token == self.token {
            self.timeout = None;
            Some(self.state)
        } else {
            None
        }
    }
}