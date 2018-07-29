//! Application for spawining simple containers.
//!
//! The application executes in two stages. The first stage loads the configuration and performs
//! any external changes that need to be made before unsharing. It then calls itself again from
//! within the namespaces to complete the sharing.

#[macro_use]
extern crate error_chain;
extern crate libc;
extern crate loadconf;
extern crate nix;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate unshare;

#[macro_use]
mod error;
mod mount;
mod config;

use std::env;
use std::ffi::{OsStr, OsString};
use std::process;

use loadconf::Load;

use error::*;
use config::*;

/// The environment variable used to indicate that the process in inside the shared.
const COMMAND_ENV_KEY: &'static str = concat!(env!("CARGO_PKG_NAME"), "_CONTAINER_INTERNAL");

/// The expected value of the envrionment variable.
const COMMAND_ENV_VAL: &'static str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// The default command to run once in the container
const DEFAULT_COMMAND: &'static str = "/bin/sh";


/// Determines if inside or outside of container before proceeding.
fn main() -> Failure {
    let config = Config::load(env!("CARGO_PKG_NAME"));

    match env::var_os(COMMAND_ENV_KEY) {
        Some(ref val) if val == AsRef::<OsStr>::as_ref(&COMMAND_ENV_VAL) => run_child(config),
        _ => setup_unshare(config),
    }
}

/// Set up the unshare externally.
fn setup_unshare(config: Config) -> Failure {
    let program = env::current_exe().expect("Determine executable name");
    let mut command = unshare::Command::new(program);
    command.args(child_command().as_ref());
    command.env(COMMAND_ENV_KEY, COMMAND_ENV_VAL);

    config.unshare(&mut command)?;

    if !command.status()?.success() {
        return Err(ErrorKind::UnshareExit.into());
    }

    ok!()
}

/// Run the command from inside the unshare.
fn run_child(config: Config) -> Failure {
    let child = child_command();
    let child_args: &[OsString] = child.as_ref();

    let mut command = process::Command::new(&child_args[0]);
    command.args(&child_args[1..]);
    command.env_remove(COMMAND_ENV_KEY);

    config.configure(&mut command)?;

    if !command.status()?.success() {
        return Err(ErrorKind::CommandExit.into());
    }

    ok!()
}

/// Determine the command to run in the child.
fn child_command() -> Vec<OsString> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    if args.len() == 0 {
            let mut default = OsString::new();
            default.push(DEFAULT_COMMAND);
            vec![default]
    } else {
        args
    }
}
