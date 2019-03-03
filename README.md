# ncspot
[![Build Status](https://travis-ci.com/hrkfdn/ncspot.svg?token=DoBH2xZ13CfuTfqgEyp7&branch=develop)](https://travis-ci.com/hrkfdn/ncspot)

ncurses Spotify client written in Rust using librespot. It is heavily inspired
by ncurses MPD clients, such as ncmpc.  My motivation was to provide a simple
and resource friendly alternative to the official client as well as to support
platforms that currently don't have a Spotify client, such as the *BSDs.

[![Search](/screenshots/search-th.png?raw=true)](/screenshots/search.png?raw=true)

**NOTE**: ncspot is still in a very early development stage. Things will break
and change. The feature set is still very limited.

## Usage

* Set your login credentials (see configuration)
* Build using `cargo build --release`
* Navigate through the screens using the F-keys:
  * `F1`: Debug log
  * `F2`: Queue
  * `F3`: Search
* Tracks can be played using `Return` and queued using `Space`

## Requirements

* Rust
* `libpulse-dev` (or `portaudio-dev`, if you want to use the PortAudio backend)

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
