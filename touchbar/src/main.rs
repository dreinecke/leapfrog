mod draw;
mod state;
#[allow(dead_code, unused_imports)]
mod t1bridge;
mod theme;
mod ui;

use std::{
    collections::{HashMap, HashSet},
    env, fs, io,
    os::{fd::RawFd, unix::fs::symlink},
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use draw::{RenderedUi, render, save_png};
use state::{TouchIdState, VisualState, home_dir, load_font, read_touch_id_state};
use t1bridge::{Brightness, Client, Event, InputFrame, Key};
use ui::{Action, Button, action_at};

const PREVIEW_WIDTH: u32 = 2170;
const PREVIEW_HEIGHT: u32 = 60;

struct App {
    visual: VisualState,
    font: fontdue::Font,
    touch_id: Option<TouchIdState>,
    sticky_fn: bool,
    physical_fn: bool,
    pressed: HashMap<u8, Action>,
    buttons: Vec<Button>,
}

impl App {
    fn new(width: u32, height: u32) -> Result<Self> {
        Ok(Self {
            visual: VisualState::load(width, height)?,
            font: load_font()?,
            touch_id: read_touch_id_state(),
            sticky_fn: false,
            physical_fn: false,
            pressed: HashMap::new(),
            buttons: Vec::new(),
        })
    }

    fn fn_mode(&self) -> bool {
        self.sticky_fn || self.physical_fn
    }

    fn rendered(&self) -> RenderedUi {
        let pressed: Vec<_> = self.pressed.values().copied().collect();
        render(
            &self.visual,
            &self.font,
            self.fn_mode(),
            self.touch_id.as_ref(),
            &pressed,
        )
    }

    fn refresh_touch_id(&mut self) -> bool {
        let next = read_touch_id_state();
        if next == self.touch_id {
            false
        } else {
            self.touch_id = next;
            self.pressed.clear();
            true
        }
    }

    fn handle_input(&mut self, input: InputFrame) -> (Vec<Action>, bool) {
        let mut changed = false;
        if self.physical_fn != input.fn_pressed {
            self.physical_fn = input.fn_pressed;
            self.pressed.clear();
            changed = true;
        }

        let active_ids: HashSet<u8> = input
            .contacts
            .iter()
            .filter(|contact| contact.tip)
            .map(|contact| contact.id)
            .collect();
        let mut actions = Vec::new();

        for contact in &input.contacts {
            if contact.tip {
                if !self.pressed.contains_key(&contact.id)
                    && let Some(action) = action_at(&self.buttons, contact.x, contact.y)
                {
                    self.pressed.insert(contact.id, action);
                    changed = true;
                }
            } else if let Some(action) = self.pressed.remove(&contact.id) {
                if action_at(&self.buttons, contact.x, contact.y) == Some(action) {
                    actions.push(action);
                }
                changed = true;
            }
        }

        let released: Vec<u8> = self
            .pressed
            .keys()
            .filter(|id| !active_ids.contains(id))
            .copied()
            .collect();
        for id in released {
            if let Some(action) = self.pressed.remove(&id) {
                actions.push(action);
                changed = true;
            }
        }
        (actions, changed)
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None | Some("run") => run_renderer(),
        Some("preview") => preview(args.get(1).map(PathBuf::from)),
        Some("select") => select_renderer(),
        Some("stock") => stock_renderer(true),
        Some(other) => bail!("unknown command: {other} (use run, preview, select, or stock)"),
    }
}

fn run_renderer() -> Result<()> {
    let mut client = Client::connect().context("connect to T1Bridge hardware service")?;
    let dimensions = client.dimensions();
    let mut app = App::new(dimensions.width, dimensions.height)?;
    let mut dirty = true;
    let mut next_system_refresh = Instant::now();
    let mut next_touch_id_refresh = Instant::now();

    loop {
        if dirty && client.frame_available() {
            let rendered = app.rendered();
            app.buttons = rendered.buttons;
            if client.submit_rgba(&rendered.pixels, dimensions.width, dimensions.height)? {
                dirty = false;
            }
        }

        poll_readable(client.raw_fd(), Duration::from_millis(50))?;
        while let Some(event) = client.receive()? {
            match event {
                Event::Input(input) => {
                    let (actions, input_changed) = app.handle_input(input);
                    dirty |= input_changed;
                    for action in actions {
                        if perform_action(action, &mut app, &mut client)? {
                            stock_renderer(false)?;
                            return Ok(());
                        }
                        dirty = true;
                    }
                }
                Event::FrameAvailable => {}
                Event::ActionRejected { action, error } => {
                    eprintln!("T1Bridge rejected {action:?}: {error:?}");
                }
            }
        }

        let now = Instant::now();
        if now >= next_touch_id_refresh {
            dirty |= app.refresh_touch_id();
            next_touch_id_refresh = now + Duration::from_millis(100);
        }
        if now >= next_system_refresh {
            dirty |= app.visual.refresh();
            next_system_refresh = now + Duration::from_millis(900);
        }
    }
}

fn perform_action(action: Action, app: &mut App, client: &mut Client) -> Result<bool> {
    match action {
        Action::Escape => client.tap_keys(&[Key::Escape])?,
        Action::Function(number) => client.tap_keys(&[function_key(number)?])?,
        Action::MediaPrevious => run_quiet("playerctl", &["previous"]),
        Action::MediaPlayPause => run_quiet("playerctl", &["play-pause"]),
        Action::MediaNext => run_quiet("playerctl", &["next"]),
        Action::BrightnessDown | Action::BrightnessUp => {
            let current = app.visual.snapshot.brightness;
            let target = if action == Action::BrightnessDown {
                current.saturating_sub(10).max(5)
            } else {
                current.saturating_add(10).min(100)
            };
            if client.capabilities().display_brightness {
                client.set_brightness(Brightness::Display, target)?;
            } else {
                let value = format!("{target}%");
                run_quiet("brightnessctl", &["set", &value]);
            }
            app.visual.snapshot.brightness = target;
        }
        Action::ToggleMute => {
            run_quiet("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]);
            app.visual.snapshot.audio_muted = !app.visual.snapshot.audio_muted;
        }
        Action::VolumeDown => {
            run_quiet(
                "wpctl",
                &["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", "5%-"],
            );
            app.visual.snapshot.volume = app.visual.snapshot.volume.saturating_sub(5);
        }
        Action::VolumeUp => {
            run_quiet(
                "wpctl",
                &["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", "5%+"],
            );
            app.visual.snapshot.volume = app.visual.snapshot.volume.saturating_add(5).min(100);
        }
        Action::FnToggle => app.sticky_fn = !app.sticky_fn,
        Action::CancelTouchId => client.cancel_touch_id()?,
        Action::StockRenderer => return Ok(true),
    }
    Ok(false)
}

fn function_key(number: u8) -> Result<Key> {
    Ok(match number {
        1 => Key::F1,
        2 => Key::F2,
        3 => Key::F3,
        4 => Key::F4,
        5 => Key::F5,
        6 => Key::F6,
        7 => Key::F7,
        8 => Key::F8,
        9 => Key::F9,
        10 => Key::F10,
        11 => Key::F11,
        12 => Key::F12,
        _ => bail!("invalid function key F{number}"),
    })
}

fn run_quiet(program: &str, args: &[&str]) {
    if let Err(error) = Command::new(program).args(args).status() {
        eprintln!("could not run {program}: {error}");
    }
}

fn poll_readable(fd: RawFd, timeout: Duration) -> io::Result<()> {
    let mut descriptor = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let timeout_ms = timeout.as_millis().min(i32::MAX as u128) as i32;
    // SAFETY: descriptor points to one initialized pollfd for the duration of the call.
    let result = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
    if result < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn preview(path: Option<PathBuf>) -> Result<()> {
    let output = path.unwrap_or_else(|| PathBuf::from("/tmp/leapfrog-touchbar-preview.png"));
    let mut app = App::new(PREVIEW_WIDTH, PREVIEW_HEIGHT)?;
    // A design preview should show the normal controls even if a real
    // authentication request happens to be active while it is generated.
    app.touch_id = None;
    let rendered = app.rendered();
    save_png(&output, PREVIEW_WIDTH, PREVIEW_HEIGHT, rendered.pixels)?;
    println!("{}", output.display());
    Ok(())
}

fn select_renderer() -> Result<()> {
    let executable = env::current_exe().context("locate renderer executable")?;
    let config = home_dir()?.join(".config/t1bridge");
    fs::create_dir_all(&config).context("create T1Bridge config directory")?;
    let renderer = config.join("renderer");
    let previous = config.join("renderer.previous");

    if let Ok(target) = fs::read_link(&renderer)
        && target == executable
    {
        restart_touchbar()?;
        return Ok(());
    }
    if fs::symlink_metadata(&renderer).is_ok() {
        if fs::symlink_metadata(&previous).is_ok() {
            bail!(
                "refusing to replace both {} and {}",
                renderer.display(),
                previous.display()
            );
        }
        fs::rename(&renderer, &previous).context("preserve previous Touch Bar renderer")?;
    }

    let temporary = config.join(format!(".renderer.{}", std::process::id()));
    let _ = fs::remove_file(&temporary);
    symlink(&executable, &temporary).context("create renderer selection")?;
    fs::rename(&temporary, &renderer).context("activate renderer selection")?;
    restart_touchbar()?;
    println!("Selected {}", executable.display());
    Ok(())
}

fn stock_renderer(restart: bool) -> Result<()> {
    let config = home_dir()?.join(".config/t1bridge");
    let renderer = config.join("renderer");
    let previous = config.join("renderer.previous");
    if fs::symlink_metadata(&renderer).is_ok() {
        fs::remove_file(&renderer).context("remove custom renderer selection")?;
    }
    if fs::symlink_metadata(&previous).is_ok() {
        fs::rename(&previous, &renderer).context("restore previous renderer selection")?;
    }
    if restart {
        restart_touchbar()?;
    }
    Ok(())
}

fn restart_touchbar() -> Result<()> {
    let status = Command::new("systemctl")
        .args(["--user", "restart", "t1-touchbar.service"])
        .status()
        .context("restart t1-touchbar.service")?;
    if !status.success() {
        bail!("systemctl could not restart t1-touchbar.service");
    }
    Ok(())
}
