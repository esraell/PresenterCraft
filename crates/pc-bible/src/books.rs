//! The sixty-six books: their codes in Bible files, and the names people type for them.

/// One book of the Bible.
pub struct Book {
    /// The code used in Bible files, such as `JOH`.
    pub code: &'static str,
    pub english: &'static str,
    /// Other things people type for it in English, without spaces or full stops, lower case.
    pub english_short: &'static [&'static str],
    pub russian: &'static str,
    pub russian_short: &'static [&'static str],
}

macro_rules! book {
    ($code:literal, $en:literal, [$($ens:literal),*], $ru:literal, [$($rus:literal),*]) => {
        Book { code: $code, english: $en, english_short: &[$($ens),*], russian: $ru, russian_short: &[$($rus),*] }
    };
}

/// Every book, in the usual order.
pub const BOOKS: [Book; 66] = [
    book!("GEN", "Genesis", ["gen", "gn"], "Бытие", ["быт"]),
    book!("EXO", "Exodus", ["exo", "ex", "exod"], "Исход", ["исх"]),
    book!("LEV", "Leviticus", ["lev", "lv"], "Левит", ["лев"]),
    book!("NUM", "Numbers", ["num", "nm"], "Числа", ["чис"]),
    book!("DEU", "Deuteronomy", ["deu", "deut", "dt"], "Второзаконие", ["втор"]),
    book!("JOS", "Joshua", ["jos", "josh"], "Иисус Навин", ["нав", "навин"]),
    book!("JDG", "Judges", ["jdg", "judg"], "Судьи", ["суд"]),
    book!("RUT", "Ruth", ["rut", "ru"], "Руфь", ["руф"]),
    book!("1SA", "1 Samuel", ["1sa", "1sam", "1sm"], "1 Царств", ["1цар"]),
    book!("2SA", "2 Samuel", ["2sa", "2sam", "2sm"], "2 Царств", ["2цар"]),
    book!("1KI", "1 Kings", ["1ki", "1kgs", "1kin"], "3 Царств", ["3цар"]),
    book!("2KI", "2 Kings", ["2ki", "2kgs", "2kin"], "4 Царств", ["4цар"]),
    book!("1CH", "1 Chronicles", ["1ch", "1chr", "1chron"], "1 Паралипоменон", ["1пар"]),
    book!("2CH", "2 Chronicles", ["2ch", "2chr", "2chron"], "2 Паралипоменон", ["2пар"]),
    book!("EZR", "Ezra", ["ezr"], "Ездра", ["езд"]),
    book!("NEH", "Nehemiah", ["neh"], "Неемия", ["неем"]),
    book!("EST", "Esther", ["est", "esth"], "Есфирь", ["есф"]),
    book!("JOB", "Job", ["jb"], "Иов", []),
    book!("PSA", "Psalms", ["psa", "ps", "psalm", "pss"], "Псалтирь", ["пс", "псалом", "псалмы"]),
    book!("PRO", "Proverbs", ["pro", "prov", "pr"], "Притчи", ["прит", "притч"]),
    book!("ECC", "Ecclesiastes", ["ecc", "eccl", "eccles"], "Екклесиаст", ["еккл", "екк"]),
    book!("SOL", "Song of Solomon", ["sol", "song", "songofsongs", "sos", "sng"], "Песня Песней", ["песн", "песнь"]),
    book!("ISA", "Isaiah", ["isa", "is"], "Исаия", ["ис"]),
    book!("JER", "Jeremiah", ["jer"], "Иеремия", ["иер"]),
    book!("LAM", "Lamentations", ["lam"], "Плач Иеремии", ["плач"]),
    book!("EZE", "Ezekiel", ["eze", "ezek", "ezk"], "Иезекииль", ["иез"]),
    book!("DAN", "Daniel", ["dan", "dn"], "Даниил", ["дан"]),
    book!("HOS", "Hosea", ["hos"], "Осия", ["ос"]),
    book!("JOE", "Joel", ["joe", "jl"], "Иоиль", ["иоил"]),
    book!("AMO", "Amos", ["amo", "am"], "Амос", ["ам"]),
    book!("OBA", "Obadiah", ["oba", "obad", "ob"], "Авдий", ["авд"]),
    book!("JON", "Jonah", ["jon", "jnh"], "Иона", ["ион"]),
    book!("MIC", "Micah", ["mic"], "Михей", ["мих"]),
    book!("NAH", "Nahum", ["nah", "na"], "Наум", []),
    book!("HAB", "Habakkuk", ["hab"], "Аввакум", ["авв"]),
    book!("ZEP", "Zephaniah", ["zep", "zeph"], "Софония", ["соф"]),
    book!("HAG", "Haggai", ["hag"], "Аггей", ["агг"]),
    book!("ZEC", "Zechariah", ["zec", "zech"], "Захария", ["зах"]),
    book!("MAL", "Malachi", ["mal"], "Малахия", ["мал"]),
    book!("MAT", "Matthew", ["mat", "matt", "mt"], "От Матфея", ["мф", "матфея", "мат"]),
    book!("MAR", "Mark", ["mar", "mk", "mrk"], "От Марка", ["мк", "марка", "мар"]),
    book!("LUK", "Luke", ["luk", "lk"], "От Луки", ["лк", "луки", "лук"]),
    book!("JOH", "John", ["joh", "jn", "jhn"], "От Иоанна", ["ин", "иоанна", "иоан"]),
    book!("ACT", "Acts", ["act", "ac"], "Деяния", ["деян"]),
    book!("ROM", "Romans", ["rom", "ro", "rm"], "К Римлянам", ["рим", "римлянам"]),
    book!("1CO", "1 Corinthians", ["1co", "1cor"], "1 Коринфянам", ["1кор"]),
    book!("2CO", "2 Corinthians", ["2co", "2cor"], "2 Коринфянам", ["2кор"]),
    book!("GAL", "Galatians", ["gal"], "Галатам", ["гал"]),
    book!("EPH", "Ephesians", ["eph"], "Ефесянам", ["еф"]),
    book!("PHI", "Philippians", ["phi", "phil", "php"], "Филиппийцам", ["флп", "фил"]),
    book!("COL", "Colossians", ["col"], "Колоссянам", ["кол"]),
    book!("1TH", "1 Thessalonians", ["1th", "1thess", "1thes"], "1 Фессалоникийцам", ["1фес"]),
    book!("2TH", "2 Thessalonians", ["2th", "2thess", "2thes"], "2 Фессалоникийцам", ["2фес"]),
    book!("1TI", "1 Timothy", ["1ti", "1tim"], "1 Тимофею", ["1тим"]),
    book!("2TI", "2 Timothy", ["2ti", "2tim"], "2 Тимофею", ["2тим"]),
    book!("TIT", "Titus", ["tit"], "Титу", ["тит"]),
    book!("PHM", "Philemon", ["phm", "philem"], "Филимону", ["флм"]),
    book!("HEB", "Hebrews", ["heb"], "Евреям", ["евр"]),
    book!("JAM", "James", ["jam", "jas"], "Иакова", ["иак"]),
    book!("1PE", "1 Peter", ["1pe", "1pet", "1pt"], "1 Петра", ["1пет", "1петр"]),
    book!("2PE", "2 Peter", ["2pe", "2pet", "2pt"], "2 Петра", ["2пет", "2петр"]),
    book!("1JO", "1 John", ["1jo", "1jn", "1jhn"], "1 Иоанна", ["1ин", "1иоан"]),
    book!("2JO", "2 John", ["2jo", "2jn", "2jhn"], "2 Иоанна", ["2ин", "2иоан"]),
    book!("3JO", "3 John", ["3jo", "3jn", "3jhn"], "3 Иоанна", ["3ин", "3иоан"]),
    book!("JUD", "Jude", ["jud", "jd"], "Иуды", ["иуд"]),
    book!("REV", "Revelation", ["rev", "rv", "revelations"], "Откровение", ["откр", "отк"]),
];

/// What someone typed for a book, made comparable: lower case, no spaces or full stops.
fn plain(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace() && *c != '.').flat_map(char::to_lowercase).collect()
}

/// The book someone means by what they typed, as its place in [`BOOKS`]. An exact name or
/// short form wins; otherwise the first book whose name starts with what was typed.
pub fn find_book(typed: &str) -> Option<usize> {
    let typed = plain(typed);
    if typed.is_empty() {
        return None;
    }
    let exact = BOOKS.iter().position(|book| {
        plain(book.english) == typed
            || plain(book.russian) == typed
            || book.code.eq_ignore_ascii_case(&typed)
            || book.english_short.contains(&typed.as_str())
            || book.russian_short.contains(&typed.as_str())
    });
    exact.or_else(|| {
        BOOKS.iter().position(|book| {
            // "От Иоанна" is also found by "Иоанна".
            let russian = plain(book.russian);
            plain(book.english).starts_with(&typed)
                || russian.starts_with(&typed)
                || russian
                    .strip_prefix("от")
                    .or_else(|| russian.strip_prefix("к"))
                    .is_some_and(|rest| rest.starts_with(&typed))
        })
    })
}

/// A book's place in [`BOOKS`] from its code in a Bible file.
pub fn book_by_code(code: &str) -> Option<usize> {
    BOOKS.iter().position(|book| book.code == code)
}
