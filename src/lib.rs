use clap::builder::PathBufValueParser;
use librespot_playback::audio_backend;

pub const AUTHOR: &str = "Henrik Friedrichsen <henrik@affekt.org> and contributors";
pub const BIN_NAME: &str = "ncspot";

/// Return the [Command](clap::Command) that models the program's command line arguments. The
/// command can be used to parse the actual arguments passed to the program, or to automatically
/// generate a man page using clap's mangen package.
pub fn program_arguments() -> clap::Command {
    let backends = {
        let backends: Vec<&str> = audio_backend::BACKENDS.iter().map(|b| b.0).collect();
        format!("Audio backends: {}", backends.join(", "))
    };

    clap::Command::new("ncspot")
        .version(env!("CARGO_PKG_VERSION"))
        .author(AUTHOR)
        .about("cross-platform ncurses Spotify client")
        .after_help(backends)
        .arg(
            clap::Arg::new("debug")
                .short('d')
                .long("debug")
                .value_name("FILE")
                .value_parser(PathBufValueParser::new())
                .help("Enable debug logging to the specified file"),
        )
        .arg(
            clap::Arg::new("basepath")
                .short('b')
                .long("basepath")
                .value_name("PATH")
                .value_parser(PathBufValueParser::new())
                .help("custom basepath to config/cache files"),
        )
        .arg(
            clap::Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Filename of config file in basepath")
                .default_value("config.toml"),
        )
}
