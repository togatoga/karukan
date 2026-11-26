//! Where ibus-daemon listens, found the way libibus finds it
//! (`ibus_get_address` / `ibus_get_socket_path` in ibusshare.c).

use std::path::PathBuf;

use anyhow::{Context, bail};

/// The daemon's D-Bus address: `IBUS_ADDRESS`, else the address file the
/// daemon wrote, as long as the pid recorded beside it is alive.
pub fn bus_address() -> anyhow::Result<String> {
    if let Some(address) = env("IBUS_ADDRESS") {
        return Ok(address);
    }
    let path = socket_path();
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("ibus-daemon address file {}", path.display()))?;
    let (address, pid) = parse_address_file(&contents);
    let (Some(address), Some(pid)) = (address, pid) else {
        bail!("no address in {}", path.display());
    };
    if !PathBuf::from(format!("/proc/{pid}")).exists() {
        bail!(
            "ibus-daemon (pid {pid}) from {} is not running",
            path.display()
        );
    }
    Ok(address)
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// `IBUS_ADDRESS_FILE`, else
/// `$XDG_CONFIG_HOME/ibus/bus/<machine-id>-<host>-<display>`.
fn socket_path() -> PathBuf {
    if let Some(path) = env("IBUS_ADDRESS_FILE") {
        return PathBuf::from(path);
    }
    let config = env("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env("HOME").unwrap_or_default()).join(".config"));
    let display = display_suffix(env("WAYLAND_DISPLAY").as_deref(), env("DISPLAY").as_deref());
    config
        .join("ibus")
        .join("bus")
        .join(format!("{}-{display}", machine_id()))
}

fn machine_id() -> String {
    ["/var/lib/dbus/machine-id", "/etc/machine-id"]
        .iter()
        .find_map(|p| std::fs::read_to_string(p).ok())
        .map(|id| id.trim().to_string())
        .unwrap_or_else(|| "machine-id".to_string())
}

/// `<host>-<display number>`: Wayland names the socket (`unix-wayland-0`),
/// X11 splits `host:number.screen` (`:1` → `unix-1`).
fn display_suffix(wayland: Option<&str>, x11: Option<&str>) -> String {
    if let Some(socket) = wayland {
        return format!("unix-{socket}");
    }
    let Some(display) = x11 else {
        return "unix-0".to_string();
    };
    let (host, rest) = display.split_once(':').unwrap_or((display, "0"));
    let number = rest.split('.').next().unwrap_or_default();
    let host = if host.is_empty() { "unix" } else { host };
    format!("{host}-{number}")
}

/// `IBUS_ADDRESS=` and `IBUS_DAEMON_PID=` from the address file.
fn parse_address_file(contents: &str) -> (Option<String>, Option<u32>) {
    let mut address = None;
    let mut pid = None;
    for line in contents.lines().filter(|l| !l.starts_with('#')) {
        if let Some(value) = line.strip_prefix("IBUS_ADDRESS=") {
            address = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("IBUS_DAEMON_PID=") {
            pid = value.trim().parse().ok();
        }
    }
    (address, pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_suffix_follows_libibus() {
        assert_eq!(
            display_suffix(Some("wayland-0"), Some(":0")),
            "unix-wayland-0"
        );
        assert_eq!(display_suffix(None, Some(":1")), "unix-1");
        assert_eq!(display_suffix(None, Some(":1.0")), "unix-1");
        assert_eq!(display_suffix(None, Some("host:2.1")), "host-2");
        assert_eq!(display_suffix(None, None), "unix-0");
    }

    #[test]
    fn address_file_is_parsed() {
        let contents = "# This file is created by ibus-daemon, please do not modify it.\n\
                        # This file allows processes on the machine to find the\n\
                        # ibus session bus with the below address.\n\
                        IBUS_ADDRESS=unix:path=/home/u/.cache/ibus/dbus-AbC,guid=0123\n\
                        IBUS_DAEMON_PID=4242\n";
        assert_eq!(
            parse_address_file(contents),
            (
                Some("unix:path=/home/u/.cache/ibus/dbus-AbC,guid=0123".to_string()),
                Some(4242)
            )
        );
        assert_eq!(parse_address_file("# only comments\n"), (None, None));
    }
}
