#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Action {
    Escape,
    Function(u8),
    MediaPrevious,
    MediaPlayPause,
    MediaNext,
    BrightnessDown,
    BrightnessUp,
    ToggleMute,
    VolumeDown,
    VolumeUp,
    FnToggle,
    CancelTouchId,
    StockRenderer,
}

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub fn contains(self, x: u32, y: u32) -> bool {
        x >= self.x
            && x < self.x.saturating_add(self.width)
            && y >= self.y
            && y < self.y.saturating_add(self.height)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tone {
    Accent,
    Info,
    Media,
    Brightness,
    Audio,
    Danger,
}

#[derive(Clone, Debug)]
pub struct Button {
    pub action: Option<Action>,
    pub rect: Rect,
    pub label: String,
    pub icon: bool,
    pub tone: Tone,
    pub muted: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UiStatus<'a> {
    pub workspace: &'a str,
    pub playback: &'a str,
    pub media_available: bool,
    pub audio_muted: bool,
}

#[derive(Clone, Debug)]
struct ButtonSpec {
    action: Option<Action>,
    label: String,
    icon: bool,
    tone: Tone,
    muted: bool,
}

const VERTICAL_MARGIN: u32 = 0;
const GAP: u32 = 12;
const NORMAL_ACTION_COUNT: u32 = 11;
const FUNCTION_ACTION_COUNT: u32 = 14;
const ACTION_BUTTON_WIDTH: u32 = 143;

const ICON_PREVIOUS: &str = "\u{f048}";
const ICON_PLAY: &str = "\u{f04b}";
const ICON_PAUSE: &str = "\u{f04c}";
const ICON_NEXT: &str = "\u{f051}";
const ICON_DIM: &str = "\u{f042}";
const ICON_BRIGHT: &str = "\u{f185}";
const ICON_MUTE: &str = "\u{f026}";
const ICON_VOLUME_DOWN: &str = "\u{f027}";
const ICON_VOLUME_UP: &str = "\u{f028}";
const ICON_STOCK: &str = "\u{f0e2}";

pub fn normal_layout(width: u32, height: u32, status: UiStatus<'_>) -> Vec<Button> {
    let left_specs = vec![
        spec(Action::Escape, "ESC", Tone::Accent, false),
        icon_spec(
            Action::MediaPrevious,
            ICON_PREVIOUS,
            Tone::Media,
            !status.media_available,
        ),
        icon_spec(
            Action::MediaPlayPause,
            if status.playback == "PAUSE" {
                ICON_PAUSE
            } else {
                ICON_PLAY
            },
            Tone::Media,
            !status.media_available,
        ),
        icon_spec(
            Action::MediaNext,
            ICON_NEXT,
            Tone::Media,
            !status.media_available,
        ),
        icon_spec(Action::BrightnessDown, ICON_DIM, Tone::Brightness, false),
        icon_spec(Action::BrightnessUp, ICON_BRIGHT, Tone::Brightness, false),
    ];
    let right_specs = vec![
        icon_spec(
            Action::ToggleMute,
            if status.audio_muted {
                ICON_VOLUME_UP
            } else {
                ICON_MUTE
            },
            Tone::Audio,
            false,
        ),
        icon_spec(Action::VolumeDown, ICON_VOLUME_DOWN, Tone::Audio, false),
        icon_spec(Action::VolumeUp, ICON_VOLUME_UP, Tone::Audio, false),
        icon_spec(Action::StockRenderer, ICON_STOCK, Tone::Danger, false),
        spec(Action::FnToggle, "FN", Tone::Accent, false),
    ];

    debug_assert_eq!(
        left_specs.len() + right_specs.len(),
        NORMAL_ACTION_COUNT as usize
    );
    let (action_width, horizontal_margin) = shared_action_geometry(width);
    let total_gap = GAP.saturating_mul(NORMAL_ACTION_COUNT);
    let available = width
        .saturating_sub(horizontal_margin * 2)
        .saturating_sub(total_gap);
    let workspace_width = available.saturating_sub(action_width * NORMAL_ACTION_COUNT);
    let mut x = horizontal_margin;
    let button_height = height.saturating_sub(VERTICAL_MARGIN * 2);
    let mut buttons = Vec::with_capacity(NORMAL_ACTION_COUNT as usize + 1);

    for spec in left_specs {
        buttons.push(button_at(spec, x, action_width, button_height));
        x = x.saturating_add(action_width + GAP);
    }
    buttons.push(Button {
        action: None,
        rect: Rect {
            x,
            y: VERTICAL_MARGIN,
            width: workspace_width,
            height: button_height,
        },
        label: status.workspace.to_uppercase(),
        icon: false,
        tone: Tone::Info,
        muted: false,
    });
    x = x.saturating_add(workspace_width + GAP);
    for spec in right_specs {
        buttons.push(button_at(spec, x, action_width, button_height));
        x = x.saturating_add(action_width + GAP);
    }
    buttons
}

pub fn function_layout(width: u32, height: u32) -> Vec<Button> {
    let mut specs = Vec::with_capacity(FUNCTION_ACTION_COUNT as usize);
    specs.push(spec(Action::Escape, "ESC", Tone::Accent, false));
    for number in 1..=12 {
        let tone = match number {
            1 | 2 | 5 | 6 => Tone::Brightness,
            3 | 4 => Tone::Accent,
            7..=9 => Tone::Media,
            10..=12 => Tone::Audio,
            _ => unreachable!(),
        };
        specs.push(spec(
            Action::Function(number),
            &format!("F{number}"),
            tone,
            false,
        ));
    }
    specs.push(spec(Action::FnToggle, "FN", Tone::Accent, false));
    debug_assert_eq!(specs.len(), FUNCTION_ACTION_COUNT as usize);
    let (action_width, horizontal_margin) = shared_action_geometry(width);
    layout_fixed_group(horizontal_margin, height, action_width, specs)
}

pub fn touch_id_layout(width: u32, height: u32, cancellable: bool) -> Vec<Button> {
    if !cancellable {
        return Vec::new();
    }
    vec![Button {
        action: Some(Action::CancelTouchId),
        rect: Rect {
            x: width.saturating_sub(194),
            y: VERTICAL_MARGIN,
            width: 190,
            height: height.saturating_sub(VERTICAL_MARGIN * 2),
        },
        label: "CANCEL".into(),
        icon: false,
        tone: Tone::Danger,
        muted: false,
    }]
}

pub fn action_at(buttons: &[Button], x: u32, y: u32) -> Option<Action> {
    buttons
        .iter()
        .find(|button| button.action.is_some() && button.rect.contains(x, y))
        .and_then(|button| button.action)
}

fn spec(action: Action, label: &str, tone: Tone, muted: bool) -> ButtonSpec {
    ButtonSpec {
        action: Some(action),
        label: label.into(),
        icon: false,
        tone,
        muted,
    }
}

fn icon_spec(action: Action, label: &str, tone: Tone, muted: bool) -> ButtonSpec {
    ButtonSpec {
        action: Some(action),
        label: label.into(),
        icon: true,
        tone,
        muted,
    }
}

fn button_at(spec: ButtonSpec, x: u32, width: u32, height: u32) -> Button {
    Button {
        action: spec.action,
        rect: Rect {
            x,
            y: VERTICAL_MARGIN,
            width,
            height,
        },
        label: spec.label,
        icon: spec.icon,
        tone: spec.tone,
        muted: spec.muted,
    }
}

fn shared_action_geometry(width: u32) -> (u32, u32) {
    let total_gap = GAP.saturating_mul(FUNCTION_ACTION_COUNT.saturating_sub(1));
    let available = width.saturating_sub(total_gap);
    let action_width = ACTION_BUTTON_WIDTH.min(available / FUNCTION_ACTION_COUNT);
    let content_width = action_width
        .saturating_mul(FUNCTION_ACTION_COUNT)
        .saturating_add(total_gap);
    let horizontal_margin = width.saturating_sub(content_width) / 2;
    (action_width, horizontal_margin)
}

fn layout_fixed_group(
    start: u32,
    height: u32,
    button_width: u32,
    specs: Vec<ButtonSpec>,
) -> Vec<Button> {
    if specs.is_empty() {
        return Vec::new();
    }
    let mut x = start;

    specs
        .into_iter()
        .map(|spec| {
            let button = Button {
                action: spec.action,
                rect: Rect {
                    x,
                    y: VERTICAL_MARGIN,
                    width: button_width,
                    height: height.saturating_sub(VERTICAL_MARGIN * 2),
                },
                label: spec.label,
                icon: spec.icon,
                tone: spec.tone,
                muted: spec.muted,
            };
            x = x.saturating_add(button_width + GAP);
            button
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_layout_keeps_action_widths_and_gives_workspace_the_remainder() {
        let buttons = normal_layout(
            2170,
            60,
            UiStatus {
                workspace: "Voyager",
                playback: "PLAY",
                media_available: true,
                audio_muted: false,
            },
        );
        let workspace = buttons
            .iter()
            .find(|button| button.action.is_none())
            .unwrap();
        let (expected_action_width, expected_margin) = shared_action_geometry(2170);
        let expected_workspace_width = 2170
            - expected_margin * 2
            - GAP * NORMAL_ACTION_COUNT
            - expected_action_width * NORMAL_ACTION_COUNT;
        assert_eq!(expected_action_width, ACTION_BUTTON_WIDTH);
        assert_eq!(workspace.rect.width, expected_workspace_width);
        assert!(buttons.iter().all(|button| button.rect.height == 60));
        let action_widths = buttons
            .iter()
            .filter(|button| button.action.is_some())
            .map(|button| button.rect.width)
            .collect::<Vec<_>>();
        assert_eq!(action_widths.len(), NORMAL_ACTION_COUNT as usize);
        assert!(
            action_widths
                .iter()
                .all(|width| *width == ACTION_BUTTON_WIDTH)
        );
        assert_eq!(buttons.last().unwrap().action, Some(Action::FnToggle));
        assert_eq!(buttons.first().unwrap().label, "ESC");
        assert!(!buttons.first().unwrap().icon);
        assert_eq!(buttons.last().unwrap().label, "FN");
        assert!(!buttons.last().unwrap().icon);

        let mut ordered = buttons.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|button| button.rect.x);
        assert!(ordered.windows(2).all(|pair| {
            pair[1]
                .rect
                .x
                .saturating_sub(pair[0].rect.x + pair[0].rect.width)
                == GAP
        }));
        let left_margin = ordered[0].rect.x;
        let last = ordered.last().unwrap();
        let right_margin = 2170 - last.rect.x - last.rect.width;
        assert_eq!(left_margin, expected_margin);
        assert_eq!(right_margin, expected_margin);
    }

    #[test]
    fn function_layout_keeps_all_keys_large_and_separated() {
        let buttons = function_layout(2170, 60);
        assert_eq!(buttons.len(), FUNCTION_ACTION_COUNT as usize);
        assert!(
            buttons
                .iter()
                .all(|button| button.rect.width == ACTION_BUTTON_WIDTH)
        );
        assert!(buttons.windows(2).all(|pair| {
            pair[1]
                .rect
                .x
                .saturating_sub(pair[0].rect.x + pair[0].rect.width)
                == GAP
        }));
        let (_, expected_margin) = shared_action_geometry(2170);
        let last = buttons.last().unwrap();
        assert_eq!(buttons[0].rect.x, expected_margin);
        assert_eq!(2170 - last.rect.x - last.rect.width, expected_margin);
    }

    #[test]
    fn action_width_is_identical_across_normal_and_function_layouts() {
        let normal = normal_layout(
            2170,
            60,
            UiStatus {
                workspace: "Voyager",
                playback: "PLAY",
                media_available: true,
                audio_muted: false,
            },
        );
        let function = function_layout(2170, 60);
        let widths = normal
            .iter()
            .filter(|button| button.action.is_some())
            .chain(function.iter())
            .map(|button| button.rect.width)
            .collect::<Vec<_>>();
        assert!(widths.iter().all(|width| *width == ACTION_BUTTON_WIDTH));
    }
}
