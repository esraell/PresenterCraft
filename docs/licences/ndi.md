# NDI: what the licence asks of PresenterCraft

A summary, in our own words, of the NDI SDK License Agreement that comes with the SDK
(`/Library/NDI SDK for Apple/NDI SDK License Agreement.pdf`), read on 2026-11-19 for SDK
version 6. This is a working note, not legal advice. Read the agreement itself before
publishing or selling anything that includes NDI.

## What we may do

- Use the SDK, free of royalties, to build software that runs on ordinary computers
  (desktops, laptops, servers the user controls). PresenterCraft is that kind of software.
  Fixed-purpose appliances, locked-down cloud services and embedded devices are excluded and
  need a commercial licence from NDI.
- Distribute the parts of the SDK that its documentation marks as redistributable (the NDI
  runtime), and only in the way that documentation says.

## What we must not do

- Put any other SDK file in this repository or in a download: no headers, examples, tools or
  documentation. The agreement treats the SDK as confidential. The source here only calls
  NDI through a third-party library; it contains nothing from the SDK.
- Modify, reverse engineer, decompile or work around the limits of the SDK or NDI's
  protocols, or let anyone else do so.
- Ship a version that is not compatible with other NDI products, or that falls behind: the
  agreement asks products to use the latest SDK available when they are built or distributed.

## What a published PresenterCraft must carry

- **Licence terms for users.** Whatever licence PresenterCraft is distributed under has to
  include terms that: forbid modifying or reverse engineering the NDI parts and working around
  their limits; forbid removing NDI's notices; disclaim warranties and liability on NDI's
  behalf; follow United States export law; and name NDI as the copyright owner of its part.
  An open-source licence alone does not say these things, so they have to be added for the
  NDI part before publishing a build with NDI in it.
- **Trademark notes.** "NDI" may be used only to say the product is compatible with NDI
  products, with a clear note that NDI® is a registered trademark of Vizrt NDI AB. Nothing may
  suggest NDI sponsors or endorses PresenterCraft.
- **Third-party notices.** The runtime's own licence list
  (`libndi_licenses.txt`, in the SDK's `lib` folder) goes beside the runtime wherever the
  runtime is shipped.

## Other points

- The agreement is governed by the laws of Sweden.
- NDI may test a product and end the licence if it does not work to their satisfaction; either
  side may end it with thirty days' notice.
- Patented codecs (H.264, H.265, AAC) are a separate matter: the SDK documentation says the
  product's maker is responsible for licensing them. PresenterCraft sends uncompressed frames
  to the SDK and uses none of those itself.

## How PresenterCraft uses it

- NDI is part of the normal build, which therefore needs the SDK installed. A computer
  without it can build the rest with `--no-default-features`; that build says NDI is not
  included.
- At run time it loads the NDI runtime already installed on the computer. Nothing from NDI is
  copied into the app.
