use gtk::*;
use gtk::ffi::*;

struct Player {
    initialized: bool,
}

impl Player {
    pub fn new() -> Player {
        Player {
            initialized: false,
        }
    }

    pub fn init(&mut self, args: ~[~str]) -> ~[~str] {
        let args2 = unsafe {
            gst_init_with_args(args)
        };
        self.initialized = true;
        args2
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                gst_deinit();
            }
        }
    }
}
