//! Receives one picture from the NDI source whose name contains the given words and says what
//! it holds. Used to check that SoftPresenter's NDI picture really arrives:
//! `cargo run -p sp-ndi --features sdk-examples --example receive -- "SoftPresenter"`.

#[cfg(feature = "sdk-examples")]
fn main() {
    use std::time::Duration;

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
    match receiver.video().capture(Duration::from_secs(8)) {
        Ok(frame) => {
            let data = frame.data();
            let pixels = data.len() / 4;
            let bright = data.as_chunks::<4>().0.iter().filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200).count();
            let lit = data.as_chunks::<4>().0.iter().filter(|p| p[0] > 40 || p[1] > 40 || p[2] > 40).count();
            println!("picture: {}x{} {:?}", frame.width(), frame.height(), frame.pixel_format());
            println!("pixels: {pixels} total, {lit} not black, {bright} white");
        }
        Err(e) => println!("no picture arrived: {e}"),
    }
}

#[cfg(not(feature = "sdk-examples"))]
fn main() {
    eprintln!("Build with --features sdk-examples to receive NDI.");
}
