use std::env;
use std::path::PathBuf;

use clap::builder::PathBufValueParser;
use clap::error::{Error, ErrorKind};
use clap::ArgMatches;
use ncspot::AUTHOR;

enum XTaskSubcommand {
    GenerateManpage,
}

impl TryFrom<&ArgMatches> for XTaskSubcommand {
    type Error = clap::Error;

    fn try_from(value: &ArgMatches) -> Result<Self, Self::Error> {
        if let Some(subcommand) = value.subcommand() {
            match subcommand.0 {
                "generate-manpage" => Ok(XTaskSubcommand::GenerateManpage),
                _ => Err(Error::new(clap::error::ErrorKind::InvalidSubcommand)),
            }
        } else {
            Err(Error::new(ErrorKind::MissingSubcommand))
        }
    }
}

type DynError = Box<dyn std::error::Error>;

fn main() {
    if let Err(e) = try_main() {
        eprintln!("{}", e);
        std::process::exit(-1);
    }
}

fn try_main() -> Result<(), DynError> {
    let arguments_model = clap::Command::new("cargo xtask")
        .version(env!("CARGO_PKG_VERSION"))
        .author(AUTHOR)
        .about("Automation using the cargo xtask convention.")
        .arg_required_else_help(true)
        .bin_name("cargo xtask")
        .disable_version_flag(true)
        .long_about(
            "
Cargo xtask is a convention that allows easy integration of third party commands into the regular
cargo workflox. Xtask's are defined as a separate package and can be used for all kinds of
automation.
        ",
        )
        .subcommand(
            clap::Command::new("generate-manpage")
                .visible_alias("gm")
                .args([clap::Arg::new("output")
                    .short('o')
                    .long("output")
                    .value_name("PATH")
                    .help("The output path for the manpage.")
                    .value_parser(PathBufValueParser::new())
                    .required(true)])
                .about("Automatic manpage generation"),
        );

    let program_parsed_arguments = arguments_model.get_matches();

    let parsed_subcommand = XTaskSubcommand::try_from(&program_parsed_arguments)?;

    match parsed_subcommand {
        XTaskSubcommand::GenerateManpage => {
            generate_manpage(program_parsed_arguments.subcommand().unwrap().1)
        }
    }
}

fn generate_manpage(subcommand_arguments: &ArgMatches) -> Result<(), DynError> {
    let cmd = ncspot::program_arguments();

    let man = clap_mangen::Man::new(cmd);
    let mut buffer: Vec<u8> = Default::default();
    man.render(&mut buffer)?;

    std::fs::write(
        subcommand_arguments
            .get_one::<PathBuf>("output")
            .unwrap()
            .join("ncspot.1"),
        buffer,
    )?;

    Ok(())
}
