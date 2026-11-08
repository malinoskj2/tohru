//! Ordinary-window presentation intent, independent of settings and lock roles.
#[derive(Default)]
pub(crate) struct Intent {
    sent: Option<bool>,
    dismissed: bool,
}

impl Intent {
    pub(crate) fn update(&mut self, requested: bool) -> Option<bool> {
        if !requested {
            self.dismissed = false;
        }
        let wanted = requested && !self.dismissed;
        if self.sent == Some(wanted) {
            return None;
        }
        self.sent = Some(wanted);
        Some(wanted)
    }

    pub(crate) fn closed(&mut self) {
        self.dismissed = true;
        self.sent = Some(false);
    }
}

#[cfg(test)]
mod tests {
    use super::Intent;

    #[test]
    fn initially_closed_stays_unmapped_until_explicit_open() {
        let mut intent = Intent::default();
        assert_eq!(intent.update(false), Some(false));
        assert_eq!(intent.update(false), None);
        assert_eq!(intent.update(true), Some(true));
        assert_eq!(intent.update(true), None);
    }

    #[test]
    fn hide_and_reopen_each_change_native_presentation_once() {
        let mut intent = Intent::default();
        assert_eq!(intent.update(true), Some(true));
        assert_eq!(intent.update(false), Some(false));
        assert_eq!(intent.update(false), None);
        assert_eq!(intent.update(true), Some(true));
    }

    #[test]
    fn wm_close_does_not_immediately_reopen_a_true_compound_condition() {
        let mut intent = Intent::default();
        assert_eq!(intent.update(true), Some(true));
        intent.closed();
        assert_eq!(intent.update(true), None);
        assert_eq!(intent.update(true), None);
        assert_eq!(intent.update(false), None);
        assert_eq!(intent.update(true), Some(true));
    }
}
