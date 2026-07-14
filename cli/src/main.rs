use std::path::PathBuf;
use std::process::ExitCode;

mod check;
mod doctor;
mod gui;
mod init;
mod link;
mod reference;
mod tap;

use clap::{Parser, Subcommand};
use sahou_core::diag::Diag;
use sahou_core::endpoints::{parse_endpoints, Endpoints};
use sahou_core::fmt::fmt as fmt_contract;
use sahou_core::ir::descriptor_json;
use sahou_core::parse::parse_contract;
use sahou_core::schema_check::validate_schema;

/// Sahou — a schema-first communication layer CLI for production environments
#[derive(Parser)]
#[command(name = "sahou", version, about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Scaffold a new project (a minimal valid seed to start from an empty directory)
    Init(init::InitArgs),
    /// Self-validate a contract (schema.sahou.yaml) with positioned, structured diagnostics
    Validate { file: PathBuf },
    /// Normalize to deterministic canonical YAML (note: comments are not preserved)
    Fmt {
        file: PathBuf,
        /// Overwrite the file in place (prints to stdout when omitted)
        #[arg(long)]
        write: bool,
    },
    /// Contract + endpoints -> full IR (<out-dir>/descriptor.json). Use --lang/--node to generate opt-in type stubs under <out-dir>/<node>/
    Gen {
        /// Contract file (when omitted, ./schema.sahou.yaml in the current directory)
        #[arg(default_value = "schema.sahou.yaml")]
        schema: PathBuf,
        /// endpoints.<env>.yaml (when omitted, the LAN-auto default = namespace "sahou")
        #[arg(long)]
        endpoints: Option<PathBuf>,
        /// Output directory for generated artifacts (IR = <out-dir>/descriptor.json / stub = <out-dir>/<node>/)
        #[arg(long, default_value = "gen")]
        out_dir: PathBuf,
        /// Type stub language (opt-in; use together with --node)
        #[arg(long, value_enum, requires = "node")]
        lang: Option<LangArg>,
        /// Node to generate a type stub for (use with --lang; stubs only — no sliced IR/ACL: Z27 deferred)
        #[arg(long, requires = "lang")]
        node: Option<String>,
    },
    /// One relay per machine + a WS entrypoint for Node/browser (remote-api). The engine spawns it automatically
    Link(link::LinkArgs),
    /// Observe/inject without an app: subscribe and show the core's validation results (--send publishes a valid sample)
    Tap(tap::TapArgs),
    /// Environment preflight diagnostics: probe loopback / ping / this binary's real zenoh scout / link WS
    Doctor(doctor::DoctorArgs),
    /// Detect stub <-> IR drift (for CI): compare the stub's embedded hashes against the descriptor and reject on mismatch
    Check(check::CheckArgs),
    /// Node editor GUI (localhost). All contract interpretation happens in the in-browser core wasm — the backend only moves raw bytes
    Gui(gui::GuiArgs),
    /// Print a schema-authoring reference for AI (coding agents)
    Reference,
    /// Print the bundled third-party license notices (retained for redistribution; Apache-2.0 §4 for Zenoh etc.)
    Licenses,
}

fn main() -> ExitCode {
    reset_sigpipe();
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(diags) => {
            print_diags(&diags);
            ExitCode::FAILURE
        }
    }
}

/// Restore the default SIGPIPE disposition so that `sahou <cmd> | head` (or any downstream that
/// closes the pipe early) exits quietly instead of panicking with "Broken pipe". Rust sets SIGPIPE
/// to SIG_IGN at startup, which turns a closed pipe into an EPIPE that `print!` panics on.
#[cfg(unix)]
fn reset_sigpipe() {
    // SAFETY: called once at the very start of main, before any other thread or IO.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}
#[cfg(not(unix))]
fn reset_sigpipe() {}

#[derive(Clone, Copy, clap::ValueEnum)]
enum LangArg {
    Python,
    Ts,
}

impl From<LangArg> for sahou_core::stub::StubLang {
    fn from(l: LangArg) -> Self {
        match l {
            LangArg::Python => Self::Python,
            LangArg::Ts => Self::Ts,
        }
    }
}

fn io_err(path: &std::path::Path, e: std::io::Error) -> Vec<Diag> {
    vec![Diag::new(
        "io_error",
        path.display().to_string(),
        e.to_string(),
    )]
}

fn print_diags(diags: &[Diag]) {
    for d in diags {
        println!("[{}] @{}: {}", d.code, d.path, d.message);
    }
}

fn read(path: &PathBuf) -> Result<String, Vec<Diag>> {
    std::fs::read_to_string(path).map_err(|e| io_err(path, e))
}

fn load_valid_contract(path: &PathBuf) -> Result<sahou_core::contract::Contract, Vec<Diag>> {
    let contract = parse_contract(&read(path)?)?;
    let diags = validate_schema(&contract);
    if diags.is_empty() {
        Ok(contract)
    } else {
        Err(diags)
    }
}

fn run(cli: Cli) -> Result<(), Vec<Diag>> {
    match cli.cmd {
        Cmd::Init(args) => init::run(args),
        Cmd::Validate { file } => {
            load_valid_contract(&file)?;
            println!("[ok] {}: valid", file.display());
            Ok(())
        }
        Cmd::Fmt { file, write } => {
            let src = read(&file)?;
            let out = fmt_contract(&src)?;
            if write {
                if src
                    .lines()
                    .any(|l| l.trim_start().starts_with('#') || l.contains(" #"))
                {
                    eprintln!("[warn] comments are not preserved (known limitation; spec §10-2)");
                }
                std::fs::write(&file, &out).map_err(|e| io_err(&file, e))?;
                println!("[ok] {}: formatted", file.display());
            } else {
                print!("{out}");
            }
            Ok(())
        }
        Cmd::Gen {
            schema,
            endpoints,
            out_dir,
            lang,
            node,
        } => {
            let contract = load_valid_contract(&schema)?;
            let eps = match endpoints {
                Some(p) => parse_endpoints(&read(&p)?)?,
                None => Endpoints::default(),
            };
            std::fs::create_dir_all(&out_dir).map_err(|e| io_err(&out_dir, e))?;
            let json = descriptor_json(&contract, &eps);
            let ir_path = out_dir.join("descriptor.json");
            std::fs::write(&ir_path, &json).map_err(|e| io_err(&ir_path, e))?;
            println!("[ok] {} -> {}", schema.display(), ir_path.display());
            if let (Some(lang), Some(node)) = (lang, node) {
                // Stub generation is a pure function in the core (design §8, D11). The CLI only does file IO.
                let desc = sahou_core::runtime::load_descriptor(&json)?;
                let files = sahou_core::stub::gen_stub(&desc, &node, lang.into())?;
                let node_dir = out_dir.join(&node);
                std::fs::create_dir_all(&node_dir).map_err(|e| io_err(&node_dir, e))?;
                for f in &files {
                    let p = node_dir.join(&f.rel_path);
                    std::fs::write(&p, &f.content).map_err(|e| io_err(&p, e))?;
                    println!("[ok] stub -> {}", p.display());
                }
            }
            Ok(())
        }
        Cmd::Link(args) => link::run(args),
        Cmd::Tap(args) => tap::run(args),
        Cmd::Doctor(args) => doctor::run(args),
        Cmd::Check(args) => check::run(args),
        Cmd::Gui(args) => gui::run(args),
        Cmd::Reference => {
            print!("{}", reference::reference_text());
            Ok(())
        }
        Cmd::Licenses => {
            // Bundled at compile time so a single binary satisfies the redistribution notice
            // requirement (same idea as the embedded GUI assets). Regenerate with `cargo about`.
            print!("{}", include_str!("../licenses/THIRD-PARTY-LICENSES.md"));
            Ok(())
        }
    }
}
