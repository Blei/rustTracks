use std::cast;
use std::logging;
use std::ptr;
use std::rt::comm;
use std::str::raw::from_c_str;

use gtk::*;
use gtk::ffi::*;

use gui;
use tfs = timerfd_source;

static PLAYBIN_ELEMENT_NAME: &'static str = "rusttracks-playbin";

struct ReportCallback {
    chan: comm::SharedChan<gui::GuiUpdateMessage>,
}

impl ReportCallback {
    fn new(chan: comm::SharedChan<gui::GuiUpdateMessage>) -> ReportCallback {
        ReportCallback { chan: chan }
    }
}

impl tfs::TimerGSourceCallback for ReportCallback {
    fn callback(&mut self, _timer: &mut tfs::Timer) -> bool {
        self.chan.send(gui::ReportCurrentTrack);
        false
    }
}

struct ProgressCallback {
    chan: comm::SharedChan<gui::GuiUpdateMessage>,
    playbin: *mut GstElement,
}

impl ProgressCallback {
    fn new(chan: comm::SharedChan<gui::GuiUpdateMessage>, playbin: *mut GstElement) -> ProgressCallback {
        ProgressCallback { chan: chan, playbin: playbin }
    }
}

impl tfs::TimerGSourceCallback for ProgressCallback {
    fn callback(&mut self, _timer: &mut tfs::Timer) -> bool {
        let mut current_position = 0;
        let mut current_duration = 0;

        let success_position = unsafe {
            gst_element_query_position(
                self.playbin, GST_FORMAT_TIME, &mut current_position)
        };

        let success_duration = unsafe {
            gst_element_query_duration(
                self.playbin, GST_FORMAT_TIME, &mut current_duration)
        };

        if success_duration != 0 && success_position != 0 {
            self.chan.send(gui::SetProgress(Some((current_position, current_duration))));
        } else {
            self.chan.send(gui::SetProgress(None));
        }

        true
    }
}

#[deriving(Eq)]
enum PlayState {
    Uninit,
    NoUri,
    Play,
    Pause,
    WaitToPlay,
}

struct Player {
    state: PlayState,

    playbin: *mut GstElement,

    report_timer: Option<tfs::TimerGSource>,
    progress_timer: Option<tfs::TimerGSource>,
}

impl Player {
    pub fn new() -> Player {
        Player {
            state: Uninit,
            playbin: ptr::mut_null(),
            report_timer: None,
            progress_timer: None,
        }
    }

    // It's important that the `gui` pointer be constant for the entire duration
    // of the program, as it's sent into the gstreamer lib.
    // I know, this is <strike>quite</strike> very hacky.
    pub fn init(&mut self, args: ~[~str], gui: &gui::Gui) -> ~[~str] {
        let args2 = unsafe {
            gst_init_with_args(args)
        };
        unsafe {
            "playbin".with_c_str(|c_str| {
                PLAYBIN_ELEMENT_NAME.with_c_str(|rtpb| {
                    self.playbin = gst_element_factory_make(c_str, rtpb);
                });
            });
            if self.playbin.is_null() {
                fail!("failed to create playbin");
            }

            let bus = gst_pipeline_get_bus(cast::transmute(self.playbin));
            gst_bus_add_watch(bus, bus_callback,
                              cast::transmute::<&gui::Gui, gpointer>(gui));
        }
        self.state = NoUri;
        args2
    }

    pub fn set_uri(&mut self, uri: &str) {
        self.stop();
        unsafe {
            "uri".with_c_str(|property_c_str| {
                uri.with_c_str(|uri_c_str| {
                    g_object_set(cast::transmute(self.playbin),
                        property_c_str, uri_c_str, ptr::null::<gchar>());
                });
            });
        }
        self.state = WaitToPlay;
    }

    pub fn play(&mut self) {
        match self.state {
            Uninit => fail!("player is not initialized"),
            NoUri => fail!("no uri set"),
            Play => {
                info!("already playing");
                return;
            }
            _ => ()
        }
        unsafe {
            gst_element_set_state(self.playbin, GST_STATE_PLAYING);
        }
        self.state = Play;
    }

    pub fn pause(&mut self) {
        match self.state {
            Uninit => fail!("player is not initialized"),
            NoUri => fail!("uri not set"),
            Pause => {
                warn!("already pausing");
                return;
            }
            _ => ()
        }
        unsafe {
            gst_element_set_state(self.playbin, GST_STATE_PAUSED);
        }
        self.state = Pause;
    }

    pub fn stop(&mut self) {
        match self.state {
            Uninit => fail!("player is not initialized"),
            NoUri => {
                warn!("already stopped");
                return;
            }
            _ => ()
        }
        self.stop_timers();
        unsafe{
            gst_element_set_state(self.playbin, GST_STATE_READY);
        }
        self.state = NoUri;
    }

    pub fn toggle(&mut self) {
        match self.state {
            Uninit => fail!("player is not initialized"),
            NoUri => fail!("Uri is not set"),
            Play | WaitToPlay => self.pause(),
            Pause => self.play()
        }
    }

    pub fn set_buffering(&mut self, is_buffering: bool) {
        match self.state {
            Uninit => fail!("player is not initialized"),
            NoUri => fail!("Uri is not set"),
            Play if is_buffering => {
                unsafe {
                    gst_element_set_state(self.playbin, GST_STATE_PAUSED);
                }
                self.state = WaitToPlay;
            }
            WaitToPlay if !is_buffering => {
                unsafe {
                    gst_element_set_state(self.playbin, GST_STATE_PLAYING);
                }
                self.state = Play;
            }
            _ => {
                // Nothing to do
            }
        }
    }

    pub fn is_playing(&self) -> bool {
        self.state == Play || self.state == WaitToPlay
    }

    pub fn start_timers(&mut self, chan: comm::SharedChan<gui::GuiUpdateMessage>) {
        let context = unsafe { g_main_context_default() };

        if self.report_timer.is_none() {
            let rc = ~ReportCallback::new(chan.clone());
            let mut rt = tfs::TimerGSource::new(rc as ~tfs::TimerGSourceCallback: Freeze + Send);
            rt.attach(context);
            rt.timer.set_oneshot(30 * 1000);
            self.report_timer = Some(rt);
        }
        self.report_timer.get_mut_ref().timer.start();

        if self.progress_timer.is_none() {
            let pc = ~ProgressCallback::new(chan, self.playbin);
            let mut pt = tfs::TimerGSource::new(pc as ~tfs::TimerGSourceCallback: Freeze + Send);
            pt.attach(context);
            pt.timer.set_interval(1, 1 * 1000);
            self.progress_timer = Some(pt);
        }
        self.progress_timer.get_mut_ref().timer.start();
    }

    pub fn pause_timers(&mut self) {
        match self.report_timer {
            Some(ref mut rt) => rt.timer.stop(),
            None => ()
        }
        match self.progress_timer {
            Some(ref mut pt) => pt.timer.stop(),
            None => ()
        }
    }

    pub fn stop_timers(&mut self) {
        self.report_timer = None;
        self.progress_timer = None;
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        if self.state != Uninit {
            unsafe {
                if !self.playbin.is_null() {
                    gst_element_set_state(self.playbin, GST_STATE_NULL);
                    gst_object_unref(cast::transmute(self.playbin));
                }
                gst_deinit();
            }
        }
    }
}

extern "C" fn bus_callback(_bus: *mut GstBus, msg: *mut GstMessage, data: gpointer) -> gboolean {
    unsafe {
    let gui: &gui::Gui = cast::transmute(data);

    let name = {
        let gst_obj = (*msg).src;
        if gst_obj.is_null() {
            ~"null-source"
        } else {
            let name_ptr = gst_object_get_name(gst_obj);
            if name_ptr.is_null() {
                ~"null-name"
            } else {
                let name = from_c_str(cast::transmute_immut_unsafe(name_ptr));
                g_free(cast::transmute(name_ptr));
                name
            }
        }
    };

    match (*msg)._type {
        GST_MESSAGE_ERROR => {
            let mut err = ptr::mut_null();
            let mut dbg_info = ptr::mut_null();

            gst_message_parse_error(msg, &mut err, &mut dbg_info);

            let err_msg = from_c_str(cast::transmute_immut_unsafe((*err).message));

            error!("ERROR from element {}: {}", name, err_msg);
            error!("Debugging info: {}", from_c_str(cast::transmute_immut_unsafe(dbg_info)));

            gui.get_chan().send(gui::Notify(format!("Playback error: `{}`", err_msg)));

            g_error_free(err);
            g_free(cast::transmute(dbg_info));
        }
        GST_MESSAGE_WARNING => {
            if log_enabled!(logging::WARN) {
                let mut err = ptr::mut_null();
                let mut dbg_info = ptr::mut_null();

                gst_message_parse_error(msg, &mut err, &mut dbg_info);

                warn!("WARNING from element {}: {}", name,
                    from_c_str(cast::transmute_immut_unsafe((*err).message)));
                warn!("Debugging info: {}", from_c_str(cast::transmute_immut_unsafe(dbg_info)));

                g_error_free(err);
                g_free(cast::transmute(dbg_info));
            }
        }
        GST_MESSAGE_INFO => {
            if log_enabled!(logging::INFO) {
                let mut err = ptr::mut_null();
                let mut dbg_info = ptr::mut_null();

                gst_message_parse_error(msg, &mut err, &mut dbg_info);

                info!("INFO from element {}: {}", name,
                    from_c_str(cast::transmute_immut_unsafe((*err).message)));
                info!("Debugging info: {}", from_c_str(cast::transmute_immut_unsafe(dbg_info)));

                g_error_free(err);
                g_free(cast::transmute(dbg_info));
            }
        }
        GST_MESSAGE_EOS => {
            debug!("EOS from element {}", name);
            gui.get_chan().send(gui::NextTrack);
        }
        GST_MESSAGE_STATE_CHANGED => {
            if name.as_slice() == PLAYBIN_ELEMENT_NAME {
                let mut new_state = 0;
                gst_message_parse_state_changed(msg, ptr::mut_null(),
                    &mut new_state, ptr::mut_null());
                match new_state {
                    GST_STATE_PLAYING => {
                        gui.get_chan().send(gui::StartTimers);
                    }
                    GST_STATE_PAUSED => {
                        gui.get_chan().send(gui::PauseTimers);
                    }
                    _ => {
                        // Do nothing, the timers will be overwritten anyways
                    }
                }
            }
        }
        GST_MESSAGE_BUFFERING => {
            let mut percent = 0;
            gst_message_parse_buffering(msg, &mut percent);
            info!("BUFFERING form element `{}`, {}%", name, percent);
            gui.get_chan().send(gui::SetBuffering(percent < 100));
        }
        _ => {
            if log_enabled!(logging::DEBUG) {
                let msg_type_cstr = gst_message_type_get_name((*msg)._type);
                let msg_type_name = ::std::str::raw::from_c_str(msg_type_cstr);
                debug!("message of type `{}` from element `{}`", msg_type_name, name);
            }
        }
    }

    // Returning 0 removes this callback
    return 1;
    }
}
