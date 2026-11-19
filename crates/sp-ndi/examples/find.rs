//! Lists the NDI sources visible on the network for a few seconds. Used to check that
//! SoftPresenter's source really appears: `cargo run -p sp-ndi --features sdk-examples --example find`.

#[cfg(feature = "sdk-examples")]
fn main() {
    use std::time::Duration;

    use grafton_ndi::{Finder, FinderOptions, NDI};

    let ndi = match NDI::new() {
        Ok(ndi) => ndi,
        Err(e) => return eprintln!("The NDI runtime could not be started: {e}"),
    };
    let options = FinderOptions::builder().show_local_sources(true).build();
    let finder = match Finder::new(&ndi, &options) {
        Ok(finder) => finder,
        Err(e) => return eprintln!("Could not look for sources: {e}"),
    };
    let mut seen: Vec<String> = Vec::new();
    for _ in 0..6 {
        let _ = finder.wait_for_sources(Duration::from_millis(500));
        if let Ok(sources) = finder.current_sources() {
            for source in sources {
                if !seen.contains(&source.name) {
                    seen.push(source.name.clone());
                }
            }
        }
    }
    seen.sort();
    println!("{} source(s) found", seen.len());
    for name in seen {
        println!("  {name}");
    }
}

#[cfg(not(feature = "sdk-examples"))]
fn main() {
    eprintln!("Build with --features sdk-examples to look for NDI sources.");
}
