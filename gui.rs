use std::cast;
use std::iter;
use std::mem;
use std::ptr;
use std::rt::comm;
use std::task;
use std::vec;

use extra::arc::RWArc;

use gtk::ffi::*;
use gtk::*;

use api;
use player;
use webinterface;

struct GuiGSource {
    g_source: GSource,
    gui_ptr: *Gui,
}

pub enum GuiUpdateMessage {
    FetchPlayToken,
    SetPlayToken(api::PlayToken),
    GetMixes(~str),
    UpdateMixes(~[api::Mix]),
    PlayMix(uint),
    SetUri(~str),
}

struct InnerGui {
    priv initialized: bool,
    priv running: bool,

    priv player: player::Player,

    priv mixes: ~[api::Mix],
    priv play_token: Option<api::PlayToken>,

    priv main_window: *mut GtkWidget,
    priv mixes_box: *mut GtkWidget,
}

pub struct Gui {
    priv ig: RWArc<InnerGui>,
    priv port: comm::Port<GuiUpdateMessage>,
    priv chan: comm::SharedChan<GuiUpdateMessage>,
    priv gui_g_source: *mut GuiGSource,
    priv g_source_funcs: GSourceFuncs,

    // this is such a hack...
    priv mix_index_table: ~[(*mut Gui, uint)],
}

impl Drop for Gui {
    fn drop(&mut self) {
        self.quit();
        if self.gui_g_source != ptr::mut_null() {
            unsafe {
                g_source_unref(cast::transmute::<*mut GuiGSource, *mut GSource>(self.gui_g_source));
            }
            self.gui_g_source = ptr::mut_null();
        }
    }
}

impl Gui {
    pub fn new() -> Gui {
        let (port, chan) = comm::stream();
        let inner_gui = InnerGui {
            initialized: false,
            running: false,
            player: player::Player::new(),
            mixes: ~[],
            play_token: None,
            main_window: ptr::mut_null(),
            mixes_box: ptr::mut_null(),
        };
        Gui {
            ig: RWArc::new(inner_gui),
            port: port,
            chan: comm::SharedChan::new(chan),
            gui_g_source: ptr::mut_null(),
            g_source_funcs: Struct__GSourceFuncs {
                prepare: prepare_gui_g_source,
                check: check_gui_g_source,
                dispatch: dispatch_gui_g_source,
                finalize: unsafe { cast::transmute(0) },
                closure_callback: unsafe { cast::transmute(0) },
                closure_marshal: unsafe{ cast::transmute(0) },
            },
            mix_index_table: ~[],
        }
    }

    pub fn init(&mut self, args: ~[~str]) {
        self.ig.write(|ig| {
            if !ig.initialized {
                unsafe {
                    let args2 = gtk_init_with_args(args.clone());
                    let _args3 = ig.player.init(args2, self);
                    ig.main_window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
                    gtk_window_set_default_size(cast::transmute(ig.main_window), 300, 400);
                    "destroy".with_c_str(|destroy| {
                        g_signal_connect(cast::transmute(ig.main_window),
                                         destroy,
                                         cast::transmute(close_button_pressed),
                                         cast::transmute::<&Gui, gpointer>(self));
                    });

                    let scrolled_window = gtk_scrolled_window_new(ptr::mut_null(), ptr::mut_null());
                    gtk_scrolled_window_set_policy(cast::transmute(scrolled_window),
                        GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
                    gtk_container_add(cast::transmute(ig.main_window), scrolled_window);

                    ig.mixes_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 5);
                    gtk_container_add(cast::transmute(scrolled_window), ig.mixes_box);

                    let g_source = g_source_new(cast::transmute(&self.g_source_funcs),
                                                mem::size_of::<GuiGSource>() as u32);
                    self.gui_g_source = cast::transmute::<*mut GSource, *mut GuiGSource>(g_source);
                    (*self.gui_g_source).gui_ptr = cast::transmute::<&Gui, *Gui>(self);
                };
                ig.initialized = true;
            }
        });
    }

    pub fn run(&self) {
        let needs_run = self.ig.write(|ig| {
            let needs_run = !ig.running;
            if needs_run {
                ig.running = true;
                unsafe {
                    gtk_widget_show_all(ig.main_window);
                    let context = g_main_context_default();
                    g_source_attach(cast::transmute::<*mut GuiGSource, *mut GSource>(self.gui_g_source),
                                    context);
                }
            }
            needs_run
        });
        if needs_run {
            unsafe {
                gtk_main();
            }
        }
    }

    pub fn quit(&self) {
        self.ig.write(|ig| {
            if ig.initialized {
                ig.player.stop();
                if ig.main_window != ptr::mut_null() {
                    unsafe {
                        gtk_widget_destroy(ig.main_window);
                    }
                    ig.main_window = ptr::mut_null();
                }
                unsafe {
                    g_source_destroy(cast::transmute::<*mut GuiGSource, *mut GSource>(self.gui_g_source));
                    gtk_main_quit();
                }
                ig.initialized = false;
            }
        });
    }

    pub fn initialized(&self) -> bool {
        self.ig.read(|ig| ig.initialized)
    }

    pub fn running(&self) -> bool {
        self.ig.read(|ig| ig.running)
    }

    fn fetch_play_token(&self) {
        if self.ig.read(|ig| ig.play_token.is_some()) {
            println!("play token already exists, ignoring request");
            return;
        }

        println!("fetching play token");
        let chan = self.chan.clone();
        do task::spawn_sched(task::SingleThreaded) {
            let pt_json = webinterface::get_play_token();
            let pt = api::parse_play_token_response(&pt_json);
            chan.send(SetPlayToken(pt.contents));
        }
    }

    fn set_play_token(&self, pt: api::PlayToken) {
        println!("setting play token to `{}`", *pt);
        self.ig.write(|ig| {
            ig.play_token = Some(pt.clone());
        });
    }

    fn set_mixes(&mut self, mixes: ~[api::Mix]) {
        self.mix_index_table = vec::from_fn(mixes.len(), |i| {
            (ptr::to_mut_unsafe_ptr(self), i)
        });
        self.ig.write(|ig| {
            // TODO why is this clone necessary?? blerg, once functions?
            ig.mixes = mixes.clone();
            println!("setting mixes, length {}", ig.mixes.len());
            unsafe {
                clear_gtk_container(cast::transmute(ig.mixes_box));
                for i in iter::range(0, mixes.len()) {
                    let mix_entry = create_mix_entry(&ig.mixes[i], &self.mix_index_table[i]);
                    gtk_box_pack_end(cast::transmute(ig.mixes_box),
                        mix_entry, 0, 1, 0);
                }
                gtk_widget_show_all(ig.mixes_box);
            }
        });
    }

    fn get_mixes(&self, smart_id: ~str) {
        println!("getting mixes for smart id '{}'", smart_id);
        let chan = self.get_chan().clone();
        do task::spawn_sched(task::SingleThreaded) {
            let mix_set_json = webinterface::get_mix_set(smart_id);
            let mix_set = api::parse_mix_set_response(&mix_set_json);
            chan.send(UpdateMixes(mix_set.contents.mixes));
        }
    }

    fn play_mix(&self, i: uint) {
        println!("playing mix with index {}", i);
        self.ig.read(|ig| {
            if i >= ig.mixes.len() {
                println!("index is out of bounds, ignoring message");
            } else {
                let mix = ig.mixes[i].clone();
                println!("playing mix with name `{}`", mix.name);
                let chan = self.chan.clone();
                let pt = ig.play_token.get_ref().clone();
                do task::spawn_sched(task::SingleThreaded) {
                    let play_state_json = webinterface::get_play_state(&pt, &mix);
                    let play_state = api::parse_play_state_response(&play_state_json);
                    chan.send(SetUri(play_state.contents.track.track_file_stream_url.clone()));
                }
            }
        });
    }

    fn set_uri(&self, uri: ~str) {
        println!("set uri to `{}`", uri);
        self.ig.read(|ig| {
            ig.player.set_uri(uri);
            ig.player.play();
        });
    }

    /// This can only be called from one thread at a time, not
    /// synchronized!!
    pub fn dispatch_message(&mut self) -> bool {
        if !self.port.peek() {
            return false;
        }

        match self.port.recv() {
            FetchPlayToken => self.fetch_play_token(),
            SetPlayToken(pt) => self.set_play_token(pt),
            UpdateMixes(m) => self.set_mixes(m),
            GetMixes(s) => self.get_mixes(s),
            PlayMix(i) => self.play_mix(i),
            SetUri(u) => self.set_uri(u),
        }

        return true;
    }

    /// This channel is synchronized, call it as often as you want
    pub fn get_chan<'a>(&'a self) -> &'a comm::SharedChan<GuiUpdateMessage> {
        &self.chan
    }
}

fn clear_gtk_container(container: *mut GtkContainer) {
    unsafe {
        let l = gtk_container_get_children(container);
        for ptr in GListIterator::new(&*l) {
            let widget: *mut GtkWidget = cast::transmute(ptr);
            gtk_widget_destroy(widget);
        }
        g_list_free(l);
    }
}

fn create_mix_entry(mix: &api::Mix, mix_table_entry: &(*mut Gui, uint)) -> *mut GtkWidget {
    unsafe {
        let box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 5);
        let label = mix.name.with_c_str(|c_str| {
            gtk_label_new(c_str)
        });
        gtk_container_add(cast::transmute(box), label);
        let button = "Play".with_c_str(|p| {
            gtk_button_new_with_label(p)
        });
        gtk_container_add(cast::transmute(box), button);
        "clicked".with_c_str(|c| {
            g_signal_connect(cast::transmute(button),
                             c,
                             cast::transmute(play_button_clicked),
                             cast::transmute::<&(*mut Gui, uint), gpointer>(mix_table_entry));
        });
        box
    }
}

unsafe fn get_gui_from_src(src: *mut GSource) -> & mut Gui {
    let gui_g_source = cast::transmute::<*mut GSource, *mut GuiGSource>(src);
    cast::transmute::<*Gui, &mut Gui>((*gui_g_source).gui_ptr)
}

extern "C" fn prepare_gui_g_source(src: *mut GSource, timeout: *mut gint) -> gboolean {
    unsafe {
        // Simplified: This is the amount of milliseconds between each call to this function.
        // This kind of simulates polling of the port, but meh, good enough for now.
        // FIXME: integrate ports into the main loop correctly
        *timeout = 40;
    }

    let gui = unsafe { get_gui_from_src(src) };
    if gui.port.peek() { 1 } else { 0 }
}

extern "C" fn check_gui_g_source(src: *mut GSource) -> gboolean {
    let gui = unsafe { get_gui_from_src(src) };
    if gui.port.peek() { 1 } else { 0 }
}

extern "C" fn dispatch_gui_g_source(src: *mut GSource,
        _callback: GSourceFunc, _user_data: gpointer) -> gboolean {
    let gui = unsafe { get_gui_from_src(src) };
    assert!(gui.port.peek());
    while gui.dispatch_message() { }

    // Returning 0 here would remove this GSource from the main loop
    return 1;
}

extern "C" fn close_button_pressed(_object: *GtkWidget, user_data: gpointer) {
    let gui: &Gui = unsafe { cast::transmute(user_data) };
    gui.quit();
}

extern "C" fn play_button_clicked(_button: *GtkButton, user_data: gpointer) {
    unsafe {
    let &(gui_ptr, i): &(*Gui, uint) = cast::transmute(user_data);
    let gui = &*gui_ptr;
    gui.get_chan().send(PlayMix(i));
    }
}
