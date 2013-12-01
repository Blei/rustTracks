use std::cast;
use std::ptr;
use std::str::raw::from_c_str;

use gtk::*;
use gtk::ffi::*;

use gui;

struct Player {
    initialized: bool,

    playbin: *mut GstElement,
}

impl Player {
    pub fn new() -> Player {
        Player {
            initialized: false,
            playbin: ptr::mut_null(),
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

    pub fn set_uri(&self, uri: &str) {
        self.stop();
        unsafe {
            "uri".with_c_str(|property_c_str| {
                uri.with_c_str(|uri_c_str| {
                    g_object_set(cast::transmute(self.playbin),
                        property_c_str, uri_c_str, ptr::null::<gchar>());
                });
            });
        }
    }

    pub fn play(&self) {
        if !self.initialized {
            fail!("player is not initialized");
        }
        unsafe {
            gst_element_set_state(self.playbin, GST_STATE_PLAYING);
        }
    }

    pub fn pause(&self) {
        if !self.initialized {
            fail!("player is not initialized");
        }
        unsafe {
            gst_element_set_state(self.playbin, GST_STATE_PAUSED);
        }
    }

    pub fn stop(&self) {
        if !self.initialized {
            fail!("player is not initialized");
        }
        unsafe {
            gst_element_set_state(self.playbin, GST_STATE_READY);
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                if !self.playbin.is_null() {
                    gst_object_unref(cast::transmute(self.playbin));
                }
                gst_deinit();
            }
        }
    }
}

extern "C" fn bus_callback(_bus: *mut GstBus, msg: *mut GstMessage, data: gpointer) -> gboolean {
    unsafe {
    let _gui: &gui::Gui = cast::transmute(data);

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

            println!("ERROR from element {}: {}", name,
                from_c_str(cast::transmute_immut_unsafe((*err).message)));
            println!("Debugging info: {}", from_c_str(cast::transmute_immut_unsafe(dbg_info)));

            g_error_free(err);
            g_free(cast::transmute(dbg_info));
        }
        GST_MESSAGE_WARNING => {
            let mut err = ptr::mut_null();
            let mut dbg_info = ptr::mut_null();

            gst_message_parse_error(msg, &mut err, &mut dbg_info);

            println!("WARNING from element {}: {}", name,
                from_c_str(cast::transmute_immut_unsafe((*err).message)));
            println!("Debugging info: {}", from_c_str(cast::transmute_immut_unsafe(dbg_info)));

            g_error_free(err);
            g_free(cast::transmute(dbg_info));
        }
        GST_MESSAGE_INFO => {
            let mut err = ptr::mut_null();
            let mut dbg_info = ptr::mut_null();

            gst_message_parse_error(msg, &mut err, &mut dbg_info);

            println!("INFO from element {}: {}", name,
                from_c_str(cast::transmute_immut_unsafe((*err).message)));
            println!("Debugging info: {}", from_c_str(cast::transmute_immut_unsafe(dbg_info)));

            g_error_free(err);
            g_free(cast::transmute(dbg_info));
        }
        GST_MESSAGE_EOS => {
            println!("EOS from element {}", name);
        }
        _ => println!("dropped bus message from element {}", name),
    }

    // Returning 0 removes this callback
    return 1;
    }
}
