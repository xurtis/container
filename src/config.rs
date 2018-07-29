use std::env;
use std::path::{Path, PathBuf};
use std::process;

use libc::{uid_t, gid_t};
use unshare;
use nix::unistd::{chroot, sethostname, setuid, setgid, Uid, Gid};

use error::*;
use mount::Mount;

/// Configuration for the container.
#[derive(Debug, Default, Deserialize)]
pub struct Config {
    // Namespaces to unshare
    #[serde(default)]
    namespaces: Vec<Namespace>,

    // User namespace configuration
    #[serde(default)]
    uid: Option<uid_t>,
    #[serde(default)]
    gid: Option<gid_t>,
    #[serde(default)]
    uid_map: Vec<UidMap>,
    #[serde(default)]
    gid_map: Vec<GidMap>,

    // Mount configuration
    #[serde(default)]
    #[serde(rename = "mount")]
    mounts: Vec<Mount>,

    // Uts COnfiguration
    hostname: Option<String>,

    // Additional configuration
    chroot_dir: Option<PathBuf>,
    working_dir: Option<PathBuf>,
}

impl Config {
    /// Configure the container prior to the container.
    pub fn unshare(self, command: &mut unshare::Command) -> Failure {
        let uses_root = self.uses_root();

        let Config {
            namespaces,
            uid_map,
            gid_map,
            uid,
            gid,
            ..
        } = self;

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

        if uses_root {
            command.uid(0);
            command.gid(0);
        } else {
            if let Some(uid) = uid {
                command.uid(uid);
            }
            if let Some(gid) = gid {
                command.uid(gid);
            }
        }

        ok!()
    }

    /// Configure the container after having entered.
    pub fn configure(self, _command: &mut process::Command) -> Failure {
        let uses_root = self.uses_root();

        let Config {
            chroot_dir,
            working_dir,
            mounts,
            hostname,
            uid,
            gid,
            ..
        } = self;

        if let Some(hostname) = hostname {
            sethostname(&hostname).chain_err(|| ErrorKind::SetHostName)?;
        }

        for mount in mounts {
            mount.mount().chain_err(|| ErrorKind::SetMount)?;
        }

        if let Some(ref chroot_dir) = chroot_dir {
            chroot_dir.canonicalize()
                .map_err(Error::from)
                .and_then(|path| {
                    env::set_current_dir(&path)?;
                    Ok(path)
                })
                .and_then(|path| {
                    chroot(&path)?;
                    ok!()
                })
                .chain_err(|| ErrorKind::EnterChroot)?;
        }

        if let Some(working_dir) = working_dir {
            ensure!(
                working_dir.is_absolute() || chroot_dir.is_none(),
                ErrorKind::RelativeWorkingDir
            );
            env::set_current_dir(&working_dir)
                .chain_err(|| ErrorKind::EnterWorkingDir)?;
        }

        if uses_root {
            if let Some(gid) = gid {
                setgid(Gid::from_raw(gid))
                    .map_err(Error::from)
                    .chain_err(|| ErrorKind::SetUser)?;
            }
            if let Some(uid) = uid {
                setuid(Uid::from_raw(uid))
                    .chain_err(|| ErrorKind::SetUser)?;
            }
        }

        ok!()
    }

    /// The inner program needs to start as root.
    fn uses_root(&self) -> bool {
        self.hostname.is_some()
            || self.chroot_dir.is_some()
            || self.mounts.len() > 0
    }
}

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
