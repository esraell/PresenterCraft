//! Checks the web engine outside the app: opens a page from a server on this computer, takes
//! its pictures 30 times a second for a few seconds, and reports what came back.
//!
//!     cargo run -p sp-video --example web_check -- http://127.0.0.1:PORT/page.html
//!
//! It also asks the page to go to a file on this computer, which must be refused. Its window is
//! far off every screen, and it takes no keyboard.

fn main() {
    let address = std::env::args().nth(1).unwrap_or_else(|| "http://127.0.0.1:8765/anim.html".to_owned());
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
        let mtm = objc2::MainThreadMarker::new().expect("the main thread");
        NSApplication::sharedApplication(mtm).setActivationPolicy(NSApplicationActivationPolicy::Prohibited);
    }
    let page = match sp_video::WebPage::open(&address, 1920, 1080) {
        Ok(page) => page,
        Err(problem) => {
            println!("could not open: {problem}");
            std::process::exit(1);
        }
    };
    let frames = page.frames();
    let started = std::time::Instant::now();
    let mut pictures = 0;
    let mut last = None;
    let mut marks: Vec<u32> = Vec::new();
    while started.elapsed().as_secs_f64() < 5.0 {
        page.capture();
        sp_video::web::pump(1.0 / 30.0);
        if let Some(frame) = frames.next() {
            pictures += 1;
            // The test page writes its frame number into the colour of its top-left corner.
            let mark = u32::from(frame.rgba[4 * (4 * frame.width as usize + 4)])
                + 256 * u32::from(frame.rgba[4 * (4 * frame.width as usize + 4) + 1]);
            marks.push(mark);
            last = Some(frame);
        }
    }
    println!("loading: {:?}; pictures in 5 s: {pictures}", page.loading());
    let distinct = {
        let mut seen = marks.clone();
        seen.dedup();
        seen.len()
    };
    println!("page frames seen: first {:?}, last {:?}, {distinct} different pictures", marks.first(), marks.last());
    if let Some(frame) = &last {
        let alpha = |x: usize, y: usize| frame.rgba[(y * frame.width as usize + x) * 4 + 3];
        println!(
            "picture {}x{}; alpha at the top-left mark {} and in an empty corner {}",
            frame.width,
            frame.height,
            alpha(4, 4),
            alpha(frame.width as usize - 4, frame.height as usize - 4)
        );
    }
    // A page sent to a file on this computer must stay where it is.
    let answer = std::rc::Rc::new(std::cell::RefCell::new(None));
    page.ask("location.href = 'file:///etc/hosts'; 'asked'", |_| {});
    sp_video::web::pump(1.0);
    let into = answer.clone();
    page.ask("JSON.stringify(location.href)", move |text| *into.borrow_mut() = text);
    sp_video::web::pump(0.5);
    println!("after asking for a local file the page is at: {:?}", answer.borrow());
}
