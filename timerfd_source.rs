//! Integration of Linux' timerfd into the GLib main loop as a GSource

use std::cast;
use std::libc::*;
use std::mem;
use std::os;
use std::unstable::intrinsics;

use gtk::ffi::*;

#[deriving(Default)]
struct timespec {
    tv_sec: time_t,
    tv_nsec: c_long,
}

#[deriving(Default)]
struct itimerspec {
    it_interval: timespec,
    it_value: timespec,
}

extern "C" {
    fn timerfd_create(clockid: c_int, flags: c_int) -> c_int;
    fn timerfd_settime(fd: c_int, flags: c_int,
                       new_value: *itimerspec, old_value: *mut itimerspec) -> c_int;
    fn timerfd_gettime(fd: c_int, curr_value: *mut itimerspec) -> c_int;
}

static CLOCK_MONOTONIC: c_int = 1;
static TFD_CLOEXEC: c_int = 0o2000000;
static TFD_NONBLOCK: c_int = 0o0004000;

/// Slightly nicer interface to the C functions.
struct TimerFD(c_int);

impl TimerFD {
    fn new() -> TimerFD {
        unsafe {
            let fd = timerfd_create(CLOCK_MONOTONIC, TFD_CLOEXEC | TFD_NONBLOCK);
            if fd == -1 {
                fail!("Failed to create timerfd: `{}`", os::last_os_error());
            }
            TimerFD(fd)
        }
    }

    fn settime(&mut self, new_value: &itimerspec) -> itimerspec {
        unsafe {
            let mut result = intrinsics::uninit();
            let ret = timerfd_settime(**self, 0, new_value, &mut result);
            if ret != 0 {
                fail!("Failed to set time of timerfd: `{}`", os::last_os_error());
            }
            result
        }
    }

    fn gettime(&self) -> itimerspec {
        unsafe {
            let mut result = intrinsics::uninit();
            let ret = timerfd_gettime(**self, &mut result);
            if ret != 0 {
                fail!("Failed to get time from timerfd: `{}`", os::last_os_error());
            }
            result
        }
    }
}

/// The actually used timer
pub struct Timer {
    priv timerfd: TimerFD,
    priv current: itimerspec,
    priv active: bool,
}

impl Timer {
    pub fn new() -> Timer {
        Timer {
            timerfd: TimerFD::new(),
            current: Default::default(),
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
        let zero = Default::default();
        let res = self.current = self.timerfd.settime(&zero);
        self.active = false;
        res
    }
}

impl Drop for TimerFD {
    fn drop(&mut self) {
        unsafe {
            close(**self);
        }
    }
}

pub trait TimerGSourceCallback {
    fn callback(&mut self, timer: &mut Timer) -> bool;
}

struct TimerGSourceInner {
    priv g_source: *mut GSource,
    timer: Timer,
    priv callback_object: ~TimerGSourceCallback: Freeze + Send,
}

struct TimerGSource(~TimerGSourceInner);

impl TimerGSource {
    pub fn new(callback_object: ~TimerGSourceCallback: Freeze + Send) -> TimerGSource {
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
                dispatch_timerfd_g_source_for_realz,
                cast::transmute(&*tgsi),
                cast::transmute(0));
        }
        TimerGSource(tgsi)
    }

    pub fn attach(&mut self, context: *mut GMainContext) {
        unsafe {
            let _tag = g_source_add_unix_fd(self.g_source, *self.timer.timerfd, G_IO_IN);
            g_source_attach(self.g_source, context);
        }
    }
}

impl Drop for TimerGSource {
    fn drop(&mut self) {
        unsafe {
            g_source_destroy(self.g_source);
            g_source_unref(self.g_source);
        }
    }
}

extern "C" fn dispatch_timerfd_g_source_for_realz(user_data: gpointer) -> gboolean {
    let tgs: &mut TimerGSourceInner = unsafe { cast::transmute(user_data) };

    let cont = tgs.callback_object.callback(&mut tgs.timer);

    // Have to read, so old timer ticks are not messing up epoll
    let mut buffer = [0, ..8];
    let n = unsafe { read(*tgs.timer.timerfd, cast::transmute(&mut buffer), 8) };
    if n != 8 {
        // Can happen when the callback reads the fd as well
        assert_eq!(os::errno() as c_int, EAGAIN);
    }

    if cont { 1 } else { 0 }
}

extern "C" fn dispatch_timerfd_g_source(src: *mut GSource,
        callback: GSourceFunc, user_data: gpointer) -> gboolean {

    let tgs: &mut TimerGSourceInner = unsafe { cast::transmute(user_data) };
    assert_eq!(tgs.g_source, src);
    callback(user_data)
}

/// To get around limitation of casting NULL function pointers in static data.
/// TODO: maybe implement this in bindgen?
struct My_Struct__GSourceFuncs {
    prepare: Option<extern "C" fn(arg1: *mut GSource, arg2: *mut gint) -> gboolean>,
    check: Option<extern "C" fn(arg1: *mut GSource) -> gboolean>,
    dispatch: extern "C" fn
                  (arg1: *mut GSource, arg2: GSourceFunc, arg3: gpointer)
                  -> gboolean,
    finalize: Option<extern "C" fn(arg1: *mut GSource)>,
    closure_callback: Option<GSourceFunc>,
    closure_marshal: Option<GSourceDummyMarshal>,
}

static mut TIMER_GSOURCE_FUNCS: My_Struct__GSourceFuncs = My_Struct__GSourceFuncs {
    prepare: None,
    check: None,
    dispatch: dispatch_timerfd_g_source,
    finalize: None,
    closure_callback: None,
    closure_marshal: None
};
