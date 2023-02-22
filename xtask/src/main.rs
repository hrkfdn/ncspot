use clap::Arg;
use std::env;

type DynError = Box<dyn std::error::Error>;

fn main() {
    if let Err(e) = try_main() {
        eprintln!("{}", e);
        std::process::exit(-1);
    }
}

fn try_main() -> Result<(), DynError> {
    let task = env::args().nth(1);
    match task.as_deref() {
        Some("generate-manpage") => generate_manpage()?,
        _ => print_help(),
    }
    Ok(())
}

fn print_help() {
    eprintln!(
        "Tasks:
generate-manpage            Generate the man pages.
"
    )
}

fn generate_manpage() -> Result<(), DynError> {
    let out_dir = std::path::PathBuf::new();
    let cmd = clap::Command::new("ncspot")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Henrik Friedrichsen <henrik@affekt.org> and contributors")
        .about("cross-platform ncurses Spotify client")
        .arg(
            Arg::new("debug")
                .short('d')
                .long("debug")
                .value_name("FILE")
                .help("Enable debug logging to the specified file"),
        )
        .arg(
            Arg::new("basepath")
                .short('b')
                .long("basepath")
                .value_name("PATH")
                .help("custom basepath to config/cache files"),
        )
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Filename of config file in basepath")
                .default_value("config.toml"),
        );

    let man = clap_mangen::Man::new(cmd);
    let mut buffer: Vec<u8> = Default::default();
    man.render(&mut buffer)?;

    std::fs::write(out_dir.join("ncspot.1"), buffer)?;

    Ok(())
}
