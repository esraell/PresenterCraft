//! Sends a moving test pattern as the NDI source "SoftPresenter NDI check", says once a second
//! how many receivers are watching it, and says when it stops and starts sending because of
//! that. For watching in an NDI receiver on the same computer, such as NDI's own monitor app:
//!
//!     cargo run -p sp-ndi --example ndi_check -- 120
//!
//! The number is how many seconds to run (120 if left out). It needs no NDI SDK, only the
//! runtime that NDI Tools installs, as the app does.

use std::sync::Arc;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use sp_ndi::{NdiOutput, State};

const NAME: &str = "SoftPresenter NDI check";
const WIDTH: u32 = 1920;
const HEIGHT: u32 = 1080;
/// The bar moves one place each second, and comes back to the left after this many.
const PLACES: u32 = 16;

fn main() {
    let seconds = match std::env::args().nth(1).map(|arg| arg.parse::<u64>()) {
        None => 120,
        Some(Ok(seconds)) => seconds,
        Some(Err(_)) => {
            eprintln!("usage: ndi_check [seconds to run, 120 if left out]");
            std::process::exit(2);
        }
    };
    // The source tells this thread when receivers come back, as it will tell the app's screens.
    let (joined_tx, joined) = channel();
    let output = match NdiOutput::start_with_wake(NAME, move || {
        let _ = joined_tx.send(());
    }) {
        Ok(output) => output,
        Err(reason) => {
            eprintln!("{reason}");
            std::process::exit(1);
        }
    };
    println!("Sending \"{NAME}\" for {seconds} seconds. Choose it in an NDI receiver on this computer.");
    let started = Instant::now();
    let at = || started.elapsed().as_secs();
    let mut tick: u64 = 0;
    let mut before: Option<u32> = None;
    let mut ever_counted = false;
    output.show_shared(WIDTH, HEIGHT, pattern(tick));
    while tick < seconds {
        let next = started + Duration::from_secs(tick + 1);
        match joined.recv_timeout(next.saturating_duration_since(Instant::now())) {
            Ok(()) => {
                println!(
                    "{:>4} s  a receiver connected: started sending again (the newest picture went at once)",
                    at()
                );
                // What the app's screens will do: draw a fresh picture straight away.
                output.show_shared(WIDTH, HEIGHT, pattern(tick));
                continue;
            }
            Err(RecvTimeoutError::Timeout) => {}
            // The source's thread has ended, which it does on its own only when it failed.
            Err(RecvTimeoutError::Disconnected) => give_up(&output),
        }
        tick += 1;
        if matches!(output.state(), State::Failed(_)) {
            give_up(&output);
        }
        output.show_shared(WIDTH, HEIGHT, pattern(tick));
        let receivers = output.receivers();
        match receivers {
            Some(count) => println!("{tick:>4} s  receivers: {count}"),
            None => println!("{tick:>4} s  receivers: not known"),
        }
        if receivers == Some(0) && before != Some(0) {
            if before.is_some() {
                println!("{tick:>4} s  the last receiver left: stopped sending (the newest picture is kept)");
            } else {
                println!("{tick:>4} s  nobody is watching: not sending (the newest picture is kept)");
            }
        }
        ever_counted |= receivers.is_some();
        if tick == 3 && !ever_counted {
            println!("{tick:>4} s  this NDI runtime can't count receivers, so the source sends all the time");
        }
        before = receivers;
    }
    println!("{:>4} s  stopping", at());
    drop(output);
    // The source's thread lets go of `wake` once it has taken the source off the network.
    let _ = joined.recv_timeout(Duration::from_secs(3));
    println!("{:>4} s  stopped", at());
}

/// Says why the source failed, and ends.
fn give_up(output: &NdiOutput) -> ! {
    match output.state() {
        State::Failed(reason) => eprintln!("{reason}"),
        _ => eprintln!("The NDI source stopped."),
    }
    std::process::exit(1);
}

/// A dark blue picture with a white bar in one of `PLACES` places along it, and a grey mark
/// under each place, as tightly packed RGBA rows.
fn pattern(tick: u64) -> Arc<Vec<u8>> {
    let (width, height) = (WIDTH as usize, HEIGHT as usize);
    let place_width = width / PLACES as usize;
    let place = (tick % u64::from(PLACES)) as usize;
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let in_bar = x / place_width == place && (height / 6..height * 5 / 6).contains(&y);
            let mark = y >= height * 7 / 8 && x % place_width < 8;
            let colour: [u8; 4] = if in_bar {
                [255, 255, 255, 255]
            } else if mark {
                [128, 128, 128, 255]
            } else {
                [16, 24, 64, 255]
            };
            rgba.extend_from_slice(&colour);
        }
    }
    Arc::new(rgba)
}
