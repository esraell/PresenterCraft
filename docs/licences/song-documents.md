# Reading song documents saved by ProPresenter 7

SoftPresenter can import songs from the `.pro` documents ProPresenter 7 saves, so a church can
bring its library across (File › Import from ProPresenter…, or dropping the files or their
folders on the window). It only ever reads them; it never writes that format.

## Where the format comes from

The documents are Protocol Buffers. SoftPresenter's reader
(`crates/sp-model/src/pro_file.rs`) uses the field numbers from the unofficial `.proto`
definitions published by greyshirtguy at
[github.com/greyshirtguy/ProPresenter7-Proto](https://github.com/greyshirtguy/ProPresenter7-Proto),
which their author worked out as a user of the program. Nothing in SoftPresenter comes from
decompiling ProPresenter or from its own files. The definitions themselves are not copied into
this repository: the reader is SoftPresenter's own code and reads only the few fields a song
needs.

ProPresenter is a trademark of Renewed Vision, LLC. SoftPresenter is not affiliated with or
endorsed by Renewed Vision, and names ProPresenter only to say which documents it can read.

## The licence of the definitions

The definitions are under the MIT licence, which asks that its notice be kept with copies of
the work. It is reproduced in [NOTICE.md](../../NOTICE.md).

## Why this is an exception

[AGENTS.md](../../AGENTS.md) otherwise forbids file formats taken from other products. The
owner agreed this one exception on 2026-11-19, for reading a church's own songs only, through
the definitions above. It does not extend to writing the format, to other files of that
program, or to any other product.

## SoftProjector song databases and songbooks

SoftPresenter also imports songs from SoftProjector (its `spData.sqlite` database and the
`.sps` songbooks it exports), so a church moving from it keeps its songs. The layout was learnt
by reading SoftProjector's source code, which is published under the GPL version 3
(github.com/SoftProjector/softprojector): the names of its tables and columns, and how it splits
a song's text into verses and choruses. Those are facts about the format; no code was copied.
The files are read with macOS's own `sqlite3`, read-only. The songs in a church's own
SoftProjector files are the church's to bring across; the songbooks SoftProjector offers for
download carry their publishers' copyrights, and SoftPresenter does not ship any of them.
