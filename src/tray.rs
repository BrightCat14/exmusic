use std::ffi::CString;
use std::sync::Mutex;

use crate::i18n::Translations;

static ACTIONS: Mutex<Vec<i32>> = Mutex::new(Vec::new());

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayAction {
    TogglePlayback,
    Next,
    Previous,
}

#[unsafe(no_mangle)]
pub extern "C" fn exmusic_tray_event(action: i32) {
    if let Ok(mut actions) = ACTIONS.lock() {
        actions.push(action);
    }
}

pub fn drain_actions() -> Vec<TrayAction> {
    let actions = ACTIONS
        .lock()
        .map(|mut actions| std::mem::take(&mut *actions))
        .unwrap_or_default();
    actions
        .into_iter()
        .filter_map(|action| match action {
            0 => Some(TrayAction::TogglePlayback),
            1 => Some(TrayAction::Next),
            2 => Some(TrayAction::Previous),
            _ => None,
        })
        .collect()
}

pub struct Labels {
    play: String,
    pause: String,
    next: String,
    previous: String,
    show: String,
    hide: String,
    quit: String,
    nothing: String,
}

impl Labels {
    pub fn new(translations: &Translations, language: &str) -> Self {
        let tr = |key: &str| translations.translate(language, key);
        Self {
            play: tr("tray.play"),
            pause: tr("tray.pause"),
            next: tr("tray.next"),
            previous: tr("tray.previous"),
            show: tr("tray.show"),
            hide: tr("tray.hide"),
            quit: tr("tray.quit"),
            nothing: tr("tray.nothing"),
        }
    }
}

fn cstring(value: &str) -> CString {
    CString::new(value).unwrap_or_default()
}

unsafe extern "C" {
    fn exmusic_application_new() -> i32;
    fn exmusic_application_exec() -> i32;
    fn exmusic_window_handle() -> u64;
    fn exmusic_window_raise();
    fn exmusic_quit();
    fn exmusic_tray_set_visible(visible: i32) -> i32;
    fn exmusic_tray_set_labels(
        play: *const std::ffi::c_char,
        pause: *const std::ffi::c_char,
        next: *const std::ffi::c_char,
        previous: *const std::ffi::c_char,
        show: *const std::ffi::c_char,
        hide: *const std::ffi::c_char,
        quit: *const std::ffi::c_char,
        nothing: *const std::ffi::c_char,
    );
    fn exmusic_tray_set_metadata(
        title: *const std::ffi::c_char,
        artist: *const std::ffi::c_char,
        playing: i32,
    );
}

pub fn create_application() -> bool {
    unsafe { exmusic_application_new() == 0 }
}

pub fn exec() -> i32 {
    unsafe { exmusic_application_exec() }
}

pub fn window_handle() -> u64 {
    unsafe { exmusic_window_handle() }
}

pub fn raise_window() {
    unsafe { exmusic_window_raise() }
}

pub fn quit() {
    unsafe { exmusic_quit() }
}

pub fn configure(enabled: bool, labels: &Labels) -> bool {
    let play = cstring(&labels.play);
    let pause = cstring(&labels.pause);
    let next = cstring(&labels.next);
    let previous = cstring(&labels.previous);
    let show = cstring(&labels.show);
    let hide = cstring(&labels.hide);
    let quit = cstring(&labels.quit);
    let nothing = cstring(&labels.nothing);
    unsafe {
        exmusic_tray_set_labels(
            play.as_ptr(),
            pause.as_ptr(),
            next.as_ptr(),
            previous.as_ptr(),
            show.as_ptr(),
            hide.as_ptr(),
            quit.as_ptr(),
            nothing.as_ptr(),
        );
        exmusic_tray_set_visible(if enabled { 1 } else { 0 }) != 0
    }
}

pub fn set_metadata(title: &str, artist: &str, playing: bool) {
    let title = cstring(title);
    let artist = cstring(artist);
    unsafe {
        exmusic_tray_set_metadata(title.as_ptr(), artist.as_ptr(), if playing { 1 } else { 0 });
    }
}
