use std::cast;
use std::logging;
use std::ptr;
use std::str::raw::from_c_str;
use std::task;

use gtk::*;
use gtk::ffi::*;

use gui;

struct Player {
    initialized: bool,

    playing: bool,

    playbin: *mut GstElement,
    clock_id: Option<GstClockID>,
}

impl Player {
    pub fn new() -> Player {
        Player {
            initialized: false,
            playing: false,
            playbin: ptr::mut_null(),
            clock_id: None,
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
                "rusttracks-playbin".with_c_str(|rtpb| {
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
        self.initialized = true;
        args2
    }

    pub fn set_uri(&mut self, uri: &str, gui: &gui::Gui) {
        self.stop();
        unsafe {
            "uri".with_c_str(|property_c_str| {
                uri.with_c_str(|uri_c_str| {
                    g_object_set(cast::transmute(self.playbin),
                        property_c_str, uri_c_str, ptr::null::<gchar>());
                });
            });

            let chan = gui.get_chan().clone();

            let clock = gst_pipeline_get_clock(cast::transmute(self.playbin));

            // in nanoseconds
            let timeout: guint64 = 30 * 1000 * 1000 * 1000;
            let target_time = gst_clock_get_time(clock) + timeout;

            let ci = gst_clock_new_single_shot_id(clock, target_time);
            self.clock_id = Some(ci);

            do task::spawn_sched(task::SingleThreaded) {
                let res = gst_clock_id_wait(ci, ptr::mut_null());
                match res {
                    GST_CLOCK_UNSCHEDULED => { } // Ignore, nothing to do
                    GST_CLOCK_OK => {
                        debug!("30s are up! sending ReportCurrentTrack to gui");
                        chan.send(gui::ReportCurrentTrack);
                    }
                    _ => unreachable!()
                }
            }

            gst_object_unref(cast::transmute(clock));
        }
    }

    pub fn play(&mut self) {
        if !self.initialized {
            fail!("player is not initialized");
        }
        unsafe {
            gst_element_set_state(self.playbin, GST_STATE_PLAYING);
        }
        self.playing = true;
    }

    pub fn pause(&mut self) {
        if !self.initialized {
            fail!("player is not initialized");
        }
        unsafe {
            gst_element_set_state(self.playbin, GST_STATE_PAUSED);
        }
        self.playing = false;
    }

    pub fn stop(&mut self) {
        if !self.initialized {
            fail!("player is not initialized");
        }
        unsafe {
            let maybe_ci = self.clock_id.take();
            for ci in maybe_ci.move_iter() {
                gst_clock_id_unschedule(ci);
                gst_clock_id_unref(ci);
            }
            gst_element_set_state(self.playbin, GST_STATE_READY);
        }
        self.playing = false;
    }

    pub fn toggle(&mut self) {
        if self.playing {
            self.pause()
        } else {
            self.play()
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        if self.initialized {
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

            error!("ERROR from element {}: {}", name,
                from_c_str(cast::transmute_immut_unsafe((*err).message)));
            error!("Debugging info: {}", from_c_str(cast::transmute_immut_unsafe(dbg_info)));

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
