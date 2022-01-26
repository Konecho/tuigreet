use std::{
  env,
  error::Error,
  fs::{self, File},
  io::{self, BufRead, BufReader},
  path::{Path, PathBuf},
  process::Command,
};

use ini::Ini;
use nix::sys::utsname;

use crate::Greeter;

const X_SESSIONS: &str = "/usr/share/xsessions";
const WAYLAND_SESSIONS: &str = "/usr/share/wayland-sessions";
const LAST_USERNAME: &str = "/var/cache/tuigreet/lastuser";
const LAST_SESSION: &str = "/var/cache/tuigreet/lastsession";

pub fn get_hostname() -> String {
  utsname::uname().nodename().to_string()
}

pub fn get_issue() -> Option<String> {
  let vtnr: usize = env::var("XDG_VTNR").unwrap_or_else(|_| "0".to_string()).parse().expect("unable to parse VTNR");
  let uts = utsname::uname();

  if let Ok(issue) = fs::read_to_string("/etc/issue") {
    return Some(
      issue
        .replace("\\S", "Linux")
        .replace("\\l", &format!("tty{}", vtnr))
        .replace("\\s", uts.sysname())
        .replace("\\r", uts.release())
        .replace("\\v", uts.version())
        .replace("\\n", uts.nodename())
        .replace("\\m", uts.machine())
        .replace("\\\\", "\\"),
    );
  }

  None
}

pub fn get_last_username() -> Result<String, io::Error> {
  fs::read_to_string(LAST_USERNAME)
}

pub fn write_last_username(username: &str) {
  let _ = fs::write(LAST_USERNAME, username);
}

pub fn get_last_session() -> Result<String, io::Error> {
  fs::read_to_string(LAST_SESSION)
}

pub fn write_last_session(session: &str) {
  let _ = fs::write(LAST_SESSION, session);
}

pub fn get_last_user_session(username: &str) -> Result<String, io::Error> {
  fs::read_to_string(&format!("{}-{}", LAST_SESSION, username))
}

pub fn write_last_user_session(username: &str, session: &str) {
  let _ = fs::write(&format!("{}-{}", LAST_SESSION, username), session);
}

pub fn get_users() -> Vec<(String, Option<String>)> {
  match File::open("/etc/passwd") {
    Err(_) => vec![],
    Ok(file) => {
      let file = BufReader::new(file);
      let (uid_min, uid_max) = get_min_max_uids();

      let users: Vec<(String, Option<String>)> = file
        .lines()
        .filter_map(|line| {
          line
            .map(|line| {
              let mut split = line.splitn(7, ':');
              let username = split.next();
              let uid = split.nth(1);
              let name = split.nth(1);

              match uid.map(|uid| uid.parse::<u16>()) {
                Some(Ok(uid)) => match (username, name) {
                  (Some(username), Some("")) => Some((uid, username.to_string(), None)),
                  (Some(username), Some(name)) => Some((uid, username.to_string(), Some(name.to_string()))),
                  _ => None,
                },

                _ => None,
              }
            })
            .ok()
            .flatten()
            .filter(|(uid, _, _)| uid >= &uid_min && uid <= &uid_max)
            .map(|(_, username, name)| (username, name))
        })
        .collect();

      users
    }
  }
}

fn get_min_max_uids() -> (u16, u16) {
  let default = (1000, 60000);

  match File::open("/etc/login.defs") {
    Err(_) => default,
    Ok(file) => {
      let file = BufReader::new(file);

      let uids: (u16, u16) = file.lines().fold(default, |acc, line| {
        line
          .map(|line| {
            let mut tokens = line.split_whitespace();

            match (tokens.next(), tokens.next()) {
              (Some("UID_MIN"), Some(value)) => (value.parse::<u16>().unwrap_or(acc.0), acc.1),
              (Some("UID_MAX"), Some(value)) => (acc.0, value.parse::<u16>().unwrap_or(acc.1)),
              _ => acc,
            }
          })
          .unwrap_or(acc)
      });

      uids
    }
  }
}

pub fn get_sessions(greeter: &Greeter) -> Result<Vec<(String, String)>, Box<dyn Error>> {
  let sessions = match greeter.sessions_path {
    Some(ref dirs) => env::split_paths(&dirs).collect(),
    None => vec![PathBuf::from(X_SESSIONS), PathBuf::from(WAYLAND_SESSIONS)],
  };

  let mut files = sessions
    .iter()
    .flat_map(fs::read_dir)
    .flat_map(|directory| directory.flat_map(|entry| entry.map(|entry| load_desktop_file(entry.path()))).flatten())
    .collect::<Vec<_>>();

  if let Some(command) = &greeter.command {
    files.insert(0, (command.clone(), command.clone()));
  }

  Ok(files)
}

fn load_desktop_file<P>(path: P) -> Result<(String, String), Box<dyn Error>>
where
  P: AsRef<Path>,
{
  let desktop = Ini::load_from_file(path)?;
  let section = desktop.section(Some("Desktop Entry")).ok_or("no Desktop Entry section in desktop file")?;

  let name = section.get("Name").ok_or("no Name property in desktop file")?;
  let exec = section.get("Exec").ok_or("no Exec property in desktop file")?;

  Ok((name.to_string(), exec.to_string()))
}

pub fn capslock_status() -> bool {
  let mut command = Command::new("kbdinfo");
  command.args(["gkbled", "capslock"]);

  match command.output() {
    Ok(output) => output.status.code() == Some(0),
    Err(_) => false,
  }
}
