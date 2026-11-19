# Security

SoftPresenter runs in front of a congregation, opens files people share, and talks to other
devices on the network, so security problems matter to us.

## Reporting a problem

Report it privately: open the repository's **Security** tab and choose **Report a
vulnerability**. Please don't open a public issue for it. Say what is affected, how to make it
happen, and what someone could do with it. You'll get a reply within a week saying what
happens next.

## What counts

- A song, theme, playlist, Bible or media file that crashes SoftPresenter, makes it hang, or
  runs code.
- A downloaded Bible accepted without matching its checksum.
- Anything that can be reached over the network: NDI today, and the remote-control API when
  there is one.
- File names reaching FFmpeg's command line in an unsafe way.
- Secrets or personal details written to the log.

## Supported versions

Fixes go into `master` and the latest release.
