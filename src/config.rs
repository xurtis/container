use std::env;
use std::path::{Path, PathBuf};
use std::process;

use libc::{uid_t, gid_t};
use unshare;
use nix::unistd::chroot;

use error::*;
use mount::Mount;

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
pub struct Config {
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
    pub fn unshare(self, command: &mut unshare::Command) -> Failure {
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

        ok!()
    }

    /// Configure the container after having entered.
    pub fn configure(self, _command: &mut process::Command) -> Failure {
        let Config { chroot_dir, mount, .. } = self;

        for mount in mount {
            mount.mount()?;
        }

        if let Some(chroot_dir) = chroot_dir {
            let full_path = chroot_dir.canonicalize()?;
            env::set_current_dir(&full_path)?;
            chroot(&full_path)?;
        }

        ok!()
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
