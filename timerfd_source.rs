//! Integration of Linux' timerfd into the GLib main loop as a GSource

// Mostly mirroring the names in C
#![allow(non_camel_case_types)]

use libc;

use std::cast;
use std::default;
use std::mem;
use std::os;

use gtk::ffi::*;

#[deriving(Default)]
struct timespec {
    tv_sec: time_t,
    tv_nsec: libc::c_long,
}

#[deriving(Default)]
struct itimerspec {
    it_interval: timespec,
    it_value: timespec,
}

extern "C" {
    fn timerfd_create(clockid: libc::c_int, flags: libc::c_int) -> libc::c_int;
    fn timerfd_settime(fd: libc::c_int, flags: libc::c_int,
                       new_value: *itimerspec, old_value: *mut itimerspec) -> libc::c_int;
    fn timerfd_gettime(fd: libc::c_int, curr_value: *mut itimerspec) -> libc::c_int;
}

static CLOCK_MONOTONIC: libc::c_int = 1;
static TFD_CLOEXEC: libc::c_int = 0o2000000;
static TFD_NONBLOCK: libc::c_int = 0o0004000;

/// Slightly nicer interface to the C functions.
pub struct TimerFD {
    fd: libc::c_int,
}

impl TimerFD {
    pub fn new() -> TimerFD {
        unsafe {
            let fd = timerfd_create(CLOCK_MONOTONIC, TFD_CLOEXEC | TFD_NONBLOCK);
            if fd == -1 {
                fail!("Failed to create timerfd: `{}`", os::last_os_error());
            }
            TimerFD { fd: fd }
        }
    }

    pub fn settime(&mut self, new_value: &itimerspec) -> itimerspec {
        unsafe {
            let mut result = mem::uninit();
            let ret = timerfd_settime(self.fd, 0, new_value, &mut result);
            if ret != 0 {
                fail!("Failed to set time of timerfd: `{}`", os::last_os_error());
            }
            result
        }
    }

    pub fn gettime(&self) -> itimerspec {
        unsafe {
            let mut result = mem::uninit();
            let ret = timerfd_gettime(self.fd, &mut result);
            if ret != 0 {
                fail!("Failed to get time from timerfd: `{}`", os::last_os_error());
            }
            result
        }
    }
}

impl Drop for TimerFD {
    fn drop(&mut self) {
        unsafe {
            close(self.fd);
        }
    }
}

/// The actually used timer
pub struct Timer {
    timerfd: TimerFD,
    current: itimerspec,
    active: bool,
}

impl Timer {
    pub fn new() -> Timer {
        Timer {
            timerfd: TimerFD::new(),
            current: default::Default::default(),
            active: false,
        }
    }

    /// initial_ms has to be > 0
    pub fn set_interval(&mut self, initial_ms: i64, interval_ms: i64) {
        assert!(initial_ms > 0);
        if self.active {
            fail!("don't change time of an active timer");
        }
        self.current.it_value.tv_sec  =  initial_ms / 1000;
        self.current.it_value.tv_nsec = (initial_ms % 1000) * 1000 * 1000;
        self.current.it_interval.tv_sec  =  interval_ms / 1000;
        self.current.it_interval.tv_nsec = (interval_ms % 1000) * 1000 * 1000;
    }

    /// Equivalent to `set_interval(timeout_ms, 0)`
    pub fn set_oneshot(&mut self, timeout_ms: i64) {
        self.set_interval(timeout_ms, 0);
    }

    pub fn start(&mut self) {
        if self.active {
            fail!("calling start on an active timer");
        }
        self.timerfd.settime(&self.current);
        self.active = true;
    }

    pub fn stop(&mut self) {
        if !self.active {
            fail!("calling stop on an non-active timer");
        }
        let zero = default::Default::default();
        let res = self.current = self.timerfd.settime(&zero);
        self.active = false;
        res
    }
}

pub trait TimerGSourceCallback: Send {
    fn callback(&mut self, timer: &mut Timer) -> bool;
}

struct TimerGSourceInner {
    g_source: *mut GSource,
    timer: Timer,
    callback_object: ~TimerGSourceCallback: Send,
}

pub struct TimerGSource {
    inner: ~TimerGSourceInner,
}

impl TimerGSource {
    pub fn new(callback_object: ~TimerGSourceCallback: Send) -> TimerGSource {
        let tgsi = ~TimerGSourceInner {
            g_source: unsafe {
                g_source_new(cast::transmute(&TIMER_GSOURCE_FUNCS),
                             mem::size_of::<GSource>() as guint)
            },
            timer: Timer::new(),
            callback_object: callback_object,
        };
        unsafe {
            g_source_set_callback(tgsi.g_source,
                Some(dispatch_timerfd_g_source_for_realz),
                cast::transmute(&*tgsi),
                cast::transmute(0));
        }
        TimerGSource { inner: tgsi }
    }

    pub fn attach(&mut self, context: *mut GMainContext) {
        unsafe {
            let _tag = g_source_add_unix_fd(self.inner.g_source,
                                            self.inner.timer.timerfd.fd, G_IO_IN);
            g_source_attach(self.inner.g_source, context);
        }
    }

    pub fn timer<'a>(&'a self) -> &'a Timer {
        &self.inner.timer
    }

    pub fn mut_timer<'a>(&'a mut self) -> &'a mut Timer {
        &mut self.inner.timer
    }
}

impl Drop for TimerGSource {
    fn drop(&mut self) {
        unsafe {
            g_source_destroy(self.inner.g_source);
            g_source_unref(self.inner.g_source);
        }
    }
}

extern "C" fn dispatch_timerfd_g_source_for_realz(user_data: gpointer) -> gboolean {
    let tgs: &mut TimerGSourceInner = unsafe { cast::transmute(user_data) };

    let cont = tgs.callback_object.callback(&mut tgs.timer);

    // Have to read, so old timer ticks are not messing up epoll
    let mut buffer = [0, ..8];
    let n = unsafe { read(tgs.timer.timerfd.fd, cast::transmute(&mut buffer), 8) };
    if n != 8 {
        // Can happen when the callback reads the fd as well
        assert_eq!(os::errno() as libc::c_int, libc::EAGAIN);
    }

    if cont { 1 } else { 0 }
}

extern "C" fn dispatch_timerfd_g_source(src: *mut GSource,
        callback: GSourceFunc, user_data: gpointer) -> gboolean {

    let tgs: &mut TimerGSourceInner = unsafe { cast::transmute(user_data) };
    assert_eq!(tgs.g_source, src);
    callback.expect("How could this happen? This must be set!")(user_data)
}

static mut TIMER_GSOURCE_FUNCS: GSourceFuncs = Struct__GSourceFuncs {
    prepare: None,
    check: None,
    dispatch: Some(dispatch_timerfd_g_source),
    finalize: None,
    closure_callback: None,
    closure_marshal: None
};
