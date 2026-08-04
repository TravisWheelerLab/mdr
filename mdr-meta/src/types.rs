use clap::{
    Parser, ValueEnum,
    builder::{PossibleValue, PossibleValuesParser},
};
use libmdrepo::constants::VALID_SOFTWARE;

// --------------------------------------------------
#[derive(Parser, Debug)]
#[command(arg_required_else_help = true, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

// --------------------------------------------------
#[derive(Parser, Debug)]
pub enum Command {
    /// Check TOML
    Check(CheckArgs),

    /// Create a full example file
    Eg(EgArgs),

    /// Generate metadata file from directory contents
    Gen(GenArgs),

    /// Print metadata in JSON format
    ToJson(ToJsonArgs),

    /// Print metadata in TOML format
    ToToml(ToTomlArgs),

    /// Upgrade binary to the latest release
    Upgrade,
}

#[derive(Debug, PartialEq, Clone)]
pub enum FileFormat {
    Json,
    Toml,
}

impl ValueEnum for FileFormat {
    fn value_variants<'a>() -> &'a [Self] {
        &[FileFormat::Json, FileFormat::Toml]
    }

    fn to_possible_value<'a>(&self) -> Option<PossibleValue> {
        Some(match self {
            FileFormat::Json => PossibleValue::new("json"),
            FileFormat::Toml => PossibleValue::new("toml"),
        })
    }
}

#[derive(Debug, Parser)]
pub struct EgArgs {
    /// Output format
    #[arg(short, long, value_name = "FORMAT")]
    pub format: Option<FileFormat>,

    /// Output filename
    #[arg(short, long, value_name = "OUTPUT", default_value = "-")]
    pub outfile: String,

    /// Only output required fields
    #[arg(short, long)]
    pub minimal: bool,
}

#[derive(Debug, Parser)]
pub struct GenArgs {
    /// Simulation software/engine, e.g. GROMACS
    ///
    /// Required because some file extensions mean different things for
    /// different engines (.gro is GROMACS's structure file but SPONGE's
    /// topology file; .crd is AMBER's trajectory but CHARMM/NAMD's
    /// structure), so files can't be classified correctly without knowing
    /// which convention applies. Validated against the same software names
    /// Meta::check() accepts.
    #[arg(
        short,
        long,
        value_name = "SOFTWARE",
        value_parser = PossibleValuesParser::new(VALID_SOFTWARE.keys().copied())
    )]
    pub software: String,

    /// Input directory (defaults to the current directory)
    #[arg(short, long, value_name = "DIR")]
    pub directory: Option<String>,

    /// Output format
    #[arg(short, long, value_name = "FORMAT")]
    pub format: Option<FileFormat>,

    /// Output filename
    #[arg(short, long, value_name = "OUTPUT", default_value = "-")]
    pub outfile: String,

    /// Trajectory filename
    #[arg(long, value_name = "TRAJ")]
    pub trajectory: Option<String>,

    /// Structure filename
    #[arg(long, value_name = "STRUCT")]
    pub structure: Option<String>,

    /// Topology filename
    #[arg(long, value_name = "TOPO")]
    pub topology: Option<String>,
}

#[derive(Debug, Parser)]
pub struct ToJsonArgs {
    /// Input filename
    #[arg(value_name = "FILE")]
    pub filename: String,

    /// Output filename
    #[arg(short, long, value_name = "OUTPUT", default_value = "-")]
    pub outfile: String,
}

#[derive(Debug, Parser)]
pub struct ToTomlArgs {
    /// Input filename
    #[arg(value_name = "FILE")]
    pub filename: String,

    /// Output filename
    #[arg(short, long, value_name = "OUTPUT", default_value = "-")]
    pub outfile: String,
}

// --------------------------------------------------
#[derive(Parser, Debug)]
#[command(alias = "ch")]
pub struct CheckArgs {
    /// Input filename or "-" for STDIN
    // "required" as well as "num_args", or checking nothing at all is a
    // pass: num_args only constrains the arity when the arg is present.
    #[arg(value_name = "FILE", num_args = 1.., required = true)]
    pub filenames: Vec<String>,

    /// Allow missing PDB/Uniprot IDs
    #[arg(short, long)]
    pub no_id: bool,
}

// --------------------------------------------------
#[derive(Debug, strum_macros::Display, PartialEq)]
pub enum FileType {
    Trajectory,
    Structure,
    Topology,
    Other,
}

// --------------------------------------------------
pub struct FileInfo {
    pub file_name: String,
    pub file_type: FileType,
    pub size: u64,
}
