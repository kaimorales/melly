use tokio_util::sync::CancellationToken;
#[derive(Default)]
pub struct LoginState {
    generation: u64,
    active: Option<CancellationToken>,
}
impl LoginState {
    pub fn begin(&mut self) -> Result<(u64, CancellationToken), String> {
        if self.active.as_ref().is_some_and(|c| !c.is_cancelled()) {
            return Err("ChatGPT sign-in is already open in your browser".into());
        }
        self.generation += 1;
        let cancel = CancellationToken::new();
        self.active = Some(cancel.clone());
        Ok((self.generation, cancel))
    }
    pub fn is_current(&self, id: u64) -> bool {
        self.generation == id
    }
    pub fn cancel(&mut self) {
        self.generation += 1;
        if let Some(c) = self.active.take() {
            c.cancel()
        }
    }
}
/// Keeps generation validation and delivery atomic with retry/disconnect.
/// The callback must be short and synchronous; never perform auth work here.
pub fn publish_if_current(
    state: &std::sync::Mutex<LoginState>,
    generation: u64,
    publish: impl FnOnce(),
) -> bool {
    let state = state.lock().unwrap();
    if !state.is_current(generation) {
        return false;
    }
    publish();
    true
}
pub fn open_authorization(
    url: &str,
    cancel: &CancellationToken,
    open: impl FnOnce(&str) -> Result<(), ()>,
) -> Result<(), &'static str> {
    if cancel.is_cancelled() {
        return Err("Sign-in was cancelled");
    }
    let valid = url::Url::parse(url).is_ok_and(|u| {
        u.scheme() == "https"
            && u.host_str() == Some("auth.openai.com")
            && u.username().is_empty()
            && u.password().is_none()
            && u.path() == "/api/accounts/authorize"
            && u.port_or_known_default() == Some(443)
    });
    if !valid {
        cancel.cancel();
        return Err("The sign-in address was not an official OpenAI authorization URL");
    }
    if open(url).is_err() {
        cancel.cancel();
        return Err("Could not open your browser. Try Continue with ChatGPT again.");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_sign_in_is_rejected_and_cancel_allows_retry() {
        let mut state = LoginState::default();
        let (old, cancel) = state.begin().unwrap();
        assert!(state.begin().is_err());
        state.cancel();
        assert!(cancel.is_cancelled());
        let (new, _) = state.begin().unwrap();
        assert!(!state.is_current(old));
        assert!(state.is_current(new));
    }
    #[test]
    fn failed_browser_handoff_cancels_listener_and_immediately_allows_retry() {
        let mut state = LoginState::default();
        let (_, cancel) = state.begin().unwrap();
        assert!(
            open_authorization(
                "https://auth.openai.com/api/accounts/authorize",
                &cancel,
                |_| Err(())
            )
            .is_err()
        );
        assert!(cancel.is_cancelled());
        assert!(state.begin().is_ok());
    }
    #[test]
    fn invalid_browser_url_never_opens() {
        let cancel = CancellationToken::new();
        assert!(
            open_authorization(
                "https://example.com/api/accounts/authorize",
                &cancel,
                |_| panic!("must not open")
            )
            .is_err()
        );
        assert!(cancel.is_cancelled());
    }

    #[test]
    fn obsolete_attempt_cannot_publish_start_or_terminal_status() {
        let state = std::sync::Mutex::new(LoginState::default());
        let (old, _) = state.lock().unwrap().begin().unwrap();
        state.lock().unwrap().cancel();
        let (current, _) = state.lock().unwrap().begin().unwrap();
        for phase in ["signing-in", "connected", "error"] {
            assert!(!publish_if_current(&state, old, || panic!("stale {phase}")));
        }
        let mut delivered = false;
        assert!(publish_if_current(&state, current, || delivered = true));
        assert!(delivered);
    }

    #[test]
    fn generation_cannot_change_between_check_and_status_delivery() {
        let state = std::sync::Mutex::new(LoginState::default());
        let (generation, cancel) = state.lock().unwrap().begin().unwrap();
        cancel.cancel(); // A retry is now allowed, as after sign-in completes.
        std::thread::scope(|scope| {
            let (started, observe) = std::sync::mpsc::channel();
            let (checked, result) = std::sync::mpsc::channel();
            let competing = &state;
            scope.spawn(move || {
                observe.recv().unwrap();
                let blocked = competing.try_lock().is_err();
                checked.send(blocked).unwrap();
            });
            assert!(publish_if_current(&state, generation, || {
                started.send(()).unwrap();
                assert!(
                    result.recv().unwrap(),
                    "a retry must wait until status delivery"
                );
            }));
        });
        assert!(state.lock().unwrap().begin().is_ok());
    }
}
