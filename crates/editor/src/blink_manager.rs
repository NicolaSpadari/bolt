use gpui::Context;
use settings::SettingsStore;
use std::time::{Duration, Instant};

// BOLT: cursor fade in/out while blinking, like VS Code's smooth caret blinking.
const BLINK_FADE: Duration = Duration::from_millis(200);
use ui::App;

pub struct BlinkManager {
    blink_interval: Duration,
    blink_epoch: usize,
    /// Whether the blinking is paused.
    blinking_paused: bool,
    /// Whether the cursor should be visibly rendered or not.
    visible: bool,
    // BOLT: when the blink last toggled `visible`, to fade between the two states.
    visibility_changed_at: Option<Instant>,
    /// Whether the blinking is currently enabled.
    enabled: bool,
    /// Whether the blinking is enabled in the settings.
    blink_enabled_in_settings: fn(&App) -> bool,
}

impl BlinkManager {
    pub fn new(
        blink_interval: Duration,
        blink_enabled_in_settings: fn(&App) -> bool,
        cx: &mut Context<Self>,
    ) -> Self {
        // Make sure we blink the cursors if the setting is re-enabled
        cx.observe_global::<SettingsStore>(move |this, cx| {
            this.blink_cursors(this.blink_epoch, cx)
        })
        .detach();

        Self {
            blink_interval,
            blink_epoch: 0,
            blinking_paused: false,
            visible: true,
            visibility_changed_at: None,
            enabled: false,
            blink_enabled_in_settings,
        }
    }

    fn next_blink_epoch(&mut self) -> usize {
        self.blink_epoch += 1;
        self.blink_epoch
    }

    pub fn pause_blinking(&mut self, cx: &mut Context<Self>) {
        self.show_cursor(cx);

        let epoch = self.next_blink_epoch();
        let interval = Duration::from_millis(500);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(interval).await;
            this.update(cx, |this, cx| this.resume_cursor_blinking(epoch, cx))
        })
        .detach();
    }

    fn resume_cursor_blinking(&mut self, epoch: usize, cx: &mut Context<Self>) {
        if epoch == self.blink_epoch {
            self.blinking_paused = false;
            self.blink_cursors(epoch, cx);
        }
    }

    fn blink_cursors(&mut self, epoch: usize, cx: &mut Context<Self>) {
        if (self.blink_enabled_in_settings)(cx) {
            if epoch == self.blink_epoch && self.enabled && !self.blinking_paused {
                self.visible = !self.visible;
                self.visibility_changed_at = Some(Instant::now());
                cx.notify();

                let epoch = self.next_blink_epoch();
                let interval = self.blink_interval;
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(interval).await;
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| this.blink_cursors(epoch, cx));
                    }
                })
                .detach();
            }
        } else {
            self.show_cursor(cx);
        }
    }

    pub fn show_cursor(&mut self, cx: &mut Context<BlinkManager>) {
        if !self.visible {
            self.visible = true;
            cx.notify();
        }
        self.visibility_changed_at = None;
    }

    /// Enable the blinking of the cursor.
    pub fn enable(&mut self, cx: &mut Context<Self>) {
        if self.enabled {
            return;
        }

        self.enabled = true;
        // Set cursors as invisible and start blinking: this causes cursors
        // to be visible during the next render.
        self.visible = false;
        self.blink_cursors(self.blink_epoch, cx);
    }

    /// Disable the blinking of the cursor.
    pub fn disable(&mut self, _cx: &mut Context<Self>) {
        self.visible = false;
        self.visibility_changed_at = None;
        self.enabled = false;
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    /// BOLT: cursor opacity, easing between hidden and visible for `BLINK_FADE` after each blink.
    pub fn opacity(&self) -> f32 {
        let target = if self.visible { 1. } else { 0. };
        let Some(changed_at) = self.visibility_changed_at else {
            return target;
        };
        let progress = (changed_at.elapsed().as_secs_f32() / BLINK_FADE.as_secs_f32()).min(1.);
        let eased = progress * progress * (3. - 2. * progress);
        if self.visible { eased } else { 1. - eased }
    }

    /// BOLT: whether the cursor is between the two blink states.
    pub fn fading(&self) -> bool {
        self.visibility_changed_at
            .is_some_and(|changed_at| changed_at.elapsed() < BLINK_FADE)
    }

    #[cfg(test)]
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }
}
