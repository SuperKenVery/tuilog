pub const TIMESTAMP_WIDTH: usize = 9;
pub const LINE_NUMBER_WIDTH: usize = 9;
pub const PREFIX_WIDTH_WITH_TIME: usize = TIMESTAMP_WIDTH + LINE_NUMBER_WIDTH;
pub const PREFIX_WIDTH_WITHOUT_TIME: usize = LINE_NUMBER_WIDTH;

pub const POLL_INTERVAL_MS: u64 = 50;

pub const INPUT_FIELD_HEIGHT: u16 = 3;
pub const STATUS_BAR_HEIGHT: u16 = 1;

pub const HELP_POPUP_WIDTH: u16 = 40;
pub const HELP_POPUP_HEIGHT: u16 = 5;

pub const QUIT_POPUP_WIDTH: u16 = 40;
pub const QUIT_POPUP_HEIGHT: u16 = 5;
