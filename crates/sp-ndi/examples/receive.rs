//! Receives pictures from the NDI source whose name contains the given words and says what
//! they hold. Used to check that SoftPresenter's NDI picture really arrives:
//! `cargo run -p sp-ndi --features sdk-examples --example receive -- "SoftPresenter"`.
//!
//! With a number of seconds after the name it stays connected that long, and says when each
//! picture that differs from the one before it came, counted from when it connected and by the
//! clock (seconds into the UTC day, to set beside when something was done), with a fingerprint
//! that tells one picture from another:
//! `cargo run -p sp-ndi --features sdk-examples --example receive -- "SoftPresenter" 10`.

#[cfg(feature = "sdk-examples")]
fn main() {
    use std::time::{Duration, Instant};

    use grafton_ndi::{Finder, FinderOptions, NDI, Receiver, ReceiverColorFormat, ReceiverOptions};

    let wanted = std::env::args().nth(1).unwrap_or_default();
    let ndi = match NDI::new() {
        Ok(ndi) => ndi,
        Err(e) => return eprintln!("The NDI runtime could not be started: {e}"),
    };
    let options = FinderOptions::builder().show_local_sources(true).build();
    let Ok(finder) = Finder::new(&ndi, &options) else { return eprintln!("Could not look for sources.") };
    let mut found = None;
    for _ in 0..10 {
        let _ = finder.wait_for_sources(Duration::from_millis(500));
        if let Ok(sources) = finder.current_sources() {
            found = sources.into_iter().find(|source| source.name.contains(&wanted));
        }
        if found.is_some() {
            break;
        }
    }
    let Some(source) = found else { return println!("no source with \"{wanted}\" in its name") };
    println!("source: {}", source.name);
    let options = ReceiverOptions::builder(source).color(ReceiverColorFormat::RGBX_RGBA).build();
    let receiver = match Receiver::new(&ndi, &options) {
        Ok(receiver) => receiver,
        Err(e) => return eprintln!("Could not connect: {e}"),
    };
    let connected = Instant::now();
    // What a picture holds, and a fingerprint of its pixels (FNV-1a) to tell pictures apart.
    let describe = |data: &[u8]| {
        let pixels = data.as_chunks::<4>().0;
        let bright = pixels.iter().filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200).count();
        let lit = pixels.iter().filter(|p| p[0] > 40 || p[1] > 40 || p[2] > 40).count();
        let print = data
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3));
        (format!("pixels: {} total, {lit} not black, {bright} white", pixels.len()), print)
    };
    let Some(stay) = std::env::args().nth(2).and_then(|seconds| seconds.parse::<f32>().ok()) else {
        return match receiver.video().capture(Duration::from_secs(8)) {
            Ok(frame) => {
                println!("picture: {}x{} {:?}", frame.width(), frame.height(), frame.pixel_format());
                println!("{}", describe(frame.data()).0);
            }
            Err(e) => println!("no picture arrived: {e}"),
        };
    };
    let (mut pictures, mut last) = (0_u32, None);
    while connected.elapsed().as_secs_f32() < stay {
        let Ok(frame) = receiver.video().capture(Duration::from_millis(200)) else { continue };
        pictures += 1;
        let (holds, print) = describe(frame.data());
        if last != Some(print) {
            last = Some(print);
            let after = connected.elapsed().as_secs_f32();
            let clock = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0.0, |since| since.as_secs_f64() % 86_400.0);
            println!(
                "{after:.2} s (clock {clock:.2}): picture {pictures}, {}x{}, fingerprint {print:016x}; {holds}",
                frame.width(),
                frame.height()
            );
        }
    }
    println!("{pictures} pictures in {stay} s");
}

#[cfg(not(feature = "sdk-examples"))]
fn main() {
    eprintln!("Build with --features sdk-examples to receive NDI.");
}
