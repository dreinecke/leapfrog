#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Action {
    Escape,
    Function(u8),
    MediaPrevious,
    MediaPlayPause,
    MediaNext,
    BrightnessDown,
    BrightnessUp,
    KeyboardBacklightDown,
    KeyboardBacklightUp,
    ToggleMute,
    VolumeDown,
    VolumeUp,
    PrintScreen,
    FnToggle,
    CancelTouchId,
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
pub enum Fill {
    Selection,
    Accent,
}

impl Fill {
    fn for_group(index: usize) -> Self {
        if index.is_multiple_of(2) {
            Self::Selection
        } else {
            Self::Accent
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Backlight {
    Dim,
    Bright,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Label {
    Text(String),
    Glyph(&'static str),
    KeyboardBacklight(Backlight),
}

#[derive(Clone, Debug)]
pub struct Button {
    pub action: Option<Action>,
    pub rect: Rect,
    pub label: Label,
    pub fill: Fill,
    pub muted: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UiStatus<'a> {
    pub playback: &'a str,
    pub media_available: bool,
    pub audio_muted: bool,
}

#[derive(Clone, Debug)]
struct ButtonSpec {
    action: Action,
    label: Label,
    muted: bool,
}

const VERTICAL_MARGIN: u32 = 0;
const GAP: u32 = 12;
const NORMAL_ACTION_COUNT: u32 = 13;
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
const ICON_PRINTSCREEN: &str = "\u{f0c4}";

pub fn normal_layout(width: u32, height: u32, status: UiStatus<'_>) -> Vec<Button> {
    let media_muted = !status.media_available;
    let groups = vec![
        vec![spec(Action::Escape, Label::Text("ESC".into()), false)],
        vec![
            spec(
                Action::MediaPrevious,
                Label::Glyph(ICON_PREVIOUS),
                media_muted,
            ),
            spec(
                Action::MediaPlayPause,
                Label::Glyph(if status.playback == "PAUSE" {
                    ICON_PAUSE
                } else {
                    ICON_PLAY
                }),
                media_muted,
            ),
            spec(Action::MediaNext, Label::Glyph(ICON_NEXT), media_muted),
        ],
        vec![
            spec(Action::BrightnessDown, Label::Glyph(ICON_DIM), false),
            spec(Action::BrightnessUp, Label::Glyph(ICON_BRIGHT), false),
        ],
        vec![
            spec(
                Action::KeyboardBacklightDown,
                Label::KeyboardBacklight(Backlight::Dim),
                false,
            ),
            spec(
                Action::KeyboardBacklightUp,
                Label::KeyboardBacklight(Backlight::Bright),
                false,
            ),
        ],
        vec![
            spec(
                Action::ToggleMute,
                Label::Glyph(if status.audio_muted {
                    ICON_VOLUME_UP
                } else {
                    ICON_MUTE
                }),
                false,
            ),
            spec(Action::VolumeDown, Label::Glyph(ICON_VOLUME_DOWN), false),
            spec(Action::VolumeUp, Label::Glyph(ICON_VOLUME_UP), false),
        ],
        vec![spec(
            Action::PrintScreen,
            Label::Glyph(ICON_PRINTSCREEN),
            false,
        )],
        vec![spec(Action::FnToggle, Label::Text("FN".into()), false)],
    ];
    let specs = alternate_fills(groups, 0);
    debug_assert_eq!(specs.len(), NORMAL_ACTION_COUNT as usize);
    let (action_width, horizontal_margin) = action_geometry(width, NORMAL_ACTION_COUNT);
    layout_fixed_group(horizontal_margin, height, action_width, specs)
}

pub fn function_layout(width: u32, height: u32) -> Vec<Button> {
    let mut groups = vec![vec![spec(Action::Escape, Label::Text("ESC".into()), false)]];
    for keys in [1..=2, 3..=4, 5..=6, 7..=9, 10..=12] {
        groups.push(
            keys.map(|number| {
                spec(
                    Action::Function(number),
                    Label::Text(format!("F{number}")),
                    false,
                )
            })
            .collect(),
        );
    }
    groups.push(vec![spec(
        Action::FnToggle,
        Label::Text("FN".into()),
        false,
    )]);
    let specs = alternate_fills(groups, 0);
    debug_assert_eq!(specs.len(), FUNCTION_ACTION_COUNT as usize);
    let (action_width, horizontal_margin) = action_geometry(width, FUNCTION_ACTION_COUNT);
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
        label: Label::Text("CANCEL".into()),
        fill: Fill::Accent,
        muted: false,
    }]
}

pub fn action_at(buttons: &[Button], x: u32, y: u32) -> Option<Action> {
    buttons
        .iter()
        .find(|button| button.action.is_some() && button.rect.contains(x, y))
        .and_then(|button| button.action)
}

fn spec(action: Action, label: Label, muted: bool) -> ButtonSpec {
    ButtonSpec {
        action,
        label,
        muted,
    }
}

fn alternate_fills(groups: Vec<Vec<ButtonSpec>>, first_group: usize) -> Vec<(ButtonSpec, Fill)> {
    groups
        .into_iter()
        .enumerate()
        .flat_map(|(offset, group)| {
            let fill = Fill::for_group(first_group + offset);
            group.into_iter().map(move |spec| (spec, fill))
        })
        .collect()
}

fn button_at(spec: ButtonSpec, fill: Fill, x: u32, width: u32, height: u32) -> Button {
    Button {
        action: Some(spec.action),
        rect: Rect {
            x,
            y: VERTICAL_MARGIN,
            width,
            height,
        },
        label: spec.label,
        fill,
        muted: spec.muted,
    }
}

fn action_geometry(width: u32, count: u32) -> (u32, u32) {
    let total_gap = GAP.saturating_mul(count.saturating_sub(1));
    let available = width.saturating_sub(total_gap);
    let action_width = ACTION_BUTTON_WIDTH.min(available / count);
    let content_width = action_width
        .saturating_mul(count)
        .saturating_add(total_gap);
    let horizontal_margin = width.saturating_sub(content_width) / 2;
    (action_width, horizontal_margin)
}

fn layout_fixed_group(
    start: u32,
    height: u32,
    button_width: u32,
    specs: Vec<(ButtonSpec, Fill)>,
) -> Vec<Button> {
    let mut x = start;
    specs
        .into_iter()
        .map(|(spec, fill)| {
            let button = button_at(
                spec,
                fill,
                x,
                button_width,
                height.saturating_sub(VERTICAL_MARGIN * 2),
            );
            x = x.saturating_add(button_width + GAP);
            button
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status() -> UiStatus<'static> {
        UiStatus {
            playback: "PLAY",
            media_available: true,
            audio_muted: false,
        }
    }

    fn fill_runs(buttons: &[Button]) -> Vec<Fill> {
        let mut runs: Vec<Fill> = Vec::new();
        for button in buttons {
            if runs.last() != Some(&button.fill) {
                runs.push(button.fill);
            }
        }
        runs
    }

    #[test]
    fn normal_layout_keeps_action_widths_and_centers_the_row() {
        let buttons = normal_layout(2170, 60, status());
        assert_eq!(buttons.len(), NORMAL_ACTION_COUNT as usize);
        assert!(buttons.iter().all(|button| button.action.is_some()));
        assert!(buttons.iter().all(|button| button.rect.width == ACTION_BUTTON_WIDTH));
        assert!(buttons.iter().all(|button| button.rect.height == 60));
        assert_eq!(buttons.first().unwrap().label, Label::Text("ESC".into()));
        assert_eq!(buttons.last().unwrap().action, Some(Action::FnToggle));
        assert_eq!(buttons.last().unwrap().label, Label::Text("FN".into()));

        let mut ordered = buttons.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|button| button.rect.x);
        assert!(ordered.windows(2).all(|pair| {
            pair[1]
                .rect
                .x
                .saturating_sub(pair[0].rect.x + pair[0].rect.width)
                == GAP
        }));
        let (expected_action_width, expected_margin) = action_geometry(2170, NORMAL_ACTION_COUNT);
        assert_eq!(expected_action_width, ACTION_BUTTON_WIDTH);
        assert_eq!(ordered[0].rect.x, expected_margin);
        let last = ordered.last().unwrap();
        let right_margin = 2170 - last.rect.x - last.rect.width;
        assert_eq!(right_margin, expected_margin + 1);
    }

    #[test]
    fn normal_layout_slots_the_printscreen_between_volume_up_and_the_function_group() {
        let buttons = normal_layout(2170, 60, status());
        let actions = buttons
            .iter()
            .map(|button| button.action)
            .collect::<Vec<_>>();
        assert_eq!(
            actions,
            [
                Some(Action::Escape),
                Some(Action::MediaPrevious),
                Some(Action::MediaPlayPause),
                Some(Action::MediaNext),
                Some(Action::BrightnessDown),
                Some(Action::BrightnessUp),
                Some(Action::KeyboardBacklightDown),
                Some(Action::KeyboardBacklightUp),
                Some(Action::ToggleMute),
                Some(Action::VolumeDown),
                Some(Action::VolumeUp),
                Some(Action::PrintScreen),
                Some(Action::FnToggle),
            ]
        );
    }

    #[test]
    fn neighbouring_groups_alternate_between_the_two_fills() {
        let expected = [
            Fill::Selection,
            Fill::Accent,
            Fill::Selection,
            Fill::Accent,
            Fill::Selection,
            Fill::Accent,
            Fill::Selection,
        ];
        assert_eq!(fill_runs(&normal_layout(2170, 60, status())), expected);
        assert_eq!(fill_runs(&function_layout(2170, 60)), expected);
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
        let (_, expected_margin) = action_geometry(2170, FUNCTION_ACTION_COUNT);
        let last = buttons.last().unwrap();
        assert_eq!(buttons[0].rect.x, expected_margin);
        assert_eq!(2170 - last.rect.x - last.rect.width, expected_margin);
    }

    #[test]
    fn action_width_is_identical_across_normal_and_function_layouts() {
        let normal = normal_layout(2170, 60, status());
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
