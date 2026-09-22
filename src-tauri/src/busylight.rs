//! The desk light, driven from the timer that already knows the answer.
//!
//! This replaces a separate Python menu bar app. It was separate because the
//! light started life as its own toy, but the light's state is not its own: it
//! is a rendering of whether you are in a focus session or a meeting, which is
//! something Tasks already tracks. Two programs holding the same fact is how
//! they come to disagree.
//!
//! The device is a Raspberry Pi Pico speaking a handful of words over a serial
//! port — BUSY, AVAILABLE, RINGING, OFFLINE, OFF — and the firmware on it is
//! unchanged.
//!
//! What is new is that the connection looks after itself. The old app connected
//! once at launch and, on any serial error, set its handle to None and never
//! tried again; unplugging the device left the menu bar showing a stale colour
//! until somebody noticed and clicked Auto Detect. Here a supervisor thread
//! reconnects on its own and re-asserts the colour when the device comes back,
//! because a Pico that has just been plugged in has forgotten everything.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// What the firmware understands.
const BAUD: u32 = 115_200;
/// How long to wait for the device to answer before giving up on a write.
const IO_TIMEOUT: Duration = Duration::from_millis(300);
/// How long to let the Pico boot after opening the port.
///
/// Opening a serial port asserts DTR, which resets the board. Anything sent
/// during that window lands in a device that is still starting up and is
/// simply lost — which looks exactly like a lamp that ignores you.
const BOOT_GRACE: Duration = Duration::from_millis(2_500);
/// How often the supervisor looks in on things.
const SUPERVISE_EVERY: Duration = Duration::from_secs(3);

/// The colour the light is showing, in the firmware's own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Busy,
    Available,
    Ringing,
    Offline,
    Off,
}

impl Mode {
    pub fn command(self) -> &'static str {
        match self {
            Mode::Busy => "BUSY",
            Mode::Available => "AVAILABLE",
            Mode::Ringing => "RINGING",
            Mode::Offline => "OFFLINE",
            Mode::Off => "OFF",
        }
    }

    /// The menu bar dot, matching what the Python app showed so the thing you
    /// glance at does not change under you.
    pub fn dot(self) -> &'static str {
        match self {
            Mode::Busy => "🔴",
            Mode::Available => "🟢",
            Mode::Ringing => "🟠",
            Mode::Offline => "⚪",
            Mode::Off => "⚫",
        }
    }

    pub fn parse(word: &str) -> Option<Mode> {
        match word.trim().to_ascii_uppercase().as_str() {
            "BUSY" => Some(Mode::Busy),
            "AVAILABLE" => Some(Mode::Available),
            "RINGING" => Some(Mode::Ringing),
            "OFFLINE" => Some(Mode::Offline),
            "OFF" => Some(Mode::Off),
            _ => None,
        }
    }
}

/// What the rest of the app can see about the light.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LightStatus {
    pub connected: bool,
    /// The port in use, or the one being looked for.
    pub port: Option<String>,
    pub mode: Option<Mode>,
    /// Plain words for the menu, e.g. "Connected to /dev/cu.usbmodem1201".
    pub message: String,
    /// Whether the light follows the timer on its own.
    pub follow_timer: bool,
    pub ports: Vec<String>,
}

/// The dot for the menu bar, including the states that are not a colour.
pub fn dot_for(status: &LightStatus) -> &'static str {
    if !status.connected {
        return "◌";
    }
    status.mode.map(Mode::dot).unwrap_or("🔵")
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LightSettings {
    /// Port to prefer. None means "find one".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    /// Whether starting a session changes the colour.
    #[serde(default = "yes")]
    pub follow_timer: bool,
    /// Whether to look for a device at all. Off by default: most people do not
    /// have one, and an app that scans serial ports uninvited is rude.
    #[serde(default)]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

pub struct Light {
    settings: Mutex<LightSettings>,
    port: Mutex<Option<Box<dyn serialport::SerialPort>>>,
    /// The colour that *should* be showing, so it can be restored after a
    /// reconnect. The device forgets; this does not.
    wanted: Mutex<Option<Mode>>,
    connected: AtomicBool,
    message: Mutex<String>,
    supervising: AtomicBool,
}

impl Default for Light {
    fn default() -> Self {
        Light {
            settings: Mutex::new(load_settings()),
            port: Mutex::new(None),
            wanted: Mutex::new(None),
            connected: AtomicBool::new(false),
            message: Mutex::new("Not connected".into()),
            supervising: AtomicBool::new(false),
        }
    }
}

impl Light {
    pub fn status(&self) -> LightStatus {
        let settings = self.settings.lock().expect("light settings");
        LightStatus {
            connected: self.connected.load(Ordering::Relaxed),
            port: settings.port.clone(),
            mode: *self.wanted.lock().expect("light mode"),
            message: self.message.lock().expect("light message").clone(),
            follow_timer: settings.follow_timer,
            ports: available_ports(),
        }
    }

    fn say(&self, message: impl Into<String>) {
        *self.message.lock().expect("light message") = message.into();
    }

    /// Ask for a colour. Remembered whether or not the device is there, so
    /// plugging it in later shows the right thing rather than the last thing.
    pub fn set_mode(&self, mode: Mode) {
        *self.wanted.lock().expect("light mode") = Some(mode);
        self.write(mode);
    }

    fn write(&self, mode: Mode) {
        let mut slot = self.port.lock().expect("light port");
        let Some(port) = slot.as_mut() else { return };
        let line = format!("{}\n", mode.command());
        if port.write_all(line.as_bytes()).and_then(|_| port.flush()).is_err() {
            // A write that fails means the device has gone. Drop it and let
            // the supervisor find it again rather than retrying in place.
            *slot = None;
            self.connected.store(false, Ordering::Relaxed);
            self.say("Lost the connection; looking again");
            return;
        }
        // Read what it says back, both to confirm and to empty the buffer.
        // The firmware volunteers lines nobody asked for — PING answers with
        // PONG *and* a STATE line — so replies cannot be counted one per
        // command; they have to be read as a stream and matched.
        let expected = format!("OK {}", mode.command());
        let acknowledged = read_lines_until(&mut **port, Duration::from_millis(400), |line| {
            line.eq_ignore_ascii_case(&expected)
        });
        if acknowledged {
            self.say(format!("Showing {}", mode.command().to_lowercase()));
        }
    }

    fn connect(&self, preferred: Option<&str>) -> bool {
        // An explicit choice is honoured whatever it looks like; a search only
        // considers things that could plausibly be the device.
        let candidates: Vec<String> = match preferred {
            Some(port) => vec![port.to_string()],
            None => detectable_ports(),
        };
        for candidate in candidates {
            let opened = serialport::new(&candidate, BAUD).timeout(IO_TIMEOUT).open();
            let Ok(mut port) = opened else { continue };

            if !handshake(&mut *port) {
                // Something else is on this port. Leave it alone.
                continue;
            }

            *self.port.lock().expect("light port") = Some(port);
            self.connected.store(true, Ordering::Relaxed);
            self.say(format!("Connected to {candidate}"));
            self.settings.lock().expect("light settings").port = Some(candidate);
            save_settings(&self.settings.lock().expect("light settings").clone());

            // The device has just booted and shows whatever its firmware
            // starts with, so tell it what we already decided.
            if let Some(mode) = *self.wanted.lock().expect("light mode") {
                self.write(mode);
            }
            return true;
        }
        false
    }

    /// Try to connect right now rather than waiting for the supervisor.
    pub fn connect_now(&self) {
        let preferred = self.settings().port;
        if !self.connect(preferred.as_deref()) {
            self.connect(None);
        }
        if !self.connected.load(Ordering::Relaxed) {
            self.say("No device found");
        }
    }

    pub fn settings(&self) -> LightSettings {
        self.settings.lock().expect("light settings").clone()
    }

    pub fn update_settings(&self, next: LightSettings) {
        let was_enabled = self.settings.lock().expect("light settings").enabled;
        *self.settings.lock().expect("light settings") = next.clone();
        save_settings(&next);
        if !next.enabled && was_enabled {
            *self.port.lock().expect("light port") = None;
            self.connected.store(false, Ordering::Relaxed);
            self.say("Off");
        }
    }
}

/// Serial ports a person might reasonably pick, for the settings list.
///
/// `/dev/tty.*` is left out on purpose. macOS exposes every serial device
/// twice, and opening the `tty` half blocks until the line asserts carrier —
/// which for a Pico is never. It is a hang, not an error, so it cannot even be
/// reported.
pub fn available_ports() -> Vec<String> {
    let mut ports: Vec<String> = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|port| port.port_name)
        .filter(|name| !name.starts_with("/dev/tty."))
        .collect();
    ports.sort_by_key(|name| !looks_like_a_device(name));
    ports
}

/// Ports worth trying when nobody has said which one.
///
/// Only USB serial adapters. A Mac's other ports are a debug console and
/// whatever Bluetooth audio is paired — this machine offers `cu.JBLWAVEBUDS` —
/// and probing a pair of earbuds looking for a desk lamp is both useless and
/// slow enough to notice.
pub fn detectable_ports() -> Vec<String> {
    available_ports().into_iter().filter(|name| looks_like_a_device(name)).collect()
}

fn looks_like_a_device(name: &str) -> bool {
    name.contains("usbmodem") || name.contains("usbserial")
}

/// Keep the connection alive, for as long as the app runs.
pub fn supervise(light: Arc<Light>) {
    if light.supervising.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || loop {
        std::thread::sleep(SUPERVISE_EVERY);
        let settings = light.settings();
        if !settings.enabled {
            continue;
        }
        if light.connected.load(Ordering::Relaxed) {
            continue;
        }
        // Try the remembered port first, then anything else that looks right.
        // This is the whole difference from the app this replaces: it keeps
        // trying, so unplugging the device is a temporary state rather than a
        // permanent one needing a human.
        if !light.connect(settings.port.as_deref()) && settings.port.is_some() {
            light.connect(None);
        }
    });
}

/// The id of the light's own menu bar item.
///
/// A second tray rather than a corner of the timer's: the dot is what you
/// glance at to answer "is the light actually on", and burying it beside a
/// countdown would lose exactly the thing that made the old app useful.
pub const TRAY_ID: &str = "busylight";

/// Build the light's menu: the five colours, then where it is connected.
pub fn tray_menu<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> tauri::Result<tauri::menu::Menu<R>> {
    use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
    use tauri::Manager;

    let status = app
        .try_state::<crate::AppState>()
        .map(|state| state.light.status());
    let current = status.as_ref().and_then(|s| s.mode);
    let follow = status.as_ref().map(|s| s.follow_timer).unwrap_or(true);

    let colour = |id: &str, label: &str, mode: Mode| {
        CheckMenuItem::with_id(app, id, label, true, current == Some(mode), None::<&str>)
    };

    Menu::with_items(
        app,
        &[
            &colour("light-busy", "Busy", Mode::Busy)?,
            &colour("light-available", "Available", Mode::Available)?,
            &colour("light-ringing", "Ringing", Mode::Ringing)?,
            &colour("light-offline", "Offline", Mode::Offline)?,
            &colour("light-off", "Off", Mode::Off)?,
            &PredefinedMenuItem::separator(app)?,
            &CheckMenuItem::with_id(
                app,
                "light-follow",
                "Follow the timer",
                true,
                follow,
                None::<&str>,
            )?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(
                app,
                "light-status",
                status
                    .as_ref()
                    .map(|s| s.message.clone())
                    .unwrap_or_else(|| "Not connected".into()),
                false,
                None::<&str>,
            )?,
            &MenuItem::with_id(app, "light-reconnect", "Look for the device", true, None::<&str>)?,
        ],
    )
}

/// Act on a click in the light's menu. Returns whether the id was one of ours.
pub fn handle_menu<R: tauri::Runtime>(app: &tauri::AppHandle<R>, id: &str) -> bool {
    use tauri::Manager;
    let Some(state) = app.try_state::<crate::AppState>() else { return false };
    let light = &state.light;

    let mode = match id {
        "light-busy" => Some(Mode::Busy),
        "light-available" => Some(Mode::Available),
        "light-ringing" => Some(Mode::Ringing),
        "light-offline" => Some(Mode::Offline),
        "light-off" => Some(Mode::Off),
        _ => None,
    };
    if let Some(mode) = mode {
        // Setting a colour by hand means you want that colour, so stop the
        // timer overwriting it a second later.
        let mut settings = light.settings();
        if settings.follow_timer {
            settings.follow_timer = false;
            light.update_settings(settings);
        }
        light.set_mode(mode);
    } else {
        match id {
            "light-follow" => {
                let mut settings = light.settings();
                settings.follow_timer = !settings.follow_timer;
                light.update_settings(settings);
            }
            "light-reconnect" => {
                let mut settings = light.settings();
                settings.enabled = true;
                light.update_settings(settings);
                // Forget the remembered port so a device on a new one is found.
                light.connect_now();
            }
            _ => return false,
        }
    }

    push_light_tray(app);
    if let Ok(menu) = tray_menu(app) {
        if let Some(tray) = app.tray_by_id(TRAY_ID) {
            let _ = tray.set_menu(Some(menu));
        }
    }
    true
}

/// Redraw the light's menu bar dot.
pub fn push_light_tray<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    let Some(state) = app.try_state::<crate::AppState>() else { return };
    let status = state.light.status();
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_title(Some(dot_for(&status)));
        let _ = tray.set_tooltip(Some(&status.message));
    }
}

/// Establish that the thing on this port is actually the lamp.
///
/// Opening the port asserts DTR, which resets the board, so it usually
/// announces itself — but only if it *did* reset. A board already running
/// says nothing at all, and waiting for a banner that will never come was
/// enough to make a working device look absent.
///
/// So: listen briefly for the banner, and if none comes, ask. PING is answered
/// with PONG whatever state the firmware is in.
fn handshake(port: &mut dyn serialport::SerialPort) -> bool {
    use std::io::Write;

    let announced = read_lines_until(port, BOOT_GRACE, |line| {
        line.eq_ignore_ascii_case("READY") || line.starts_with("STATE:")
    });
    if announced {
        return true;
    }
    if port.write_all(b"PING\n").and_then(|_| port.flush()).is_err() {
        return false;
    }
    read_lines_until(port, Duration::from_millis(900), |line| {
        line.eq_ignore_ascii_case("PONG")
    })
}

/// Read lines until one satisfies `wanted`, or the deadline passes.
///
/// Returns whether it was seen. Everything else is discarded, which is the
/// point as much as the matching is: unread bytes pile up in the OS buffer
/// over a working day and eventually get in the way.
fn read_lines_until(
    port: &mut dyn serialport::SerialPort,
    within: Duration,
    wanted: impl Fn(&str) -> bool,
) -> bool {
    use std::io::Read;
    let deadline = std::time::Instant::now() + within;
    let mut pending = String::new();
    let mut chunk = [0u8; 256];

    while std::time::Instant::now() < deadline {
        match port.read(&mut chunk) {
            Ok(0) => std::thread::sleep(Duration::from_millis(20)),
            Ok(read) => {
                pending.push_str(&String::from_utf8_lossy(&chunk[..read]));
                while let Some(at) = pending.find('\n') {
                    let line: String = pending.drain(..=at).collect();
                    if wanted(line.trim()) {
                        return true;
                    }
                }
                // A device spewing without newlines must not grow this forever.
                if pending.len() > 4096 {
                    pending.clear();
                }
            }
            // A timeout is normal: it means nothing more has arrived yet.
            Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {}
            Err(_) => return false,
        }
    }
    false
}

fn settings_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(std::path::PathBuf::from(home).join(".intentio").join("busylight.json"))
}

fn load_settings() -> LightSettings {
    settings_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn save_settings(settings: &LightSettings) {
    let Some(path) = settings_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(path, format!("{json}\n"));
    }
}

/// The colour a timer state calls for.
///
/// Idle is green rather than off: the point of the light is that somebody
/// walking up can tell, and an unlit lamp says "broken" as readily as "free".
pub fn mode_for(running: bool, paused: bool, kind: int_tasks_core::SessionKind) -> Mode {
    use int_tasks_core::SessionKind;
    if !running || paused {
        return Mode::Available;
    }
    match kind {
        SessionKind::Focus | SessionKind::Meeting => Mode::Busy,
        SessionKind::Break => Mode::Available,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use int_tasks_core::SessionKind;

    #[test]
    fn focus_and_meetings_both_mean_do_not_disturb() {
        assert_eq!(mode_for(true, false, SessionKind::Focus), Mode::Busy);
        assert_eq!(mode_for(true, false, SessionKind::Meeting), Mode::Busy);
    }

    #[test]
    fn a_break_is_not_busy() {
        assert_eq!(mode_for(true, false, SessionKind::Break), Mode::Available);
    }

    #[test]
    fn paused_is_not_busy_either() {
        // Stepping away mid-session is exactly when somebody may interrupt.
        assert_eq!(mode_for(true, true, SessionKind::Focus), Mode::Available);
    }

    #[test]
    fn idle_is_green_rather_than_dark() {
        assert_eq!(mode_for(false, false, SessionKind::Focus), Mode::Available);
    }

    #[test]
    fn the_dots_match_the_app_this_replaces() {
        assert_eq!(Mode::Busy.dot(), "🔴");
        assert_eq!(Mode::Available.dot(), "🟢");
        assert_eq!(Mode::Ringing.dot(), "🟠");
    }

    #[test]
    fn a_disconnected_light_shows_that_it_is_disconnected() {
        let status = LightStatus {
            connected: false,
            port: None,
            mode: Some(Mode::Busy),
            message: String::new(),
            follow_timer: true,
            ports: vec![],
        };
        // Not the last colour it was: that would be a lie to anyone looking.
        assert_eq!(dot_for(&status), "◌");
    }

    #[test]
    fn the_tty_half_of_every_device_is_ignored() {
        // Opening /dev/tty.usbmodem* blocks forever waiting for carrier, so it
        // must never appear as a candidate.
        for port in available_ports() {
            assert!(!port.starts_with("/dev/tty."), "{port} would hang on open");
        }
    }

    #[test]
    fn auto_detect_only_considers_usb_serial_devices() {
        for port in detectable_ports() {
            assert!(
                looks_like_a_device(&port),
                "{port} is not a USB serial device and should not be probed"
            );
        }
    }

    #[test]
    fn firmware_words_round_trip() {
        for mode in [Mode::Busy, Mode::Available, Mode::Ringing, Mode::Offline, Mode::Off] {
            assert_eq!(Mode::parse(mode.command()), Some(mode));
        }
        assert_eq!(Mode::parse("nonsense"), None);
    }
}
