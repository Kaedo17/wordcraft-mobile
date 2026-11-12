#[cfg(not(target_arch = "wasm32"))]
use super::i18n;
use super::{WordApp, json};

impl WordApp {
    /// Read a Control-click as the right-click on macOS, then offer at most one synthetic gesture
    /// step to the host. eframe carries an unconsumed RawInput into the next pass and into hidden
    /// logic ticks, so offering it again would duplicate input. The step stays queued until a
    /// final (not discarded) UI pass acknowledges it, and a press/release pair stays intact.
    pub fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        // Android: winit's backend never emits `ModifiersChanged`, so shortcuts and the
        // wheel/click modifiers egui-winit fills would all see an empty state.
        #[cfg(target_os = "android")]
        stamp_android_modifiers(&mut raw.events, &mut self.android_modifiers);
        // Capture the surface from the raw input itself. `InputState::viewport_rect` and the
        // last painted screen rect are stale in a hidden `run_logic` tick; RawInput is the host's
        // current answer, including minimized/occluded and a Wayland `inner_rect: None`.
        if raw.viewport_id == egui::ViewportId::ROOT
            && let Some(geometry) = crate::control::SurfaceGeometry::from_raw(raw, ctx)
        {
            // A hidden resize and change back still invalidate an older capture, even if
            // neither intermediate geometry reaches a drawing or logic pass.
            if self.surface_geometry != Some(geometry) {
                self.capture_epoch = self.capture_epoch.wrapping_add(1);
            }
            self.surface_geometry = Some(geometry);
        }
        if cfg!(target_os = "macos") {
            control_click_as_secondary(&mut raw.events, &mut self.control_click);
        }
        if self.synthetic_pending || self.synthetic.is_empty() || self.fonts_frames < 2 || !self.control_input_is_roots(ctx) {
            return;
        }
        let Some((events, n)) = self.peek_synthetic() else { return };
        if events.iter().any(|e| matches!(e, egui::Event::Key { .. } | egui::Event::Text(_))) {
            // Control input belongs to this window's remembered widget even while another app
            // is active. Give only this input frame egui focus, without requesting OS activation.
            raw.focused = true;
            if let (Some(ctx), Some((id, _))) = (&self.ctx, self.canvas.widget_id) {
                // Canvas keys run before painting updates last frame's native focus flag.
                self.canvas.focused = ctx.memory(|m| m.has_focus(id));
            }
        }
        raw.events.extend(events);
        self.synthetic_pending = true;
        self.synthetic_injected = n;
    }

    /// Whether this pass's input came from the control channel (`ui.click`, `ui.key`, `ui.drag`,
    /// `ui.text`…, which MCP's `click`, `key` and `type` tools send). Decisions that are only the
    /// user's ignore it: letting agent messages through again, and answering an agent's question
    /// (#959 review). A real click in the same frame as a control step is ignored too, which errs
    /// the safe way.
    pub(crate) fn input_from_control(&self, ctx: &egui::Context) -> bool {
        let viewport = ctx.viewport_id();
        self.synthetic_pending || ctx.with_plugin(|p: &mut crate::windows::ViewportInput| p.injected == Some(viewport)).unwrap_or(false)
    }

    /// The control channel's next input frame, taken off the queue.
    pub(crate) fn next_synthetic(&mut self) -> Option<Vec<egui::Event>> {
        let (events, n) = self.peek_synthetic()?;
        self.synthetic.drain(..n);
        Some(events)
    }

    /// The control channel's next input frame: one pointer event (after a move to it), or keys
    /// and text up to the next key release; with how many queued events it takes.
    fn peek_synthetic(&self) -> Option<(Vec<egui::Event>, usize)> {
        if self.synthetic.is_empty() {
            return None;
        }
        let n = match self.synthetic.first() {
            Some(egui::Event::PointerMoved(_) | egui::Event::PointerButton { .. }) => 1,
            _ => self.synthetic.iter().position(|e| matches!(e, egui::Event::Key { pressed: false, .. })).map_or(self.synthetic.len(), |i| i + 1),
        };
        let mut events = Vec::new();
        if let Some(egui::Event::PointerMoved(p) | egui::Event::PointerButton { pos: p, .. }) = self.synthetic.first() {
            events.push(egui::Event::PointerMoved(*p));
        }
        let n = n.min(self.synthetic.len());
        events.extend(self.synthetic.iter().take(n).cloned());
        Some((events, n))
    }

    /// Raw input read in the known UI phase, first pass only: dropped files and the SmartArt Text
    /// Pane's Tab/Shift+Tab, before the font warm-up return. `logic` also runs hidden, where its
    /// input events are from the previous shown frame.
    pub(super) fn handle_ui_input_events(&mut self, ctx: &egui::Context) {
        // eframe does not preserve dropped files for a later frame, so a drop in the first two
        // passes would otherwise disappear.
        #[cfg(not(target_arch = "wasm32"))]
        if ctx.current_pass_index() == 0 {
            let dropped = ctx.input_mut(|i| std::mem::take(&mut i.raw.dropped_files));
            // A drop behind a modal dialog does nothing: it would insert a picture or open a
            // document under it.
            if !dropped.is_empty() && !self.modal_open() {
                self.open_dropped_files(dropped);
                let lang = i18n::Lang::from_pref(&self.ui.language);
                self.install_ui_fonts(ctx, lang);
            }
        }
        // SmartArt Text Pane (#2701): Tab demotes and Shift+Tab promote the focused item, as
        // Word's pane does. This is an input action, so it runs in the first UI pass only; hidden
        // `run_logic` ticks still see the previous shown frame's keys.
        if ctx.current_pass_index() == 0
            && self.session.view.smart_art_pane
            && self.smart_art_row_focused
            && self.smart_art_item.is_some()
            && !self.modal_open()
        {
            let shift_tab = ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::Tab));
            let tab = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
            if shift_tab || tab {
                ctx.memory_mut(|m| m.move_focus(egui::FocusDirection::None));
                let _ = self.run(if shift_tab { "smartArt.promote" } else { "smartArt.demote" }, json!({"index": self.smart_art_item.unwrap_or(0)}));
            }
        }
    }

    /// A click on the window's chrome (ribbon tabs and empty ribbon space, galleries, check
    /// boxes, the status bar, panes) hands keyboard focus back to the document, as in Word, so the
    /// caret keeps blinking and typing continues where it was (#1093). egui drops the page's focus
    /// on any click elsewhere; only a click that left nothing focused counts, so a text field
    /// (font size, zoom, command search) keeps the focus it took, and clicks on a dialog, menu or
    /// popup, or while one is open, leave focus alone. Clicks on the page are the canvas's own.
    pub(super) fn refocus_after_chrome_click(&mut self, ctx: &egui::Context) {
        if self.ui.backstage || self.dialog.is_some() || self.colors.is_some() {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.file_browser.open.is_some() {
            return;
        }
        let Some(pos) = ctx.input(|i| if i.pointer.any_click() { i.pointer.interact_pos() } else { None }) else { return };
        // The ribbon dropped over the page in Focus (#1678) is chrome, though it lies over the page.
        let on_focus_ribbon = self.focus_ribbon && ctx.layer_id_at(pos).is_some_and(|l| l.id == egui::Id::unique("focus_ribbon"));
        // The page handles its own clicks (comment balloons there take the keyboard themselves).
        if !on_focus_ribbon && (self.canvas.balloon.is_some() || self.canvas.viewport.is_some_and(|v| v.contains(pos))) {
            return;
        }
        let on_chrome = on_focus_ribbon || ctx.layer_id_at(pos).is_none_or(|l| l.order == egui::Order::Background);
        if on_chrome && ctx.memory(|m| m.focused().is_none()) && !egui::Popup::is_any_open(ctx) {
            self.canvas.want_focus = true;
            // The page has already been drawn, so it only sees `want_focus` next frame, and its own
            // `focused` flag (which gates the keys) isn't recomputed until then either. Claim both
            // now, or the click swallows the first keystroke.
            self.canvas.focused = true;
            // Only a page this pass drew. The File page's back arrow closes the File page in the
            // pass that drew it instead of the page, and focus on a widget missing from the pass
            // names a node AccessKit doesn't have: with accessibility on, AccessKit ends the program
            // over that. `want_focus` hands the keyboard over next frame instead.
            if let Some((id, pass)) = self.canvas.widget_id
                && pass == ctx.cumulative_pass_nr()
            {
                ctx.memory_mut(|m| m.request_focus(id));
            }
            // The canvas reads keys before it takes focus back (#1107), so it counts as focused
            // now: a keystroke in the very next frame reaches the page instead of being dropped.
            self.canvas.focused = true;
            ctx.request_repaint();
        }
    }
}

/// macOS reads a Control-click as the right-click (winit reports the primary button with Control
/// held), so menus open on it as on a two-finger click. The release follows its press even if
/// Control was let go in between.
pub(crate) fn control_click_as_secondary(events: &mut [egui::Event], held: &mut bool) {
    for e in events {
        if let egui::Event::PointerButton { button: button @ egui::PointerButton::Primary, pressed, modifiers, .. } = e {
            if *pressed {
                *held = modifiers.ctrl && !modifiers.mac_cmd;
            }
            if *held {
                *button = egui::PointerButton::Secondary;
            }
            if !*pressed {
                *held = false;
            }
        }
    }
}

/// Called only on Android, from `raw_input_hook`; everywhere else winit reports the real
/// modifiers, which this would only duplicate.
///
/// Android: winit's backend never emits `ModifiersChanged` (it ignores the meta state Android
/// carries on every `KeyEvent`), so egui-winit's own modifiers state — what shortcuts and the
/// wheel and click events read — is always empty. Track the modifier key events as they arrive,
/// fill every other event's `modifiers` with what was held when it fired, and end the frame
/// with a `ModifiersChanged` carrying the end state. Mirrors FilmCraft's `stamp_android_modifiers`.
#[cfg(any(target_os = "android", test))]
fn stamp_android_modifiers(events: &mut Vec<egui::Event>, held: &mut egui::Modifiers) {
    for event in events.iter_mut() {
        match event {
            // A lost focus clears the tracked state, so a modifier held while the app is
            // backgrounded cannot stick.
            egui::Event::WindowFocused(false) => *held = egui::Modifiers::NONE,
            egui::Event::Key { key: egui::Key::ControlLeft | egui::Key::ControlRight, pressed, .. } => {
                held.ctrl = *pressed;
                // As egui-winit itself does on every platform but macOS: the command
                // shortcut is Ctrl, and COMMAND chords need `command` set.
                held.command = *pressed;
            }
            egui::Event::Key { key: egui::Key::ShiftLeft | egui::Key::ShiftRight, pressed, .. } => held.shift = *pressed,
            egui::Event::Key { key: egui::Key::AltLeft | egui::Key::AltRight, pressed, .. } => held.alt = *pressed,
            // The wheel and the buttons carry their own modifiers too (Ctrl + wheel zooms the
            // page, Ctrl + click opens a cross-reference), and egui-winit fills them from its
            // own state — always empty here. Stamp them with what was held when they fired.
            egui::Event::Key { modifiers, .. } => *modifiers = *held,
            egui::Event::MouseWheel { modifiers, .. } | egui::Event::PointerButton { modifiers, .. } => {
                *modifiers = *held;
            }
            _ => {}
        }
    }
    // `held.command` mirrors ctrl here (COMMAND chords match). Appended last: egui applies
    // `ModifiersChanged` events in order, so the frame-level modifiers end up as this frame's
    // end state — not a stale value left in eframe's pending input buffer from an earlier frame.
    events.push(egui::Event::ModifiersChanged(*held));
}

#[cfg(test)]
mod android_modifier_tests {
    use egui::{Event, Key, Modifiers};

    fn key(key: Key, pressed: bool) -> Event {
        Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: Modifiers::NONE }
    }

    fn stamp(events: Vec<Event>) -> (Vec<Event>, Modifiers) {
        let mut events = events;
        let mut held = Modifiers::NONE;
        super::stamp_android_modifiers(&mut events, &mut held);
        (events, held)
    }

    /// The modifiers each key event of the frame carries, in order.
    fn stamped(events: &[Event]) -> Vec<Modifiers> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Key { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn ctrl_chord_gets_the_command_modifiers() {
        let (events, held) = stamp(vec![key(Key::ControlLeft, true), key(Key::S, true)]);
        // The Ctrl press itself is left alone; the S press under it carries Ctrl…
        assert_eq!(stamped(&events), vec![Modifiers::NONE, Modifiers { ctrl: true, command: true, ..Modifiers::NONE }]);
        assert!(held.ctrl && held.command);
    }

    #[test]
    fn a_released_ctrl_ends_the_chord_within_the_frame() {
        let ctrl = Modifiers { ctrl: true, command: true, ..Modifiers::NONE };
        let (events, held) =
            stamp(vec![key(Key::Z, true), key(Key::ControlLeft, true), key(Key::Z, true), key(Key::ControlRight, false), key(Key::Z, true)]);
        // Five key events, none of the modifier presses/releases stamped: the Z under Ctrl
        // carries Ctrl, and the Z after the release carries nothing — the frame ends clean,
        // which is what a shortcut bound to Ctrl+Z must see.
        assert_eq!(stamped(&events), vec![Modifiers::NONE, Modifiers::NONE, ctrl, Modifiers::NONE, Modifiers::NONE]);
        assert_eq!(held, Modifiers::NONE);
    }

    #[test]
    fn shift_and_alt_are_tracked() {
        // The modifier presses themselves stay unstamped (their own events carry nothing); only
        // the key pressed under both carries the pair.
        let both = Modifiers { shift: true, alt: true, ..Modifiers::NONE };
        let (events, held) = stamp(vec![key(Key::ShiftLeft, true), key(Key::AltLeft, true), key(Key::F2, true)]);
        assert_eq!(stamped(&events)[2], both);
        assert_eq!(held, both);
    }

    #[test]
    fn a_lost_focus_clears_a_stuck_modifier() {
        let (events, held) = stamp(vec![key(Key::ControlLeft, true), Event::WindowFocused(false), key(Key::S, true)]);
        assert_eq!(stamped(&events), vec![Modifiers::NONE, Modifiers::NONE], "the S after the lost focus is a bare S");
        assert_eq!(held, Modifiers::NONE);
    }

    #[test]
    fn the_frame_announces_its_end_state() {
        let ctrl = Modifiers { ctrl: true, command: true, ..Modifiers::NONE };
        let (events, _) = stamp(vec![key(Key::ControlLeft, true)]);
        let last = events.last().cloned().expect("the frame ends with a ModifiersChanged");
        assert!(matches!(last, Event::ModifiersChanged(m) if m == ctrl));
    }

    /// The stamped state must match what the real binding patterns expect: egui matches a
    /// pattern with `matches_logically`, and a pattern holding COMMAND matches only when
    /// `command` is set (egui-winit sets it from Ctrl off macOS).
    #[test]
    fn stamped_modifiers_match_the_binding_patterns() {
        let pattern = Modifiers { command: true, ..Modifiers::NONE };
        let (_, held) = stamp(vec![key(Key::ControlLeft, true), key(Key::S, true)]);
        assert!(held.matches_logically(pattern), "a Ctrl+S chord matches a COMMAND+S binding");
        let (_, held) = stamp(vec![key(Key::ControlLeft, true), key(Key::ControlLeft, false), key(Key::S, true)]);
        assert!(!held.matches_logically(pattern));
    }

    /// The wheel and the pointer buttons carry the held modifiers too (Ctrl + wheel zooms the
    /// page, Ctrl + click opens a cross-reference), where egui-winit fills them from its —
    /// always empty — state.
    #[test]
    fn wheel_and_click_carry_the_held_modifiers() {
        let ctrl = Modifiers { ctrl: true, command: true, ..Modifiers::NONE };
        let (events, _) = stamp(vec![
            key(Key::ControlLeft, true),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, -3.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Wheel,
                modifiers: Modifiers::NONE,
            },
            egui::Event::PointerButton { pos: egui::Pos2::ZERO, button: egui::PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
        ]);
        let carried: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                egui::Event::MouseWheel { modifiers, .. } | egui::Event::PointerButton { modifiers, .. } => Some(*modifiers),
                _ => None,
            })
            .collect();
        assert_eq!(carried, vec![ctrl, ctrl]);
    }
}
