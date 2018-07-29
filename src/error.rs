#![allow(missing_docs)]
#![allow(renamed_and_removed_lints)]

//! Errors generated by isolate.
error_chain!{
    // Wrappers for other error_chains.
    links {
    }

    // Wrappers for other errors.
    foreign_links {
        Io(::std::io::Error);
        Nul(::std::ffi::NulError);
        Utf8(::std::str::Utf8Error);
        Nix(::nix::Error);
        Unshare(::unshare::Error);
    }

    // Internally defined errors.
    errors {
        UnshareExit(status: ::unshare::ExitStatus) {
            description("The unshared was unsuccessful")
        }
        CommandExit(status: ::std::process::ExitStatus) {
            description("The requested command was unsuccessful")
        }
        RelativeWorkingDir {
            description("Attempted to use relative working directory in chroot")
        }
        EnterChroot {
            description("Failed to enter chroot directory")
        }
        SetMount {
            description("Failed to set a mountpoint")
        }
        EnterWorkingDir {
            description("Failed to set working directory")
        }
        SetHostName {
            description("Failed to set the host name of the container")
        }
        SetUser {
            description("Failed to set user after configuring container")
        }
    }
}

pub type Failure = Result<()>;

macro_rules! ok { () => (Ok(())) }
