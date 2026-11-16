//! The onboarding sidebar's vault graphic: a six-digit code whose digits scramble into ciphertext one by
//! one and flow along a line into the app icon, which glows briefly before the next code. Decorative and hidden from
//! screen readers; with Reduce Motion it draws one still frame. Every frame derives from the time alone.
use crate::components::*;
use nsh_base::theme;
use nsh_core::prelude::*;
use std::time::{Duration, Instant};

const CYCLE: f64 = 3.6;
const ICON: f32 = 64.0;
const HEX: &[u8] = b"0123456789ABCDEF";
/// The line between the code card (89pt) and the icon inside the 236pt sidebar column, less its 8pt margins.
const TRACK: f32 = 236.0 - 89.0 - ICON - 16.0;

#[derive(Clone, Copy, PartialEq)]
enum Digit {
    Plain,
    Scrambling,
    Sealed,
}
fn digit_state(t: f64, index: usize) -> Digit {
    if t >= 3.3 {
        return Digit::Plain;
    }
    let start = 0.9 + index as f64 * 0.1;
    if t < start {
        Digit::Plain
    } else if t < start + 0.32 {
        Digit::Scrambling
    } else {
        Digit::Sealed
    }
}
fn transfer(t: f64) -> f64 {
    (t - 1.6) / 1.1
}
fn arrival(t: f64) -> f64 {
    let x = (t - 2.5) / 0.5;
    if x > 0.0 && x < 1.0 { (x * std::f64::consts::PI).sin() } else { 0.0 }
}
fn code_opacity(t: f64) -> f64 {
    if (3.0..3.3).contains(&t) {
        1.0 - (t - 3.0) / 0.3 * 0.6
    } else if t >= 3.3 {
        (0.4 + (t - 3.3) / 0.3 * 0.6).min(1.0)
    } else {
        1.0
    }
}
/// A fixed illustrative code per round, unrelated to any account.
fn code(round: u64) -> [u8; 6] {
    let mut state = round.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
    let mut out = [0; 6];
    for d in &mut out {
        state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        *d = ((state >> 33) % 10) as u8;
    }
    out
}

pub fn vault_graphic() -> impl IntoElement {
    let time = signal(0.4_f64);
    if !reduce_motion() {
        let start = Instant::now();
        set_interval(Duration::from_millis(33), move || time.set(start.elapsed().as_secs_f64() + 0.4));
    }
    let phase = move || time.get() % CYCLE;
    let round = move || (time.get() / CYCLE) as u64 + u64::from(phase() >= 3.3);
    let glyph = move |index: usize| {
        let state = move || digit_state(phase(), index);
        text(move || match state() {
            Digit::Plain => ((b'0' + code(round())[index]) as char).to_string(),
            Digit::Scrambling => (HEX[((time.get() * 18.0) as usize + index * 7) % HEX.len()] as char).to_string(),
            Digit::Sealed => "•".to_string(),
        })
        .w(px(11.0))
        .text_center()
        .font_family(CODE_FONT.family)
        .text_size(px(17.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(move || match state() {
            Digit::Plain => theme().foreground,
            Digit::Scrambling => tok("code-accent"),
            Digit::Sealed => tok("tertiary"),
        })
    };
    view()
        .h(px(ICON))
        .flex_row()
        .items_center()
        .accessibility_elements_hidden(true)
        .child(
            view()
                .flex_row()
                .items_center()
                .h(px(38.0))
                .px(px(9.0))
                .rounded(px(9.0))
                .bg(move || theme().card.opacity(0.75))
                .border_1()
                .border_color(move || tok("separator").opacity(0.5))
                .opacity(move || code_opacity(phase()) as f32)
                .children((0..6).map(move |i| view().flex_row().child(glyph(i)).child(when(move || i == 2, || view().w(px(5.0)))))),
        )
        .child(
            view()
                .flex_1()
                .h(px(ICON))
                .mx(px(8.0))
                .overflow_hidden()
                .justify_center()
                .child(view().h(px(1.5)).rounded_full().bg(move || tok("code-accent").opacity(0.35)))
                .children((0..4).map(move |i| {
                    let progress = move || transfer(phase()) - i as f64 * 0.16;
                    view()
                        .absolute()
                        .top(px(ICON / 2.0 - 1.5))
                        .left(move || px((progress().clamp(0.0, 1.0) * f64::from(TRACK - 7.0)) as f32))
                        .w(px(7.0))
                        .h(px(3.0))
                        .rounded_full()
                        .bg(move || tok("code-accent"))
                        .opacity(move || {
                            let p = progress();
                            if p > 0.0 && p < 1.0 { (p * std::f64::consts::PI).sin() as f32 } else { 0.0 }
                        })
                })),
        )
        .child(
            view()
                .size(px(ICON))
                .child(
                    view()
                        .absolute()
                        .top(px(4.0))
                        .left(px(4.0))
                        .size(px(ICON - 8.0))
                        .rounded(px(16.0))
                        .bg(move || tok("code-accent"))
                        .blur(px(14.0))
                        .opacity(move || (0.55 * arrival(phase())) as f32),
                )
                .child(crate::components::app_icon(ICON).scale(move || (1.0 + 0.05 * arrival(phase())) as f32)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn digits_seal_in_order_and_the_next_code_shows_after_the_cycle() {
        assert!(digit_state(0.5, 0) == Digit::Plain);
        assert!(digit_state(1.0, 0) == Digit::Scrambling && digit_state(1.0, 5) == Digit::Plain);
        assert!(digit_state(2.0, 5) == Digit::Sealed);
        assert!(digit_state(3.4, 0) == Digit::Plain);
        assert!(arrival(2.75) > 0.99 && arrival(2.0) == 0.0);
        assert_ne!(code(1), code(2));
    }
}
