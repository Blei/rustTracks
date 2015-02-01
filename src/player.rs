use std::mem;
use std::ptr;
use std::str;
use std::sync::mpsc;

use log;

use gtk::*;
use gtk::ffi::*;

use timerfd;

use gui;

static PLAYBIN_ELEMENT_NAME: &'static str = "rusttracks-playbin";

struct ReportCallback {
    sender: mpsc::Sender<gui::GuiUpdateMessage>,
}

impl ReportCallback {
    fn new(sender: mpsc::Sender<gui::GuiUpdateMessage>) -> ReportCallback {
        ReportCallback { sender: sender }
    }
}

impl timerfd::TimerGSourceCallback for ReportCallback {
    fn callback(&mut self, _timer: &mut timerfd::Timer) -> bool {
        self.sender.send(gui::GuiUpdateMessage::ReportCurrentTrack);
        false
    }
}

struct ProgressCallback {
    sender: mpsc::Sender<gui::GuiUpdateMessage>,
}

impl ProgressCallback {
    fn new(sender: mpsc::Sender<gui::GuiUpdateMessage>) -> ProgressCallback {
        ProgressCallback { sender: sender }
    }
}

impl timerfd::TimerGSourceCallback for ProgressCallback {
    fn callback(&mut self, _timer: &mut timerfd::Timer) -> bool {
        self.sender.send(gui::GuiUpdateMessage::UpdateProgress);
        true
    }
}

#[derive(PartialEq,Eq)]
enum PlayState {
    Uninit,
    NoUri,
    Play,
    Pause,
    WaitToPlay,
}

pub struct Player {
    state: PlayState,
    gui_sender: Option<Box<mpsc::Sender<gui::GuiUpdateMessage>>>,

    playbin: *mut GstElement,

    report_timer: Option<timerfd::TimerGSource>,
    progress_timer: Option<timerfd::TimerGSource>,
}

impl Player {
    pub fn new() -> Player {
        Player {
            state: PlayState::Uninit,
            gui_sender: None,
            playbin: ptr::null_mut(),
            report_timer: None,
            progress_timer: None,
        }
    }

    pub fn init(&mut self, args: Vec<String>, gui_sender: mpsc::Sender<gui::GuiUpdateMessage>) -> Vec<String> {
        let args2 = unsafe {
            gst_init_with_args(args)
        };
        self.gui_sender = Some(Box::new(gui_sender));
        unsafe {
            "playbin".with_c_str(|c_str| {
                PLAYBIN_ELEMENT_NAME.with_c_str(|rtpb| {
                    self.playbin = gst_element_factory_make(c_str, rtpb);
                });
            });
            if self.playbin.is_null() {
                panic!("failed to create playbin");
            }

            let bus = gst_pipeline_get_bus(self.playbin as *mut GstPipeline);
            gst_bus_add_watch(bus, Some(bus_callback),
                              mem::transmute::<&mpsc::Sender<gui::GuiUpdateMessage>, gpointer>(
                                  &**self.gui_sender.get_ref()));
        }
        self.state = PlayState::NoUri;
        args2
    }

    pub fn set_uri(&mut self, uri: &str) {
        self.stop();
        unsafe {
            "uri".with_c_str(|property_c_str| {
                uri.with_c_str(|uri_c_str| {
                    g_object_set(self.playbin as gpointer,
                        property_c_str, uri_c_str, ptr::null::<gchar>());
                });
            });
        }
        self.state = PlayState::WaitToPlay;
    }

    pub fn play(&mut self) {
        match self.state {
            PlayState::Uninit => panic!("player is not initialized"),
            PlayState::NoUri => panic!("no uri set"),
            PlayState::Play => {
                info!("already playing");
                return;
            }
            _ => ()
        }
        unsafe {
            gst_element_set_state(self.playbin, GST_STATE_PLAYING);
        }
        self.state = PlayState::Play;
    }

    pub fn pause(&mut self) {
        match self.state {
            PlayState::Uninit => panic!("player is not initialized"),
            // NoUri -> we're not playing anyway.
            // Pause -> noop.
            PlayState::NoUri | PlayState::Pause => return,
            _ => ()
        }
        unsafe {
            gst_element_set_state(self.playbin, GST_STATE_PAUSED);
        }
        self.state = PlayState::Pause;
    }

    pub fn stop(&mut self) {
        match self.state {
            PlayState::Uninit => panic!("player is not initialized"),
            PlayState::NoUri => {
                warn!("already stopped");
                return;
            }
            _ => ()
        }
        self.stop_timers();
        unsafe {
            gst_element_set_state(self.playbin, GST_STATE_READY);
        }
        self.state = PlayState::NoUri;
    }

    pub fn toggle(&mut self) {
        match self.state {
            PlayState::Uninit => panic!("player is not initialized"),
            // NoUri -> don't do anything
            PlayState::NoUri => warn!("Uri is not set"),
            PlayState::Play | PlayState::WaitToPlay => self.pause(),
            PlayState::Pause => self.play()
        }
    }

    pub fn set_buffering(&mut self, is_buffering: bool) {
        match self.state {
            PlayState::Uninit => panic!("player is not initialized"),
            PlayState::NoUri => panic!("Uri is not set"),
            PlayState::Play if is_buffering => {
                unsafe {
                    gst_element_set_state(self.playbin, GST_STATE_PAUSED);
                }
                self.state = PlayState::WaitToPlay;
            }
            PlayState::WaitToPlay if !is_buffering => {
                unsafe {
                    gst_element_set_state(self.playbin, GST_STATE_PLAYING);
                }
                self.state = PlayState::Play;
            }
            _ => {
                // Nothing to do
            }
        }
    }

    pub fn is_playing(&self) -> bool {
        self.state == PlayState::Play || self.state == PlayState::WaitToPlay
    }

    pub fn start_timers(&mut self, sender: mpsc::Sender<gui::GuiUpdateMessage>) {
        let context = unsafe { g_main_context_default() };

        if self.report_timer.is_none() {
            let rc = Box::new(ReportCallback::new(sender.clone()));
            let mut rt = timerfd::TimerGSource::new(rc as Box<timerfd::TimerGSourceCallback+Send>);
            rt.attach(context);
            rt.mut_timer().set_oneshot(30 * 1000);
            self.report_timer = Some(rt);
        }
        self.report_timer.get_mut_ref().mut_timer().start();

        if self.progress_timer.is_none() {
            let pc = Box::new(ProgressCallback::new(sender));
            let mut pt = timerfd::TimerGSource::new(pc as Box<timerfd::TimerGSourceCallback+Send>);
            pt.attach(context);
            pt.mut_timer().set_interval(1, 1 * 1000);
            self.progress_timer = Some(pt);
        }
        self.progress_timer.get_mut_ref().mut_timer().start();
    }

    pub fn pause_timers(&mut self) {
        match self.report_timer {
            Some(ref mut rt) => rt.mut_timer().stop(),
            None => ()
        }
        match self.progress_timer {
            Some(ref mut pt) => pt.mut_timer().stop(),
            None => ()
        }
    }

    pub fn stop_timers(&mut self) {
        self.report_timer = None;
        self.progress_timer = None;
    }

    pub fn get_progress_info(&self) -> Option<(i64, i64)> {
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
            Some((current_position, current_duration))
        } else {
            None
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        if self.state != PlayState::Uninit {
            unsafe {
                if !self.playbin.is_null() {
                    gst_element_set_state(self.playbin, GST_STATE_NULL);
                    gst_object_unref(self.playbin as gpointer);
                }
                gst_deinit();
            }
        }
    }
}

extern "C" fn bus_callback(_bus: *mut GstBus, msg: *mut GstMessage, data: gpointer) -> gboolean {
    unsafe {
    let gui_sender = &*(data as *const mpsc::Sender<gui::GuiUpdateMessage>);

    let name = {
        let gst_obj = (*msg).src;
        if gst_obj.is_null() {
            "null-source".to_string()
        } else {
            let name_ptr = gst_object_get_name(gst_obj);
            if name_ptr.is_null() {
                "null-name".to_string()
            } else {
                let name = str::from_utf8(name_ptr as *const u8).unwrap().to_string();
                g_free(name_ptr as gpointer);
                name
            }
        }
    };

    match (*msg)._type {
        GST_MESSAGE_ERROR => {
            let mut err = ptr::null_mut();
            let mut dbg_info = ptr::null_mut();

            gst_message_parse_error(msg, &mut err, &mut dbg_info);


            let err_msg = str::from_utf8((*err).message as *const u8).unwrap();
            let info = str::from_utf8(dbg_info as *const u8).unwrap();

            error!("ERROR from element {}: {}", name, err_msg);
            error!("Debugging info: {}", info);

            gui_sender.send(gui::GuiUpdateMessage::Notify(format!("Playback error: `{}`", err_msg)));

            g_error_free(err);
            g_free(dbg_info as gpointer);
        }
        GST_MESSAGE_WARNING => {
            if log_enabled!(log::WARN) {
                let mut err = ptr::null_mut();
                let mut dbg_info = ptr::null_mut();

                gst_message_parse_error(msg, &mut err, &mut dbg_info);

                let err_msg = str::from_utf8((*err).message as *const u8).unwrap();
                let info = str::from_utf8(dbg_info as *const u8).unwrap();

                warn!("WARNING from element {}: {}", name, err_msg);
                warn!("Debugging info: {}", info);

                g_error_free(err);
                g_free(dbg_info as gpointer);
            }
        }
        GST_MESSAGE_INFO => {
            if log_enabled!(log::INFO) {
                let mut err = ptr::null_mut();
                let mut dbg_info = ptr::null_mut();

                gst_message_parse_error(msg, &mut err, &mut dbg_info);

                let err_msg = str::from_utf8((*err).message as *const u8).unwrap();
                let info = str::from_utf8(dbg_info as *const u8).unwrap();

                info!("INFO from element {}: {}", name, err_msg);
                info!("Debugging info: {}", info);

                g_error_free(err);
                g_free(dbg_info as gpointer);
            }
        }
        GST_MESSAGE_EOS => {
            debug!("EOS from element {}", name);
            gui_sender.send(gui::GuiUpdateMessage::NextTrack);
        }
        GST_MESSAGE_STATE_CHANGED => {
            if name.as_slice() == PLAYBIN_ELEMENT_NAME {
                let mut new_state = 0;
                gst_message_parse_state_changed(msg, ptr::null_mut(),
                    &mut new_state, ptr::null_mut());
                if log_enabled!(log::DEBUG) {
                    let new_state_name = gst_element_state_get_name(new_state);
                    let name = str::from_utf8(new_state_name as *const u8).unwrap();
                    debug!("new playbin state: {}", name);
                }
                match new_state {
                    GST_STATE_PLAYING => {
                        gui_sender.send(gui::GuiUpdateMessage::StartTimers);
                    }
                    GST_STATE_PAUSED => {
                        gui_sender.send(gui::GuiUpdateMessage::PauseTimers);
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
            info!("BUFFERING from element `{}`, {}%", name, percent);
            gui_sender.send(gui::GuiUpdateMessage::SetBuffering(percent < 100));
        }
        _ => {
            if log_enabled!(log::DEBUG) {
                let msg_type_cstr = gst_message_type_get_name((*msg)._type);
                let msg_type_name = str::from_utf8(msg_type_cstr as *const u8).unwrap();
                debug!("message of type `{}` from element `{}`", msg_type_name, name);
            }
        }
    }

    // Returning 0 removes this callback
    return 1;
    }
}
