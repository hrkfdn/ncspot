# ncspot

[![Gitter](https://badges.gitter.im/ncspot/community.svg)](https://gitter.im/ncspot/community?utm_source=badge&utm_medium=badge&utm_campaign=pr-badge)
[![Build Status](https://travis-ci.com/hrkfdn/ncspot.svg?token=DoBH2xZ13CfuTfqgEyp7&branch=develop)](https://travis-ci.com/hrkfdn/ncspot)

ncurses Spotify client written in Rust using librespot. It is heavily inspired
by ncurses MPD clients, such as ncmpc.  My motivation was to provide a simple
and resource friendly alternative to the official client as well as to support
platforms that currently don't have a Spotify client, such as the *BSDs.

[![Search](/screenshots/screenshot-thumb.png?raw=true)](/screenshots/screenshot.png?raw=true)

**NOTE**: ncspot is still in a very early development stage. Things will break
and change. The feature set is still very limited. Also, as this is my first
contact with Rust, some design decisions may need to be reworked in the
future. Contributions welcome, but please be kind ;)

## Requirements

* Rust
* `libasound2-dev` (or `portaudio-dev`, if you want to use the PortAudio backend)
* `libncurses-dev` and `libssl-dev`
* `libdbus-1-dev`
* `libxcb` + development headers (for clipboard access)
* A Spotify premium account
* pkg-config

## Usage

* Build using `cargo build --release`
* For debugging, pass a debug log filename, e.g. `ncspot -d debug.log`

### Key Bindings

These keybindings are hardcoded for now. In the future it may be desirable to
have them configurable.

* Navigate through the screens using the F-keys:
  * `F1`: Queue
    * `c` clears the entire queue
    * `d` deletes the currently selected track
    * `Ctrl-s` opens a dialog to save the queue to a playlist
  * `F2`: Search
  * `F3`: Library
    * `d` deletes the currently selected playlist
* Tracks and playlists can be played using `Return` and queued using `Space`
* `s` will save or remove the currently selected track to your library
* `o` will open a detail view or context menu for the selected item
* `Shift-o` will open a context menu for the currently playing track
* `a` will open the album view for the selected item
* `A` will open the artist view for the selected item
* `Backspace` closes the current view
* `Shift-p` toggles playback of a track
* `Shift-s` stops a track
* `Shift-r` updates the playlist cache
* `<` and `>` play the previous or next track
* `,` and `.` to rewind or skip forward
* `r` to toggle repeat mode
* `z` to toggle shuffle playback
* `q` quits ncspot
* `x` copies a sharable URL to the song to the system clipboard
* `Shift-x` copies a sharable URL to the currently selected item to the system clipboard

You can also open a Vim style commandprompt using `:`, the following commands
are supported:

* `quit`: Quit ncspot
* `toggle`: Toggle playback
* `stop`: Stop playback
* `previous`/`next`: Play previous/next track
* `clear`: Clear playlist
* `share [current | selected]`: Copies a sharable URL of either the selected item or the currernt song to the system clipboard

The screens can be opened with `queue`, `search`, `playlists` and `log`, whereas
`search` can be supplied with a search term that will be entered after opening
the search view.

## Configuration

Configuration is saved to `~/.config/ncspot/config.toml`.

### Theming

The color palette can be modified in the configuration. For instance, to have
ncspot match Spotify's official client, you can add the following entries to the
configuration file:

```
[theme]
background = "black"
primary = "light white"
secondary = "light black"
title = "green"
playing = "green"
playing_bg = "black"
highlight = "light white"
highlight_bg = "#484848"
error = "light white"
error_bg = "red"
statusbar = "black"
statusbar_progress = "green"
statusbar_bg = "green"
cmdline = "light white"
cmdline_bg = "black"
```

More examples can be found in pull request
https://github.com/hrkfdn/ncspot/pull/40.

## Audio backends

By default ncspot is built using the Rodio backend.  To make it use the
PortAudio backend (e.g. *BSD), you need to recompile ncspot with the
`portaudio_backend` feature:

* `cargo run --no-default-features --features
  portaudio_backend,cursive/pancurses-backend`
