use std::fs::create_dir_all;
use std::path::{Path, PathBuf};

use nix::NixPath;
use nix::mount::{mount, umount, MsFlags};

// TODO: MS_LAZYATIME (not currently in libc)

use ::error::*;

/// A new mountpoint within a mount namespace.
///
/// Each process exists in a particular mount namespace which specifies which
/// *additional* mount mappings exist over the base file-system. This means that
/// if a set of processes exists in a separate mount namespace, they can have
/// directory mounts applied that are not visible to processes in any other
/// namespace. These processes are also unable to affect the mounts on external
/// namespaces.
///
/// This is simply a wrapper for `mount(2)` in Linux.
///
/// ```rust
/// DirMount::bind("/proc", "/tmp/jail/proc").read_only().mount();
/// ```
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "option")]
#[serde(rename_all = "snake_case")]
pub enum Mount {
    /// Create a new mount from `src` to `target`.
    ///
    /// The file system type must be explicitly provided as along with the
    /// target and the source.
    Mount {
        source: PathBuf,
        target: PathBuf,
        filesystem_type: PathBuf,
        #[serde(default)]
        flags: Vec<MountFlags>,
        #[serde(default)]
        make_target: bool,
    },
    /// Update the mount flags on an existing mount.
    Remount {
        target: PathBuf,
        #[serde(default)]
        flags: Vec<MountFlags>,
    },
    /// Update an existing mount point to be _shared_.
    ///
    /// This ensures that _mount_ and _unmount_ events that occur within the
    /// subtree of this mount point may propogate to peer mounts within the
    /// namespace.
    Shared {
        target: PathBuf,
        #[serde(default)]
        flags: Vec<MountFlags>,
    },
    /// Update an existing mount point to be _private_.
    ///
    /// This ensures that _mount_ and _unmount_ events that occur within the
    /// subtree of this mountpoint will not propogate to peer mounts within the
    /// namespace.
    Private {
        target: PathBuf,
        #[serde(default)]
        flags: Vec<MountFlags>,
    },
    /// Update an existing mount point to be a _slave_.
    ///
    /// This ensures that _mount_ and _unmount_ events never propogate out of
    /// the subtree from the mount point but events will propogate into it.
    Slave {
        target: PathBuf,
        #[serde(default)]
        flags: Vec<MountFlags>,
    },
    /// Update an existing mount point to be a _unbindable_.
    ///
    /// This has the same effect as [`Mount::private`](#method.provate) but
    /// also ensures the mount point, and its children, can't be mounted as a
    /// bind. Recursive bind mounts will simply have _unbindable_ mounts pruned.
    Unbindable {
        target: PathBuf,
        #[serde(default)]
        flags: Vec<MountFlags>,
    },
    /// Bind a directory to a new mount point.
    Bind {
        source: PathBuf,
        target: PathBuf,
        #[serde(default)]
        flags: Vec<MountFlags>,
        #[serde(default)]
        make_target: bool,
    },
    /// Bind a directory and all mounts in its subtree to a new mount point.
    RecursiveBind {
        source: PathBuf,
        target: PathBuf,
        #[serde(default)]
        flags: Vec<MountFlags>,
        #[serde(default)]
        make_target: bool,
    },
    /// Move a mount from an existing mount point to a new mount point.
    Relocate {
        source: PathBuf,
        target: PathBuf,
        #[serde(default)]
        flags: Vec<MountFlags>,
        #[serde(default)]
        make_target: bool,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MountFlags {
    /// This simply takes a non-bind mount and adds the bind flag.
    ///
    /// This is useful if remounting bind mounts.
    Bind,
    /// Make directory changes on this filesystem synchronous.
    SynchronousDirectories,
    /// Reduce on-disk updates of inode timestamps (atime, mtime, ctime) by
    /// maintaining these changes only in memory.  The on-disk timestamps are
    /// updated only when:
    ///
    /// * the inode needs to be updated for some change unrelated to file
    ///   timestamps;
    /// * the application employs fsync(2), syncfs(2), or sync(2);
    /// * an undeleted inode is evicted from memory; or
    /// * more than 24 hours have passed since the inode was written to disk.
    ///
    /// This mount option significantly reduces writes needed to update the
    /// inode's timestamps, especially mtime and atime.  However, in the event
    /// of a system crash, the atime  and mtime fields on disk might be out of
    /// date by up to 24 hours.
    ///
    /// Examples  of  workloads  where  this  option  could be of significant
    /// benefit include frequent random writes to preallocated files, as well as
    /// cases where the MS_STRICTATIME mount option is also enabled.
    #[cfg(not)]
    LazyAccessTime,
    /// Permit mandatory locking on files in this filesystem.
    MandatoryLock,
    /// Do not update access times for (all types of) files on this mount.
    NoAccessTime,
    /// Do not allow access to devices (special files) on this mount.
    NoDevices,
    /// Do not update access times for directories on this mount.
    NoDirectoryAccessTime,
    /// Do not allow programs to be executed from this mount.
    NoExecute,
    /// Do not honor set-user-ID and set-group-ID bits or file capabilities when
    /// executing programs from this mount.
    NoSuid,
    /// Mount read-only.
    ReadOnly,
    /// Update access time on files only if newer than the modification time.
    ///
    /// When a file on this mount is accessed, update the file's last
    /// access time (atime) only if the current value of atime is less than or
    /// equal to the file's last modification time (mtime) or last status change
    /// time (ctime).
    ///
    /// This option is useful for programs, such as mutt(1), that need to know
    /// when a file has been read since it was last modified.
    RelativeAccessTime,
    /// Suppress the display of certain warning messages in the kernel log.
    Silent,
    /// Always update the last access time.
    StrictAccessTime,
    /// Make writes on this mount synchronous.
    Synchronous,
}

impl Into<MsFlags> for MountFlags {
    fn into(self) -> MsFlags {
        match self {
            MountFlags::Bind                   => MsFlags::MS_BIND,
            MountFlags::SynchronousDirectories => MsFlags::MS_DIRSYNC,
            MountFlags::MandatoryLock          => MsFlags::MS_MANDLOCK,
            MountFlags::NoAccessTime           => MsFlags::MS_NOATIME,
            MountFlags::NoDevices              => MsFlags::MS_NODEV,
            MountFlags::NoDirectoryAccessTime  => MsFlags::MS_NODIRATIME,
            MountFlags::NoExecute              => MsFlags::MS_NOEXEC,
            MountFlags::NoSuid                 => MsFlags::MS_NOSUID,
            MountFlags::ReadOnly               => MsFlags::MS_RDONLY,
            MountFlags::RelativeAccessTime     => MsFlags::MS_RELATIME,
            MountFlags::Silent                 => MsFlags::MS_SILENT,
            MountFlags::StrictAccessTime       => MsFlags::MS_STRICTATIME,
            MountFlags::Synchronous            => MsFlags::MS_SYNCHRONOUS,
        }
    }
}


impl Mount {
    /// Create a new mount from `src` to `target`.
    ///
    /// The file system type must be explicitly provided as along with the
    /// target and the source.
    ///
    /// ```rust
    /// Mount::new("/dev/sda1", "/mnt", "ext4").mount();
    /// ```
    pub fn new<P: AsRef<Path>>(src: P, target: P, fstype: P) -> Mount {
        Mount::Mount {
            source: src.as_ref().to_owned(),
            target: target.as_ref().to_owned(),
            filesystem_type: fstype.as_ref().to_owned(),
            flags: Vec::new(),
            make_target: false,
        }
    }

    /// Update the mount flags on an existing mount.
    ///
    /// ```rust
    /// Mount::remount("/home").read_only().mount();
    /// ```
    pub fn remount<P: AsRef<Path>>(target: P) -> Mount {
        Mount::Remount {
            target: target.as_ref().to_owned(),
            flags: Vec::new(),
        }
    }

    /// Bind a directory to a new mount point.
    ///
    /// ```rust
    /// Mount::bind("/lib", "/tmp/jail/lib").mount();
    /// ```
    pub fn bind<P: AsRef<Path>>(src: P, target: P) -> Mount {
        Mount::Bind {
            source: src.as_ref().to_owned(),
            target: target.as_ref().to_owned(),
            flags: Vec::new(),
            make_target: false,
        }
    }


    /// Bind a directory and all mounts in its subtree to a new mount point.
    ///
    /// ```rust
    /// Mount::recursive_bind("/proc", "/tmp/jail/proc").mount();
    /// ```
    pub fn recursive_bind<P: AsRef<Path>>(src: P, target: P) -> Mount {
        Mount::RecursiveBind {
            source: src.as_ref().to_owned(),
            target: target.as_ref().to_owned(),
            flags: Vec::new(),
            make_target: false,
        }
    }

    /// Update an existing mount point to be _shared_.
    ///
    /// This ensures that _mount_ and _unmount_ events that occur within the
    /// subtree of this mount point may propogate to peer mounts within the
    /// namespace.
    pub fn shared<P: AsRef<Path>>(target: P) -> Mount {
        Mount::Shared {
            target: target.as_ref().to_owned(),
            flags: Vec::new(),
        }
    }


    /// Update an existing mount point to be _private_.
    ///
    /// This ensures that _mount_ and _unmount_ events that occur within the
    /// subtree of this mountpoint will not propogate to peer mounts within the
    /// namespace.
    pub fn private<P: AsRef<Path>>(target: P) -> Mount {
        Mount::Private {
            target: target.as_ref().to_owned(),
            flags: Vec::new(),
        }
    }

    /// Update an existing mount point to be a _slave_.
    ///
    /// This ensures that _mount_ and _unmount_ events never propogate out of
    /// the subtree from the mount point but events will propogate into it.
    pub fn slave<P: AsRef<Path>>(target: P) -> Mount {
        Mount::Slave {
            target: target.as_ref().to_owned(),
            flags: Vec::new(),
        }
    }

    /// Update an existing mount point to be a _unbindable_.
    ///
    /// This has the same effect as [`Mount::private`](#method.provate) but
    /// also ensures the mount point, and its children, can't be mounted as a
    /// bind. Recursive bind mounts will simply have _unbindable_ mounts pruned.
    pub fn unbindable<P: AsRef<Path>>(target: P) -> Mount {
        Mount::Unbindable {
            target: target.as_ref().to_owned(),
            flags: Vec::new(),
        }
    }

    /// Move a mount from an existing mount point to a new mount point.
    pub fn relocate<P: AsRef<Path>>(src: P, target: P) -> Mount {
        Mount::Relocate {
            source: src.as_ref().to_owned(),
            target: target.as_ref().to_owned(),
            flags: Vec::new(),
            make_target: false,
        }
    }
}

impl Mount {
    fn add_flag(mut self, flag: MountFlags) -> Mount {
        match &mut self {
            Mount::Mount         { flags, .. } => flags.push(flag),
            Mount::Remount       { flags, .. } => flags.push(flag),
            Mount::Shared        { flags, .. } => flags.push(flag),
            Mount::Private       { flags, .. } => flags.push(flag),
            Mount::Slave         { flags, .. } => flags.push(flag),
            Mount::Unbindable    { flags, .. } => flags.push(flag),
            Mount::Bind          { flags, .. } => flags.push(flag),
            Mount::RecursiveBind { flags, .. } => flags.push(flag),
            Mount::Relocate      { flags, .. } => flags.push(flag),
        };
        self
    }

    /// If the target directory does not exist, create it.
    pub fn make_target_dir(mut self) -> Mount {
        match self {
            Mount::Mount {
                source,
                target,
                filesystem_type,
                flags,
                ..
            } => Mount::Mount {
                source,
                target,
                filesystem_type,
                flags,
                make_target: true,
            },
            Mount::Bind {
                source,
                target,
                flags,
                ..
            } => Mount::Bind {
                make_target: true,
                source,
                target,
                flags,
            },
            Mount::RecursiveBind {
                source,
                target,
                flags,
                ..
            } => Mount::RecursiveBind {
                make_target: true,
                source,
                target,
                flags,
            },
            Mount::Relocate {
                source,
                target,
                flags,
                ..
            } => Mount::Relocate {
                make_target: true,
                source,
                target,
                flags,
            },
            _ => self,
        }
    }

    fn should_make_dir(&self) -> bool {
        match self {
            Mount::Mount         { make_target, .. } => *make_target,
            Mount::Bind          { make_target, .. } => *make_target,
            Mount::RecursiveBind { make_target, .. } => *make_target,
            Mount::Relocate      { make_target, .. } => *make_target,
            _ => false,
        }
    }

    fn flags(&self) -> MsFlags {
        let supplied = match self {
            Mount::Mount         { flags, .. } => flags,
            Mount::Remount       { flags, .. } => flags,
            Mount::Shared        { flags, .. } => flags,
            Mount::Private       { flags, .. } => flags,
            Mount::Slave         { flags, .. } => flags,
            Mount::Unbindable    { flags, .. } => flags,
            Mount::Bind          { flags, .. } => flags,
            Mount::RecursiveBind { flags, .. } => flags,
            Mount::Relocate      { flags, .. } => flags,
        };
        let default = match self {
            Mount::Mount         {..} => MsFlags::empty(),
            Mount::Remount       {..} => MsFlags::MS_REMOUNT,
            Mount::Shared        {..} => MsFlags::MS_SHARED,
            Mount::Private       {..} => MsFlags::MS_PRIVATE,
            Mount::Slave         {..} => MsFlags::MS_SLAVE,
            Mount::Unbindable    {..} => MsFlags::MS_UNBINDABLE,
            Mount::Bind          {..} => MsFlags::MS_BIND,
            Mount::RecursiveBind {..} => MsFlags::MS_BIND | MsFlags::MS_REC,
            Mount::Relocate      {..} => MsFlags::MS_MOVE,
        };

        let supplied: MsFlags = supplied.iter().map(|f| f.clone().into()).collect();
        supplied | default
    }

    fn target(&self) -> &Path {
        match self {
            Mount::Mount         { target, .. } => target.as_path(),
            Mount::Remount       { target, .. } => target.as_path(),
            Mount::Shared        { target, .. } => target.as_path(),
            Mount::Private       { target, .. } => target.as_path(),
            Mount::Slave         { target, .. } => target.as_path(),
            Mount::Unbindable    { target, .. } => target.as_path(),
            Mount::Bind          { target, .. } => target.as_path(),
            Mount::RecursiveBind { target, .. } => target.as_path(),
            Mount::Relocate      { target, .. } => target.as_path(),
        }
    }

    fn source(&self) -> Option<&Path> {
        match self {
            Mount::Mount         { source, .. } => Some(source.as_path()),
            Mount::Bind          { source, .. } => Some(source.as_path()),
            Mount::RecursiveBind { source, .. } => Some(source.as_path()),
            Mount::Relocate      { source, .. } => Some(source.as_path()),
            _ => None,
        }
    }

    fn filesystem_type(&self) -> Option<&Path> {
        match self {
            Mount::Mount { filesystem_type, .. } => Some(filesystem_type.as_path()),
            _ => None,
        }
    }
}

impl Mount {
    /// Mount using the given specification.
    pub fn mount(self) -> Result<()> {

        if self.should_make_dir() {
            create_dir_all(self.target())?;
        }

        let data: Option<&PathBuf> = None;

        mount(
            self.source(),
            self.target(),
            self.filesystem_type(),
            self.flags(),
            data
        )?;

        Ok(())
    }
}
