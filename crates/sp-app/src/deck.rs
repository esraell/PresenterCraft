//! Slides made from pictures: the pages of a PDF, a presentation file (PowerPoint, Keynote,
//! OpenDocument) turned into a PDF by the program that made it (see [`crate::convert`]), or a
//! folder of pictures exported from one. Each page or picture
//! becomes a slide showing it across the whole slide, in a new presentation named after the
//! file or folder. Drawing a deck's pages takes a few seconds, so it is done on a thread of its
//! own: the operator's window and keys go on working meanwhile, even mid-service.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use sp_model::{Group, Object, Presentation, SlideRect, Uuid};

/// A PDF, whose pages become slides.
pub const PDF_ENDING: &str = "pdf";
/// Files presentation programs save, turned into a PDF by a program on this Mac first.
pub const DECK_ENDINGS: [&str; 6] = ["pptx", "ppt", "pps", "ppsx", "key", "odp"];
/// The most pages or pictures a deck makes slides of.
const MOST_SLIDES: usize = 300;
/// The largest PDF read.
const LARGEST_PDF: u64 = 512 * 1024 * 1024;
/// The size each page is drawn at, at most: a full 1080p screen.
const PAGE_SIZE: (u32, u32) = (1920, 1080);

fn has_ending(path: &Path, endings: &[&str]) -> bool {
    path.extension().is_some_and(|ext| endings.iter().any(|e| ext.eq_ignore_ascii_case(e)))
}

/// True for a PDF.
pub fn is_pdf(path: &Path) -> bool {
    has_ending(path, &[PDF_ENDING])
}

/// True for a file a presentation program saved (PowerPoint, Keynote, OpenDocument).
pub fn is_presentation_file(path: &Path) -> bool {
    has_ending(path, &DECK_ENDINGS)
}

/// The pictures directly inside a folder, in the order people number them (Slide2 before
/// Slide10), or `None` when there are none or the folder holds songs, which go in as songs.
pub fn pictures_in(folder: &Path) -> Option<Vec<PathBuf>> {
    let mut pictures = Vec::new();
    for entry in std::fs::read_dir(folder).ok()?.flatten() {
        let path = entry.path();
        let hidden = path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if hidden || !path.is_file() {
            continue;
        }
        if crate::load::is_importable(&path) || crate::load::is_song_file(&path) {
            return None;
        }
        if has_ending(&path, &crate::media::PICTURES) {
            pictures.push(path);
        }
    }
    pictures.sort_by(|a, b| natural_order(&a.to_string_lossy(), &b.to_string_lossy()));
    (!pictures.is_empty()).then_some(pictures)
}

/// Compares names the way people number things: runs of digits by their value, so "Slide2"
/// comes before "Slide10"; letters without regard to case.
pub fn natural_order(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let number = |chars: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(c) = chars.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        chars.next();
                    }
                    digits.trim_start_matches('0').to_owned()
                };
                let (x, y) = (number(&mut a), number(&mut b));
                let by_value = x.len().cmp(&y.len()).then_with(|| x.cmp(&y));
                if by_value != std::cmp::Ordering::Equal {
                    return by_value;
                }
            }
            (Some(x), Some(y)) => {
                let by_letter = x.to_lowercase().cmp(y.to_lowercase());
                if by_letter != std::cmp::Ordering::Equal {
                    return by_letter;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// A slide showing one picture across the whole slide.
fn picture_slide(file: String, name: String, width: u32, height: u32) -> sp_model::Slide {
    let mut picture = Object::image(file, name, width, height);
    picture.frame = Some(SlideRect { x: 0.0, y: 0.0, w: sp_model::SLIDE_WIDTH, h: sp_model::SLIDE_HEIGHT });
    sp_model::Slide::new(vec![picture])
}

/// A presentation of slides, named.
fn presentation(title: String, slides: Vec<sp_model::Slide>) -> Presentation {
    Presentation {
        id: Uuid::new_v4(),
        title,
        groups: vec![Group { id: Uuid::new_v4(), name: String::new(), color: sp_model::group_color(""), slides }],
        copyright: sp_model::Copyright::default(),
        theme: None,
        arrangements: Vec::new(),
        arrangement: None,
        loops: false,
        transition: None,
    }
}

/// What a deck is made from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// A PDF, or a folder of pictures.
    File(PathBuf),
    /// Pictures chosen together, as slides in this order, under this title.
    Pictures { title: String, files: Vec<PathBuf> },
}

impl Source {
    /// What the deck is known by while it is being made.
    fn key(&self) -> PathBuf {
        match self {
            Source::File(path) => path.clone(),
            Source::Pictures { files, .. } => files.first().cloned().unwrap_or_default(),
        }
    }

    /// The name of the file or folder, for messages.
    pub fn name(&self) -> String {
        match self {
            Source::File(path) => path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            Source::Pictures { title, .. } => title.clone(),
        }
    }
}

/// Pictures chosen together, as one deck: in the order people number them, named after their
/// folder.
pub fn chosen_pictures(mut files: Vec<PathBuf>) -> Source {
    files.sort_by(|a, b| natural_order(&a.to_string_lossy(), &b.to_string_lossy()));
    let title = files
        .first()
        .and_then(|f| f.parent())
        .and_then(Path::file_name)
        .map_or_else(|| "Slides".to_owned(), |n| n.to_string_lossy().into_owned());
    Source::Pictures { title, files }
}

/// Makes the presentation for a deck, on the calling thread. Each page or picture is saved in
/// the Images folder, so the deck keeps working when the file is moved or deleted.
pub fn make(source: &Source) -> Result<Presentation, String> {
    let (title, pictures) = match source {
        Source::Pictures { title, files } => (title.clone(), files.clone()),
        Source::File(path) if path.is_dir() => {
            let title = path.file_name().map_or_else(|| "Slides".to_owned(), |n| n.to_string_lossy().into_owned());
            let pictures = pictures_in(path).ok_or_else(|| format!("{title} has no pictures in it."))?;
            (title, pictures)
        }
        Source::File(path) if is_presentation_file(path) => return make_from_presentation_file(path),
        Source::File(path) => {
            let title = path.file_stem().map_or_else(|| "Slides".to_owned(), |s| s.to_string_lossy().into_owned());
            return make_from_pdf(path, title);
        }
    };
    let mut slides = Vec::new();
    for picture in pictures.iter().take(MOST_SLIDES) {
        let imported = crate::images::import(picture)?;
        slides.push(picture_slide(imported.file, imported.name, imported.width, imported.height));
    }
    Ok(presentation(title, slides))
}

/// A presentation file's slides, through a PDF made by the program that opens it. The PDF is
/// deleted afterwards; the slides' pictures stay in the Images folder.
fn make_from_presentation_file(path: &Path) -> Result<Presentation, String> {
    let title = path.file_stem().map_or_else(|| "Slides".to_owned(), |s| s.to_string_lossy().into_owned());
    let work = crate::bible_store::data_dir()
        .ok_or("Could not find where to work on this computer.")?
        .join("Converting")
        .join(Uuid::new_v4().to_string());
    let made = crate::convert::to_pdf(path, &work);
    let deck = made.and_then(|(pdf, _)| {
        let deck = make_from_pdf(&pdf, title);
        let _ = std::fs::remove_file(&pdf);
        deck
    });
    let _ = std::fs::remove_dir_all(&work);
    deck
}

/// A PDF's pages as slides, in a presentation with this title.
fn make_from_pdf(path: &Path, title: String) -> Result<Presentation, String> {
    let size = std::fs::metadata(path).map_err(|e| format!("{title} could not be read ({e}).")).map(|m| m.len())?;
    if size > LARGEST_PDF {
        return Err(format!("{title} is too large to make slides of."));
    }
    let folder = crate::images::folder().ok_or("Could not find where to keep pictures on this computer.")?;
    std::fs::create_dir_all(&folder).map_err(|e| format!("Could not make the Images folder: {e}"))?;
    let mut slides = Vec::new();
    let mut problem = None;
    sp_video::pdf_pages(path, PAGE_SIZE.0, PAGE_SIZE.1, MOST_SLIDES, |number, _, frame| {
        let file = format!("{}.png", Uuid::new_v4());
        match save_png(&folder.join(&file), &frame) {
            Ok(()) => {
                slides.push(picture_slide(file, format!("{title} {number}"), frame.width, frame.height));
                true
            }
            Err(reason) => {
                problem = Some(format!("Page {number} of {title} could not be saved ({reason})."));
                false
            }
        }
    })?;
    if let Some(problem) = problem {
        return Err(problem);
    }
    Ok(presentation(title, slides))
}

/// Saves a drawn page as a PNG, without its alpha (a page is opaque), compressed quickly.
fn save_png(path: &Path, frame: &sp_video::Frame) -> Result<(), String> {
    use image::ImageEncoder;
    let rgb: Vec<u8> = frame.rgba.as_chunks::<4>().0.iter().flat_map(|&[r, g, b, _]| [r, g, b]).collect();
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let encoder = image::codecs::png::PngEncoder::new_with_quality(
        std::io::BufWriter::new(file),
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Adaptive,
    );
    encoder.write_image(&rgb, frame.width, frame.height, image::ExtendedColorType::Rgb8).map_err(|e| e.to_string())
}

/// What one deck being made came to.
pub type Made = (PathBuf, Result<Presentation, String>);

/// Decks being made on threads of their own.
pub struct Decks {
    tx: Sender<Made>,
    rx: Receiver<Made>,
    busy: Vec<PathBuf>,
}

impl Default for Decks {
    fn default() -> Self {
        let (tx, rx) = channel();
        Decks { tx, rx, busy: Vec::new() }
    }
}

impl Decks {
    /// Starts making a deck; `wake` is called when it is done, to redraw the window. A deck
    /// already being made isn't started twice.
    pub fn start(&mut self, source: Source, wake: impl FnOnce() + Send + 'static) {
        let path = source.key();
        if self.busy.contains(&path) {
            return;
        }
        self.busy.push(path.clone());
        let tx = self.tx.clone();
        let work = {
            let path = path.clone();
            move || {
                let made = make(&source);
                let _ = tx.send((path, made));
                wake();
            }
        };
        if std::thread::Builder::new().name("making slides".to_owned()).spawn(work).is_err() {
            self.busy.retain(|busy| *busy != path);
        }
    }

    /// The decks finished since the last look.
    pub fn finished(&mut self) -> Vec<Made> {
        let done: Vec<Made> = self.rx.try_iter().collect();
        for (path, _) in &done {
            self.busy.retain(|busy| busy != path);
        }
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_put_in_the_order_people_number_them() {
        let mut names = vec!["Slide10.png", "slide2.png", "Slide1.png", "Slide02b.png", "Intro.png"];
        names.sort_by(|a, b| natural_order(a, b));
        assert_eq!(names, ["Intro.png", "Slide1.png", "slide2.png", "Slide02b.png", "Slide10.png"]);
    }

    #[test]
    fn presentation_files_and_pdfs_are_told_apart() {
        assert!(is_presentation_file(Path::new("Sermon.PPTX")) && is_presentation_file(Path::new("Talk.key")));
        assert!(!is_presentation_file(Path::new("Sermon.pdf")) && is_pdf(Path::new("Sermon.PDF")));
    }

    #[test]
    fn a_folder_of_pictures_is_a_deck_but_one_with_songs_is_not() {
        let folder = std::env::temp_dir().join(format!("softpresenter-deck-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        for name in ["Slide10.png", "Slide2.png", ".hidden.png", "notes.md"] {
            std::fs::write(folder.join(name), b"x").unwrap();
        }
        let found = pictures_in(&folder).unwrap();
        let names: Vec<_> = found.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["Slide2.png", "Slide10.png"]);
        std::fs::write(folder.join("song.txt"), b"words").unwrap();
        assert!(pictures_in(&folder).is_none(), "a folder with songs in it goes in as songs");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
