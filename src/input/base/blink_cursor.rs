use gpui::{Context, Pixels, Task, px};
use instant::Duration;

static INTERVAL: Duration = Duration::from_millis(500);
static PAUSE_DELAY: Duration = Duration::from_millis(300);
/// Blinks after the last input before the cursor settles, visible and with no
/// timer running: five seconds at [`INTERVAL`].
const IDLE_BLINKS: usize = 10;

// On Windows, Linux, we should use integer to avoid blurry cursor.
#[cfg(not(target_os = "macos"))]
pub(super) const CURSOR_WIDTH: Pixels = px(2.);
#[cfg(target_os = "macos")]
pub(super) const CURSOR_WIDTH: Pixels = px(1.5);

/// To manage the Input cursor blinking.
///
/// It will start blinking with a interval of 500ms.
/// Every loop will notify the view to update the `visible`, and Input will observe this update to touch repaint.
///
/// The input painter will check if this in visible state, then it will draw the cursor.
///
/// After [`IDLE_BLINKS`] blinks without input the cursor stays visible and the
/// timer stops, so an idle focused input does not keep waking the app. The next
/// [`Self::pause`] (a key, a click, an edit) or [`Self::start`] resumes it.
pub(crate) struct BlinkCursor {
    visible: bool,
    paused: bool,
    epoch: usize,
    /// Blinks left before the cursor settles; `start` and `pause` reset it.
    blinks_left: usize,

    _task: Task<()>,
}

impl BlinkCursor {
    pub(crate) fn new() -> Self {
        Self {
            visible: false,
            paused: false,
            epoch: 0,
            blinks_left: IDLE_BLINKS,
            _task: Task::ready(()),
        }
    }

    /// Start the blinking
    pub(crate) fn start(&mut self, cx: &mut Context<Self>) {
        self.blinks_left = IDLE_BLINKS;
        self.blink(self.epoch, cx);
    }

    /// Stop the blinking and drop the pending timer, so nothing wakes until the
    /// next [`Self::start`], which begins from a visible cursor.
    pub(crate) fn stop(&mut self, cx: &mut Context<Self>) {
        self.epoch = 0;
        self.paused = false;
        self.visible = false;
        self._task = Task::ready(());
        cx.notify();
    }

    fn next_epoch(&mut self) -> usize {
        self.epoch += 1;
        self.epoch
    }

    fn blink(&mut self, epoch: usize, cx: &mut Context<Self>) {
        if self.paused || epoch != self.epoch {
            self.visible = true;
            return;
        }

        if self.blinks_left == 0 {
            // Idle: settle visible and schedule nothing.
            self.visible = true;
            cx.notify();
            return;
        }
        self.blinks_left -= 1;

        self.visible = !self.visible;
        cx.notify();

        // Schedule the next blink
        let epoch = self.next_epoch();
        self._task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(INTERVAL).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| this.blink(epoch, cx));
            }
        });
    }

    pub(crate) fn visible(&self) -> bool {
        // Keep showing the cursor if paused
        self.paused || self.visible
    }

    /// Pause the blinking, and delay 500ms to resume the blinking.
    ///
    /// A no-op on a cursor that is not blinking (a zero epoch: never started,
    /// or stopped by a blur or a window deactivation), so a programmatic edit
    /// of an unfocused input does not start a blink loop nothing would stop.
    pub(crate) fn pause(&mut self, cx: &mut Context<Self>) {
        if self.epoch == 0 {
            return;
        }
        self.blinks_left = IDLE_BLINKS;
        self.paused = true;
        self.visible = true;
        cx.notify();

        // delay 500ms to start the blinking
        let epoch = self.next_epoch();
        self._task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PAUSE_DELAY).await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.paused = false;
                    this.blink(epoch, cx);
                });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext as _, Entity, Subscription, TestAppContext};
    use std::{cell::Cell, rc::Rc};

    fn visible(cursor: &Entity<BlinkCursor>, cx: &mut TestAppContext) -> bool {
        cursor.read_with(cx, |cursor, _| cursor.visible())
    }

    fn advance(cx: &mut TestAppContext, by: Duration) {
        cx.executor().advance_clock(by);
        cx.run_until_parked();
    }

    fn started(cx: &mut TestAppContext) -> Entity<BlinkCursor> {
        let cursor = cx.new(|_| BlinkCursor::new());
        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();
        cursor
    }

    /// Counts the cursor's notifications; each one repaints the input.
    fn count_notifies(
        cursor: &Entity<BlinkCursor>,
        cx: &mut TestAppContext,
    ) -> (Rc<Cell<usize>>, Subscription) {
        let count = Rc::new(Cell::new(0usize));
        let counter = count.clone();
        let subscription =
            cx.update(|cx| cx.observe(cursor, move |_, _| counter.set(counter.get() + 1)));
        cx.run_until_parked();
        (count, subscription)
    }

    #[gpui::test]
    fn an_idle_cursor_settles_visible_and_stops_its_timer(cx: &mut TestAppContext) {
        let cursor = started(cx);
        assert!(visible(&cursor, cx));
        advance(cx, INTERVAL);
        assert!(!visible(&cursor, cx), "the cursor never blinked");

        advance(cx, INTERVAL * IDLE_BLINKS as u32);
        assert!(visible(&cursor, cx));
        let (notifies, _subscription) = count_notifies(&cursor, cx);
        advance(cx, Duration::from_secs(60));
        assert!(visible(&cursor, cx));
        assert_eq!(notifies.get(), 0, "an idle cursor kept blinking");
    }

    #[gpui::test]
    fn input_resumes_blinking_on_a_settled_cursor(cx: &mut TestAppContext) {
        let cursor = started(cx);
        advance(cx, INTERVAL * (IDLE_BLINKS as u32 + 1));

        cursor.update(cx, |cursor, cx| cursor.pause(cx));
        cx.run_until_parked();
        assert!(visible(&cursor, cx));
        advance(cx, PAUSE_DELAY);
        assert!(!visible(&cursor, cx), "input did not resume the blinking");

        advance(cx, INTERVAL * (IDLE_BLINKS as u32 + 1));
        assert!(visible(&cursor, cx));
        let (notifies, _subscription) = count_notifies(&cursor, cx);
        advance(cx, Duration::from_secs(60));
        assert_eq!(notifies.get(), 0, "the resumed cursor never settled again");
    }

    #[gpui::test]
    fn input_within_the_idle_delay_keeps_the_cursor_blinking(cx: &mut TestAppContext) {
        let cursor = started(cx);
        for _ in 0..5 {
            advance(cx, Duration::from_secs(4));
            cursor.update(cx, |cursor, cx| cursor.pause(cx));
            cx.run_until_parked();
        }
        let (notifies, _subscription) = count_notifies(&cursor, cx);
        advance(cx, PAUSE_DELAY + INTERVAL * 2);
        assert!(
            notifies.get() >= 2,
            "the cursor settled while input kept coming"
        );
    }

    #[gpui::test]
    fn stopping_a_cursor_ends_the_blink_loop(cx: &mut TestAppContext) {
        let cursor = started(cx);
        cursor.update(cx, |cursor, cx| cursor.pause(cx));
        cx.run_until_parked();
        cursor.update(cx, |cursor, cx| cursor.stop(cx));
        cx.run_until_parked();

        let (notifies, _subscription) = count_notifies(&cursor, cx);
        advance(cx, Duration::from_secs(3));
        assert!(!visible(&cursor, cx));
        assert_eq!(notifies.get(), 0, "a stopped cursor kept blinking");

        cursor.update(cx, |cursor, cx| cursor.start(cx));
        cx.run_until_parked();
        assert!(visible(&cursor, cx), "a restarted cursor began hidden");
    }

    #[gpui::test]
    fn pausing_a_cursor_that_is_not_blinking_does_not_start_it(cx: &mut TestAppContext) {
        let cursor = cx.new(|_| BlinkCursor::new());
        let (notifies, _subscription) = count_notifies(&cursor, cx);
        // What a programmatic `set_value` on an unfocused input does.
        cursor.update(cx, |cursor, cx| cursor.pause(cx));
        cx.run_until_parked();
        advance(cx, Duration::from_secs(3));
        assert!(!visible(&cursor, cx));
        assert_eq!(notifies.get(), 0);
    }
}
