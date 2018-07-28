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

mod error;
mod mount;

use std::env;
use std::ffi::{OsStr, OsString};
use std::process;
use std::path::{Path, PathBuf};
use std::os::unix::fs::PermissionsExt;

use libc::{uid_t, gid_t};
use loadconf::Load;
use unshare::ExitStatus;

use error::*;
use mount::Mount;

/// The environment variable used to indicate that the process in inside the shared.
const COMMAND_ENV_KEY: &'static str = concat!(env!("CARGO_PKG_NAME"), "_CONTAINER_INTERNAL");

/// The expected value of the envrionment variable.
const COMMAND_ENV_VAL: &'static str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// The default command to run once in the container
const DEFAULT_COMMAND: &'static str = "/bin/sh";

/// Serialisable namespaces.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Namespace {
    Mount,
    Uts,
    Ipc,
    User,
    Pid,
    Net,
    Cgroup,
}

impl Into<unshare::Namespace> for Namespace {
    fn into(self) -> unshare::Namespace {
        match self {
            Namespace::Mount  => unshare::Namespace::Mount,
            Namespace::Uts    => unshare::Namespace::Uts,
            Namespace::Ipc    => unshare::Namespace::Ipc,
            Namespace::User   => unshare::Namespace::User,
            Namespace::Pid    => unshare::Namespace::Pid,
            Namespace::Net    => unshare::Namespace::Net,
            Namespace::Cgroup => unshare::Namespace::Cgroup,
        }
    }
}

#[derive(Debug, Deserialize)]
struct UidMap {
    inside: uid_t,
    outside: uid_t,
    count: uid_t,
}

impl Into<unshare::UidMap> for UidMap {
    fn into(self) -> unshare::UidMap {
        let UidMap {inside, outside, count} = self;
        unshare::UidMap {
            inside_uid: inside,
            outside_uid: outside,
            count
        }
    }
}

#[derive(Debug, Deserialize)]
struct GidMap {
    inside: gid_t,
    outside: gid_t,
    count: gid_t,
}

impl Into<unshare::GidMap> for GidMap {
    fn into(self) -> unshare::GidMap {
        let GidMap {inside, outside, count} = self;
        unshare::GidMap {
            inside_gid: inside,
            outside_gid: outside,
            count
        }
    }
}

/// Configuration for the container.
#[derive(Debug, Default, Deserialize)]
struct Config {
    #[serde(default)]
    namespaces: Vec<Namespace>,
    #[serde(default)]
    uid: uid_t,
    #[serde(default)]
    gid: gid_t,
    #[serde(default)]
    uid_map: Vec<UidMap>,
    #[serde(default)]
    gid_map: Vec<GidMap>,
    chroot_dir: Option<PathBuf>,
    #[serde(default)]
    mount: Vec<Mount>,
}

impl Config {
    /// Configure the container prior to the container.
    fn unshare(self, command: &mut unshare::Command) {
        let Config { namespaces, uid_map, gid_map, uid, gid, .. } = self;
        command.unshare(namespaces.into_iter().map(Namespace::into));
        command.set_id_maps(
            uid_map.into_iter().map(UidMap::into).collect(),
            gid_map.into_iter().map(GidMap::into).collect(),
        );

        if let (
            Some(newuidmap), Some(newgidmap)
        ) = (
            find_exec("newuidmap"), find_exec("newgidmap")
        ) {
            command.set_id_map_commands(newuidmap, newgidmap);
        }

        command.uid(uid);
        command.gid(gid);
    }

    /// Configure the container after having entered.
    fn configure(self, _command: &mut process::Command) {
        let Config { chroot_dir, mount, .. } = self;

        for mount in mount {
            println!("Mounting: {:#?}", mount);
            mount.mount().expect("Mounting filesystems");
        }

        if let Some(chroot_dir) = chroot_dir {
            let full_path = chroot_dir.canonicalize().expect("Canonicalizing chroot");
            env::set_current_dir(&full_path);
            nix::unistd::chroot(&full_path).expect("Change into chroot");
        }
    }
}

/// Determines if inside or outside of container before proceeding.
fn main() -> Result<()> {
    let config = Config::load(env!("CARGO_PKG_NAME"));

    match env::var_os(COMMAND_ENV_KEY) {
        Some(ref val) if val == AsRef::<OsStr>::as_ref(&COMMAND_ENV_VAL) => run_child(config),
        _ => setup_unshare(config),
    }
}

/// Set up the unshare externally.
fn setup_unshare(config: Config) -> Result<()> {
    eprintln!("Configuring unshare container.");
    let program = env::current_exe().expect("Determine executable name");
    let mut command = unshare::Command::new(program);
    command.args(child_command().as_ref());
    command.env(COMMAND_ENV_KEY, COMMAND_ENV_VAL);

    config.unshare(&mut command);

    if command.status()?.success() {
        Ok(())
    } else {
        Err(ErrorKind::UnshareExit.into())
    }
}

/// Run the command from inside the unshare.
fn run_child(config: Config) -> Result<()> {
    eprintln!("Configuring child command.");
    let child = child_command();
    let child_args: &[OsString] = child.as_ref();

    let mut command = process::Command::new(&child_args[0]);
    command.args(&child_args[1..]);
    command.env_remove(COMMAND_ENV_KEY);

    config.configure(&mut command);

    if command.status()?.success() {
        Ok(())
    } else {
        Err(ErrorKind::CommandExit.into())
    }
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

/// Find a path for an executable.
fn find_exec<P: AsRef<Path>>(executable: P) -> Option<PathBuf> {
    env::var_os("PATH")
        .as_ref()
        .and_then(|s| s.to_str())
        .and_then(|s| Some(s.split(':')))
        .and_then(|p| find_first(p, executable))
}

fn find_first<'p, I, P, E>(paths: I, executable: E) -> Option<PathBuf>
where
    I: Iterator<Item = P>,
    P: AsRef<Path>,
    E: AsRef<Path>,
{
    for prefix in paths {
        let path = prefix.as_ref().join(executable.as_ref());
        if path.exists() {
            return Some(path)
        }
    }

    None
}
