use std::cast;
use std::iter;
use std::libc;
use std::mem;
use std::ptr;
use std::rt::comm;
use std::task;
use std::vec;
use vecraw = std::vec::raw;

use extra::arc::RWArc;

use gtk::ffi::*;
use gtk::*;

use api;
use player;
use webinterface;

static ICON_DATA: &'static [u8] = include_bin!("8tracks-icon.jpg");

fn get_icon_pixbuf() -> *mut GdkPixbuf {
    unsafe {
        let mut err = ptr::mut_null();
        let stream = g_memory_input_stream_new_from_data(
            vecraw::to_ptr(ICON_DATA) as *libc::c_void,
            ICON_DATA.len() as i64, cast::transmute(0));
        let pixbuf = gdk_pixbuf_new_from_stream(stream, ptr::mut_null(), &mut err);
        assert!(pixbuf != ptr::mut_null());
        g_input_stream_close(stream, ptr::mut_null(), &mut err);
        pixbuf
    }
}

static PLAY_ICON_NAME: &'static str = "media-playback-start";
static PAUSE_ICON_NAME: &'static str = "media-playback-pause";
static SKIP_ICON_NAME: &'static str = "media-skip-forward";

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
    PlayTrack(api::Track),
    ReportCurrentTrack,
    TogglePlaying,
    NextTrack,
    SkipTrack,
    SetPic(uint, ~[u8]),
    SetProgress(Option<(i64, i64)>),
    Notify(~str),
}

#[deriving(Clone)]
struct MixEntry {
    mix: api::Mix,

    widget: *mut GtkWidget,
    image: *mut GtkImage,
}

impl MixEntry {
    fn new(mix: api::Mix, mix_table_entry: &(*mut Gui, uint)) -> MixEntry {
        let (widget, image) = unsafe {
            let box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 5);

            let label = mix.name.with_c_str(|c_str| {
                gtk_label_new(c_str)
            });
            gtk_box_pack_start(cast::transmute(box), label, 1, 1, 0);
            gtk_label_set_line_wrap(label as *mut GtkLabel, 1);
            gtk_label_set_line_wrap_mode(label as *mut GtkLabel, PANGO_WRAP_WORD_CHAR);
            gtk_misc_set_alignment(label as *mut GtkMisc, 0f32, 0.5f32);

            let pixbuf1 = get_icon_pixbuf();
            let pixbuf2 = gdk_pixbuf_scale_simple(&*pixbuf1, 133, 133, GDK_INTERP_BILINEAR);
            gdk_pixbuf_unref(pixbuf1);
            let image = gtk_image_new_from_pixbuf(pixbuf2);
            gdk_pixbuf_unref(pixbuf2);
            gtk_box_pack_end(cast::transmute(box), image, 0, 0, 0);

            let button_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
            gtk_box_pack_end(cast::transmute(box), button_box, 0, 0, 0);

            let button = "Play".with_c_str(|p| {
                gtk_button_new_with_label(p)
            });
            gtk_box_pack_end(cast::transmute(button_box), button, 1, 0, 0);
            "clicked".with_c_str(|c| {
                g_signal_connect(cast::transmute(button),
                                 c,
                                 cast::transmute(play_button_clicked),
                                 cast::transmute::<&(*mut Gui, uint), gpointer>(mix_table_entry));
            });

            (box, image as *mut GtkImage)
        };

        MixEntry {
            mix: mix,
            widget: widget,
            image: image,
        }
    }

    // &mut is not strictly required, but kind of makes sense.
    // This is a "safe" interface after all
    fn set_pic(&mut self, pixbuf: *mut GdkPixbuf) {
        unsafe {
            gtk_image_set_from_pixbuf(self.image, pixbuf);
        }
    }
}

struct InnerGui {
    priv initialized: bool,
    priv running: bool,

    priv player: player::Player,

    priv mix_entries: ~[MixEntry],
    priv play_token: Option<api::PlayToken>,

    priv current_mix_index: Option<uint>,
    priv current_track: Option<api::Track>,

    priv main_window: *mut GtkWidget,
    priv mixes_box: *mut GtkWidget,
    priv toggle_button: *mut GtkWidget,
    priv skip_button: *mut GtkWidget,
    priv progress_bar: *mut GtkWidget,
    priv status_bar: *mut GtkWidget,
    priv status_bar_ci: Option<guint>,
}

impl InnerGui {
    fn control_buttons_set_sensitive(&self, sensitive: bool) {
        unsafe {
            gtk_widget_set_sensitive(self.toggle_button,
                if sensitive { 1 } else { 0 });
            gtk_widget_set_sensitive(self.skip_button,
                if sensitive { 1 } else { 0 });
        }
    }

    fn update_play_button_icon(&self) {
        let icon_name = if self.player.is_playing() {
            PAUSE_ICON_NAME
        } else {
            PLAY_ICON_NAME
        };
        unsafe {
            let image = icon_name.with_c_str(|cstr|
                gtk_image_new_from_icon_name(cstr, GTK_ICON_SIZE_BUTTON)
            );
            gtk_button_set_image(cast::transmute(self.toggle_button), image);
        }
    }

    fn set_progress(&mut self, progress: Option<(i64, i64)>) {
        match progress {
            Some((pos, dur)) => {
                let fraction = (pos as f64) / ((dur - 1) as f64);
                let pos_sec = pos / 1000000000;
                let dur_sec = dur / 1000000000;
                let text = format!("{}:{:02d} / {}:{:02d}",
                                   pos_sec / 60, pos_sec % 60,
                                   dur_sec / 60, dur_sec % 60);
                unsafe {
                    text.with_c_str(|cstr|
                        gtk_progress_bar_set_text(cast::transmute(self.progress_bar), cstr)
                    );
                    gtk_progress_bar_set_fraction(cast::transmute(self.progress_bar), fraction);
                }
            }
            None => {
                unsafe {
                    "".with_c_str(|cstr|
                        gtk_progress_bar_set_text(cast::transmute(self.progress_bar), cstr)
                    );
                    gtk_progress_bar_set_fraction(cast::transmute(self.progress_bar), 0.);
                }
            }
        }
    }
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
            mix_entries: ~[],
            play_token: None,
            current_mix_index: None,
            current_track: None,
            main_window: ptr::mut_null(),
            mixes_box: ptr::mut_null(),
            toggle_button: ptr::mut_null(),
            skip_button: ptr::mut_null(),
            progress_bar: ptr::mut_null(),
            status_bar: ptr::mut_null(),
            status_bar_ci: None,
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
                    gtk_window_set_default_size(cast::transmute(ig.main_window), 400, 500);
                    "destroy".with_c_str(|destroy| {
                        g_signal_connect(cast::transmute(ig.main_window),
                                         destroy,
                                         cast::transmute(close_button_pressed),
                                         cast::transmute::<&Gui, gpointer>(self));
                    });
                    let icon = get_icon_pixbuf();
                    gtk_window_set_icon(cast::transmute(ig.main_window), icon);
                    gdk_pixbuf_unref(icon);

                    let main_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 5);
                    gtk_container_add(cast::transmute(ig.main_window), main_box);

                    let control_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
                    gtk_container_add(cast::transmute(main_box), control_box);

                    ig.toggle_button = PAUSE_ICON_NAME.with_c_str(|cstr|
                        gtk_button_new_from_icon_name(cstr, GTK_ICON_SIZE_BUTTON)
                    );
                    "clicked".with_c_str(|clicked| {
                        g_signal_connect(cast::transmute(ig.toggle_button),
                                         clicked,
                                         cast::transmute(toggle_button_clicked),
                                         cast::transmute::<&Gui, gpointer>(self));
                    });
                    gtk_box_pack_start(cast::transmute(control_box), ig.toggle_button, 0, 0, 0);

                    ig.skip_button = SKIP_ICON_NAME.with_c_str(|cstr|
                        gtk_button_new_from_icon_name(cstr, GTK_ICON_SIZE_BUTTON)
                    );
                    "clicked".with_c_str(|clicked| {
                        g_signal_connect(cast::transmute(ig.skip_button),
                                         clicked,
                                         cast::transmute(skip_button_clicked),
                                         cast::transmute::<&Gui, gpointer>(self));
                    });
                    gtk_box_pack_start(cast::transmute(control_box), ig.skip_button, 0, 0, 0);

                    ig.progress_bar = gtk_progress_bar_new();
                    gtk_box_pack_end(cast::transmute(control_box), ig.progress_bar, 1, 1, 0);
                    "".with_c_str(|cstr|
                        gtk_progress_bar_set_text(cast::transmute(ig.progress_bar), cstr)
                    );
                    gtk_progress_bar_set_show_text(cast::transmute(ig.progress_bar), 1);

                    ig.control_buttons_set_sensitive(false);

                    let scrolled_window = gtk_scrolled_window_new(ptr::mut_null(), ptr::mut_null());
                    gtk_scrolled_window_set_policy(cast::transmute(scrolled_window),
                        GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
                    gtk_box_pack_start(cast::transmute(main_box), scrolled_window, 1, 1, 0);

                    ig.mixes_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 5);
                    gtk_container_add(cast::transmute(scrolled_window), ig.mixes_box);

                    ig.status_bar = gtk_statusbar_new();
                    ig.status_bar_ci = "rusttracks".with_c_str(|cstr|
                        Some(gtk_statusbar_get_context_id(cast::transmute(ig.status_bar), cstr))
                    );
                    gtk_box_pack_start(cast::transmute(main_box), ig.status_bar, 0, 0, 0);

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

    pub fn notify(&self, message: &str) {
        if !self.initialized() {
            warn!("Not initialized, message `{}` is ignored", message);
            return
        }

        unsafe {
            self.ig.read(|ig| {
                message.with_c_str(|cstr|
                    gtk_statusbar_push(cast::transmute(ig.status_bar),
                                       *ig.status_bar_ci.get_ref(), cstr)
                );
            });
        }
    }

    fn fetch_play_token(&self) {
        if self.ig.read(|ig| ig.play_token.is_some()) {
            debug!("play token already exists, ignoring request");
            return;
        }

        debug!("fetching play token");
        let chan = self.chan.clone();
        do task::spawn_sched(task::SingleThreaded) {
            let pt_json = webinterface::get_play_token();
            let pt = api::parse_play_token_response(&pt_json);
            chan.send(SetPlayToken(pt.contents));
        }
    }

    fn set_play_token(&self, pt: api::PlayToken) {
        debug!("setting play token to `{}`", *pt);
        self.ig.write(|ig| {
            ig.play_token = Some(pt.clone());
        });
    }

    fn set_mixes(&mut self, mixes: ~[api::Mix]) {
        self.mix_index_table = vec::from_fn(mixes.len(), |i| {
            (ptr::to_mut_unsafe_ptr(self), i)
        });
        self.ig.write(|ig| {
            ig.mix_entries.clear();
            debug!("setting mixes, length {}", mixes.len());
            unsafe {
                clear_gtk_container(cast::transmute(ig.mixes_box));
                for i in iter::range(0, mixes.len()) {
                    let mix_entry = MixEntry::new(mixes[i].clone(), &self.mix_index_table[i]);
                    gtk_box_pack_start(cast::transmute(ig.mixes_box),
                        mix_entry.widget, 0, 1, 0);
                    ig.mix_entries.push(mix_entry);

                    // Fetch cover pic
                    let chan = self.chan.clone();
                    let pic_url_str = mixes[i].cover_urls.sq133.clone();
                    do task::spawn_sched(task::SingleThreaded) {
                        let pic_data = webinterface::get_data_from_url_str(pic_url_str);
                        chan.send(SetPic(i, pic_data));
                    }
                }
                gtk_widget_show_all(ig.mixes_box);
            }
        });
    }

    fn get_mixes(&self, smart_id: ~str) {
        debug!("getting mixes for smart id '{}'", smart_id);
        let chan = self.get_chan().clone();
        do task::spawn_sched(task::SingleThreaded) {
            let mix_set_json = webinterface::get_mix_set(smart_id);
            let mix_set = api::parse_mix_set_response(&mix_set_json);
            chan.send(UpdateMixes(mix_set.contents.mixes));
        }
    }

    fn play_mix(&self, i: uint) {
        debug!("playing mix with index {}", i);
        self.ig.write(|ig| {
            if i >= ig.mix_entries.len() {
                warn!("index is out of bounds, ignoring message");
            } else {
                ig.current_mix_index = Some(i);
                let mix = ig.mix_entries[i].mix.clone();
                debug!("playing mix with name `{}`", mix.name);
                let chan = self.chan.clone();
                let pt = ig.play_token.get_ref().clone();
                do task::spawn_sched(task::SingleThreaded) {
                    let play_state_json = webinterface::get_play_state(&pt, &mix);
                    let play_state = api::parse_play_state_response(&play_state_json);
                    chan.send(PlayTrack(play_state.contents.track));
                }
            }
        });
    }

    fn play_track(&self, track: api::Track) {
        self.ig.write(|ig| {
            debug!("playing track `{}`", track.name);
            ig.current_track = Some(track.clone());
            debug!("setting uri to `{}`", track.track_file_stream_url);
            ig.player.set_uri(track.track_file_stream_url, self);
            ig.player.play();
            ig.update_play_button_icon();
            ig.control_buttons_set_sensitive(true);
            ig.set_progress(None);
        });
    }

    fn report_current_track(&self) {
        debug!("reporting current track");
        let (pt, ti, mi) = self.ig.read(|ig|
            (
                ig.play_token.get_ref().clone(),
                ig.mix_entries[*ig.current_mix_index.get_ref()].mix.id,
                ig.current_track.get_ref().id,
            )
        );
        do task::spawn_sched(task::SingleThreaded) {
            webinterface::report_track(&pt, ti, mi);
        }
    }

    fn toggle_playing(&self) {
        debug!("toggling!");
        self.ig.write(|ig| {
            ig.player.toggle();
            ig.update_play_button_icon();
        });
    }

    fn next_track(&self) {
        self.ig.write(|ig| {
            ig.player.stop();
            ig.current_track = None;
            ig.control_buttons_set_sensitive(false);

            let i = ig.current_mix_index.unwrap();
            let mix = ig.mix_entries[i].mix.clone();
            debug!("getting next track of mix with name `{}`", mix.name);
            let chan = self.chan.clone();
            let pt = ig.play_token.get_ref().clone();
            do task::spawn_sched(task::SingleThreaded) {
                let next_track_json = webinterface::get_next_track(&pt, &mix);
                let play_state = api::parse_play_state_response(&next_track_json);
                match play_state.contents {
                    Some(ps) => chan.send(PlayTrack(ps.track)),
                    None => chan.send(Notify(~"Next track could not be obtained"))
                }
            }
        });
    }

    fn skip_track(&self) {
        self.ig.write(|ig| {
            ig.player.pause();
            ig.update_play_button_icon();

            let i = ig.current_mix_index.unwrap();
            let mix = ig.mix_entries[i].mix.clone();
            debug!("skipping track of mix with name `{}`", mix.name);
            let chan = self.chan.clone();
            let pt = ig.play_token.get_ref().clone();
            do task::spawn_sched(task::SingleThreaded) {
                let skip_track_json = webinterface::get_skip_track(&pt, &mix);
                let play_state = api::parse_play_state_response(&skip_track_json);
                chan.send(PlayTrack(play_state.contents.track));
            }
        });
    }

    fn set_pic(&self, i: uint, mut pic_data: ~[u8]) {
        let pixbuf = unsafe {
            let mut err = ptr::mut_null();

            let stream = g_memory_input_stream_new_from_data(
                vecraw::to_mut_ptr(pic_data) as *libc::c_void,
                pic_data.len() as i64, cast::transmute(0));
            let pixbuf = gdk_pixbuf_new_from_stream(stream, ptr::mut_null(), &mut err);

            g_input_stream_close(stream, ptr::mut_null(), &mut err);
            pixbuf
        };

        self.ig.write(|ig| {
            if i >= ig.mix_entries.len() {
                warn!("set_pic: index {} is out of range, only {} mix_entries",
                      i, ig.mix_entries.len());
            } else {
                ig.mix_entries[i].set_pic(pixbuf);
            }
        });


        unsafe {
            gdk_pixbuf_unref(pixbuf);
        }
    }

    fn set_progress(&self, progress: Option<(i64, i64)>) {
        debug!("setting progress to {:?}", progress);
        self.ig.write(|ig| {
            ig.set_progress(progress);
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
            PlayTrack(t) => self.play_track(t),
            ReportCurrentTrack => self.report_current_track(),
            TogglePlaying => self.toggle_playing(),
            NextTrack => self.next_track(),
            SkipTrack => self.skip_track(),
            SetPic(i, d) => self.set_pic(i, d),
            SetProgress(p) => self.set_progress(p),
            Notify(m) => self.notify(m),
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

extern "C" fn toggle_button_clicked(_button: *GtkButton, user_data: gpointer) {
    unsafe {
    let gui: &Gui = cast::transmute(user_data);
    gui.get_chan().send(TogglePlaying);
    }
}

extern "C" fn skip_button_clicked(_button: *GtkButton, user_data: gpointer) {
    unsafe {
    let gui: &Gui = cast::transmute(user_data);
    gui.get_chan().send(SkipTrack);
    }
}
