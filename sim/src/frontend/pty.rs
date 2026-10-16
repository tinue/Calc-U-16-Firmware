//! Pseudo-terminal exposing the emulated UART (the CP2102 USB-serial path).

use anyhow::{bail, Result};
use std::ffi::CStr;
use std::path::{Path, PathBuf};

pub struct Pty {
    master: libc::c_int,
    /// Kept open so the master does not see EOF/EIO when no client is attached.
    slave: libc::c_int,
    pub path: PathBuf,
    link: Option<PathBuf>,
}

impl Pty {
    pub fn open(link: Option<&Path>) -> Result<Pty> {
        let mut master = -1;
        let mut slave = -1;
        // SAFETY: openpty writes two fds; null name/termios/winsize are allowed.
        let rc = unsafe { libc::openpty(&mut master, &mut slave, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null_mut()) };
        if rc != 0 {
            bail!("openpty failed: {}", std::io::Error::last_os_error());
        }
        // SAFETY: valid fds from openpty; termios is plain data.
        unsafe {
            let mut tio: libc::termios = std::mem::zeroed();
            libc::tcgetattr(slave, &mut tio);
            libc::cfmakeraw(&mut tio);
            libc::cfsetspeed(&mut tio, libc::B38400);
            libc::tcsetattr(slave, libc::TCSANOW, &tio);
            let flags = libc::fcntl(master, libc::F_GETFL);
            libc::fcntl(master, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
        // SAFETY: ttyname returns a pointer to a static buffer or null.
        let name = unsafe { libc::ttyname(slave) };
        if name.is_null() {
            bail!("ttyname failed");
        }
        let path = PathBuf::from(unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned());
        let link = match link {
            Some(l) => {
                let _ = std::fs::remove_file(l);
                std::os::unix::fs::symlink(&path, l)?;
                Some(l.to_path_buf())
            }
            None => None,
        };
        Ok(Pty { master, slave, path, link })
    }

    /// Reads whatever the client has written (non-blocking).
    pub fn read(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buf = [0u8; 1024];
        loop {
            // SAFETY: buf is valid for buf.len() bytes.
            let n = unsafe { libc::read(self.master, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                break;
            }
            out.extend_from_slice(&buf[..n as usize]);
        }
        out
    }

    /// Writes bytes to the client; drops them if nobody reads (buffer full).
    pub fn write(&mut self, data: &[u8]) {
        let mut off = 0;
        while off < data.len() {
            // SAFETY: the slice is valid for its length.
            let n = unsafe { libc::write(self.master, data[off..].as_ptr().cast(), data.len() - off) };
            if n <= 0 {
                break;
            }
            off += n as usize;
        }
    }
}

impl Drop for Pty {
    fn drop(&mut self) {
        if let Some(l) = &self.link {
            let _ = std::fs::remove_file(l);
        }
        // SAFETY: fds owned by this struct.
        unsafe {
            libc::close(self.master);
            libc::close(self.slave);
        }
    }
}
