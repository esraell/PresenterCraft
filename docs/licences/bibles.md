# Bibles PresenterCraft may offer

Which Bible translations can be downloaded from inside the app, or shipped with it, and what
each one's terms ask. Researched on 2026-11-19. This is a working note, not legal advice, and it
comes from one pass of reading: **read each source again before the download is built**, and
before a version of the app is sold.

The rule for the list in `crates/pc-app/src/bible.rs`: a Bible is offered only if its terms let
anyone pass it on, including inside software that is sold, and only if those terms were read
at the source named here.

## Offered

"eBible" means the translation's own copyright page at `https://ebible.org/<ID>/copyright.htm`.
eBible states its public-domain findings as true in the United States to the best of its
knowledge.

| Language | Translation | eBible ID | Terms | What they ask |
|---|---|---|---|---|
| English | King James Version (1769 text) | `eng-kjv2006` | Public domain | In the United Kingdom the Crown holds printing rights; nothing elsewhere |
| English | American Standard Version (1901) | `eng-asv` | Public domain | Nothing |
| English | World English Bible | `engwebp` | Public domain | The name is a trademark: use it only on unchanged text |
| English | Berean Standard Bible | `engbsb` | Public domain since 30 April 2023 (berean.bible/licensing.htm) | Asks that changed text not carry the Berean name |
| English | Young's Literal Translation (1898) | `engylt` | Public domain | Nothing |
| English | Darby Translation (1884) | `engDBY` | Public domain | Nothing |
| English | Douay-Rheims, American edition (1899) | `engDRA` | Public domain | Nothing |
| English | Geneva Bible (1599) | `enggnv` | Public domain | Nothing |
| French | Louis Segond (1910) | `fraLSG` | Public domain | Nothing |
| French | Bible Darby (1885) | `frajnd` | Public domain | Nothing |
| German | Luther (1912) | `deu1912` | Public domain | Nothing |
| German | Elberfelder (1905) | `deuelo` | Public domain | Not `deuelbbk`, which is non-commercial |
| Greek | SBL Greek New Testament | `grcsbl` | CC BY 4.0 (sblgnt.com/license) | Credit SBL and Logos |
| Greek | Byzantine text, Robinson-Pierpont | `grcmt` | Public domain | Nothing |
| Greek | Septuagint, Brenton (1851) | `grcbrent` | Public domain | Nothing |
| Hebrew | Westminster Leningrad Codex | `hboWLC` | Text public domain | Its word-by-word data is CC BY-SA; we use the text only |
| Portuguese | Bíblia Livre (2018) | `porbr2018` | CC BY 4.0 | Credit; "BLIVRE" is enough on a slide |
| Russian | Синодальный перевод (1876) | `russyn` | Public domain | Nothing. Which printed edition the text follows was not confirmed |
| Russian | Russian Open Bible | Door43 `ru_rob` | CC BY-SA 4.0 | Credit; changed text stays under the same licence |
| Spanish | Reina-Valera (1909) | `spaRV1909` | Public domain | Nothing |
| Spanish | Versión Biblia Libre | `spavbl` | CC BY-SA 4.0 | Credit; changed text stays under the same licence |
| Ukrainian | Куліш and Пулюй (1905) | `ukr1871` | Public domain | Nothing |

Rules that follow from these terms:

- Show the text exactly as it is. Capitalisation and layout on a slide are fine; rewording is not.
- A Bible under a CC licence keeps its own licence and credit. Each installed Bible shows its
  terms (the "License" link beside each translation on the Bible page).
- These texts are data the app downloads or carries. They are not part of the app's own licence.

## Never offered

Their owners do not allow free redistribution, or forbid it in anything that is sold:
NIV, ESV, NLT, NKJV, NASB, CSB, Amplified, NET (may not be bundled with anything sold),
Reina-Valera 1960, Reina Valera Gómez, LBLA, the 1996 Ukrainian Bible, the modern Russian
translations (Новый Русский Перевод, the Russian Bible Society's translation). A church that
has bought one of these elsewhere cannot get it through PresenterCraft.

## Not decided

- **Огієнко (Ukrainian).** Widely copied as public domain, but no statement from a rights holder
  was found. Left out until one is.
- **Almeida (Portuguese)** and the **Bible in Basic English**: public-domain status unclear outside
  the United States.
- **Incomplete or draft texts** that are free but not ready to project from: Open English Bible
  (CC0, not all books), Biblica's open Ukrainian New Testament (CC BY-SA), Библия свободы (draft).

## Where the text comes from

- **eBible.org**, one zip per translation (`https://ebible.org/Scriptures/<ID>_vpl.zip`). The
  "VPL" form is one verse per line (`GEN 1:1 text`), which is all a slide needs. Each zip
  carries the translation's own copyright page. eBible's list marks many texts
  "redistributable" that are non-commercial only, so the list above goes by each
  translation's own page, never by that flag.
- **Door43** (`git.door43.org`) for the Russian Open Bible, in USFM.
- Not used: collections on GitHub that mix in copyrighted Bibles, and the online Bible APIs
  whose terms forbid keeping the text for use without a connection.

## What is built

- **Four Bibles come with the app**: King James, World English, Berean Standard and the
  Synodal. All four are public domain with no conditions. Their files are in `assets/bibles/`.
- **Our file format** (`.pcbible`) is the publisher's words, one verse per line, with a few
  lines saying which Bible it is, its terms and its source, compressed. `cargo run -p pc-bible
  --example convert` makes one from eBible's one-verse-per-line text. The only changes to the
  words: the King James source marks paragraphs with a pilcrow and the translators' added
  words with square brackets, and those marks are removed (the words stay).
- **Verse numbering.** A table lines up English and Synodal numbering for the Psalms and for
  the chapters that start at a different verse. Checked against the shipped texts: of 31,102
  King James verses, 5 have no partner in the Synodal text because it joins them to the verse
  before (Leviticus 14:57, Isaiah 3:26, Acts 19:41, 2 Corinthians 11:33 and 13:14); those
  slides show the English alone.

## Still to work out

- **Downloading the others.** The Install buttons do nothing yet. The plan is to convert each
  offered Bible to our format and host the files ourselves.
