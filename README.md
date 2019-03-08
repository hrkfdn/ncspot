# ncspot
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
* `libpulse-dev` (or `portaudio-dev`, if you want to use the PortAudio backend)
* `libncurses-dev` and `libssl-dev`
* A Spotify premium account
* pkg-config

## Usage

* Set your login credentials (see configuration)
* Build using `cargo build --release`
* The initial screen is the debug log. Press `F2` for the queue and `F3` to
  search for a track. More key bindings are described below.

### Key Bindings

These keybindings are hardcoded for now. In the future it may be desirable to
have them configurable.

* Navigate through the screens using the F-keys:
  * `F1`: Queue
  * `F2`: Search
  * `F3`: Playlists
    * `d` deletes the currently selected track
    * `c` clears the entire playlist
* `F9`: Debug log
* Tracks and playlists can be played using `Return` and queued using `Space`
* `Shift-p` toggles playback of a track
* `Shift-s` stops a track
* `q` quits ncspot

## Configuration

For now, a configuration file containing Spotify login data needs to be created
manually, until a login-screen is implemented
(https://github.com/hrkfdn/ncspot/issues/1).

The file needs to look like this:

```
username = "spotify_user"
password = "spotify_password"
```

Please save it to `~/.config/ncspot`.

## Audio backends

By default ncspot is built using the PulseAudio backend.
To make it use the PortAudio backend (e.g. for macOS, *BSD, ..),
you need to recompile ncspot with the `portaudio_backend` feature:

* `cargo run --no-default-features --features portaudio_backend`
