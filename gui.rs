use libc;

use std::cast;
use std::iter;
use std::mem;
use std::ptr;
use std::comm;
use strraw = std::str::raw;

use gtk::ffi::*;
use gtk::*;

use api;
use player;
use webinterface;

fn as_box<T>(in_ptr: *mut T) -> *mut GtkBox {
    in_ptr as *mut GtkBox
}

static ICON_DATA: &'static [u8] = include_bin!("8tracks-icon.jpg");

fn get_icon_pixbuf() -> *mut GdkPixbuf {
    unsafe {
        let mut err = ptr::mut_null();
        let stream = g_memory_input_stream_new_from_data(
            ICON_DATA.as_ptr() as *libc::c_void,
            ICON_DATA.len() as i64, None);
        let pixbuf = gdk_pixbuf_new_from_stream(stream, ptr::mut_null(), &mut err);
        assert!(pixbuf != ptr::mut_null());
        g_input_stream_close(stream, ptr::mut_null(), &mut err);
        pixbuf
    }
}

static PLAY_ICON_NAME: &'static str = "media-playback-start";
static PAUSE_ICON_NAME: &'static str = "media-playback-pause";
static SKIP_ICON_NAME: &'static str = "media-skip-forward";

enum MixesOrdering {
    Popular = 0,
    New = 1,
}

struct GuiGSource {
    g_source: GSource,
    gui_ptr: *mut Gui,
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
    SetBuffering(bool),
    NextTrack,
    SkipTrack,
    SetPic(uint, Vec<u8>),
    SetProgress(Option<(i64, i64)>),
    Notify(~str),
    StartTimers,
    PauseTimers,
}

struct MixEntry {
    mix: api::Mix,

    widget: *mut GtkWidget,
    image: *mut GtkImage,
}

impl MixEntry {
    fn new(mix: api::Mix, mix_table_entry: &(*mut Gui, uint)) -> MixEntry {
        let (widget, image) = unsafe {
            let entry_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 5);

            let label = mix.name.with_c_str(|c_str| {
                gtk_label_new(c_str)
            });
            gtk_box_pack_start(as_box(entry_box), label, 1, 1, 0);
            gtk_label_set_line_wrap(label as *mut GtkLabel, 1);
            gtk_label_set_line_wrap_mode(label as *mut GtkLabel, PANGO_WRAP_WORD_CHAR);
            gtk_misc_set_alignment(label as *mut GtkMisc, 0f32, 0.5f32);

            let pixbuf1 = get_icon_pixbuf();
            let pixbuf2 = gdk_pixbuf_scale_simple(&*pixbuf1, 133, 133, GDK_INTERP_BILINEAR);
            gdk_pixbuf_unref(pixbuf1);
            let image = gtk_image_new_from_pixbuf(pixbuf2);
            gdk_pixbuf_unref(pixbuf2);
            gtk_box_pack_end(as_box(entry_box), image, 0, 0, 0);

            let button_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
            gtk_box_pack_end(as_box(entry_box), button_box, 0, 0, 0);

            let button = "Play".with_c_str(|p| {
                gtk_button_new_with_label(p)
            });
            gtk_box_pack_end(as_box(button_box), button, 1, 0, 0);
            "clicked".with_c_str(|c| {
                g_signal_connect(button as gpointer,
                                 c,
                                 Some(cast::transmute(play_button_clicked)),
                                 cast::transmute::<&(*mut Gui, uint), gpointer>(mix_table_entry));
            });

            (entry_box, image as *mut GtkImage)
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

pub struct Gui {
    initialized: bool,
    running: bool,

    mix_entries: Vec<MixEntry>,
    play_token: Option<api::PlayToken>,

    current_mix_index: Option<uint>,
    current_track: Option<api::Track>,

    main_window: *mut GtkWidget,
    mixes_scrolled_window: *mut GtkWidget,
    mixes_box: *mut GtkWidget,
    toggle_button: *mut GtkWidget,
    skip_button: *mut GtkWidget,
    progress_bar: *mut GtkWidget,
    info_label: *mut GtkWidget,
    status_bar: *mut GtkWidget,
    status_bar_ci: Option<guint>,

    receiver: comm::Receiver<GuiUpdateMessage>,
    sender: comm::Sender<GuiUpdateMessage>,
    buffered_msg: Option<GuiUpdateMessage>,

    gui_g_source: *mut GuiGSource,
    g_source_funcs: GSourceFuncs,

    player: player::Player,

    // this is such a hack...
    mix_index_table: Vec<(*mut Gui, uint)>,
}

#[unsafe_destructor]
impl Drop for Gui {
    fn drop(&mut self) {
        self.quit();
        if self.gui_g_source != ptr::mut_null() {
            unsafe {
                g_source_unref(self.gui_g_source as *mut GSource);
            }
            self.gui_g_source = ptr::mut_null();
        }
    }
}

impl Gui {
    pub fn new() -> Gui {
        let (sender, receiver) = comm::channel();
        Gui {
            initialized: false,
            running: false,
            mix_entries: Vec::new(),
            play_token: None,
            current_mix_index: None,
            current_track: None,
            main_window: ptr::mut_null(),
            mixes_scrolled_window: ptr::mut_null(),
            mixes_box: ptr::mut_null(),
            toggle_button: ptr::mut_null(),
            skip_button: ptr::mut_null(),
            progress_bar: ptr::mut_null(),
            info_label: ptr::mut_null(),
            status_bar: ptr::mut_null(),
            status_bar_ci: None,
            receiver: receiver,
            sender: sender,
            buffered_msg: None,
            gui_g_source: ptr::mut_null(),
            g_source_funcs: Struct__GSourceFuncs {
                prepare: Some(prepare_gui_g_source),
                check: Some(check_gui_g_source),
                dispatch: Some(dispatch_gui_g_source),
                finalize: None,
                closure_callback: None,
                closure_marshal: None,
            },
            player: player::Player::new(),
            mix_index_table: Vec::new(),
        }
    }

    fn control_buttons_set_sensitive(&mut self, sensitive: bool) {
        unsafe {
            gtk_widget_set_sensitive(self.toggle_button,
                if sensitive { 1 } else { 0 });
            gtk_widget_set_sensitive(self.skip_button,
                if sensitive { 1 } else { 0 });
        }
    }

    fn update_play_button_icon(&mut self) {
        let icon_name = if self.player.is_playing() {
            PAUSE_ICON_NAME
        } else {
            PLAY_ICON_NAME
        };
        unsafe {
            let image = icon_name.with_c_str(|cstr|
                gtk_image_new_from_icon_name(cstr, GTK_ICON_SIZE_BUTTON)
            );
            gtk_button_set_image(self.toggle_button as *mut GtkButton, image);
        }
    }

    fn set_current_track(&mut self, track: api::Track) {
        self.current_track = Some(track);
        self.update_track_info();
    }

    fn remove_current_track(&mut self) {
        self.current_track = None;
        self.update_track_info();
    }

    fn update_track_info(&mut self) {
        match self.current_track {
            None => {
                unsafe {
                    "".with_c_str(|cstr|
                        gtk_label_set_text(self.info_label as *mut GtkLabel, cstr)
                    );
                }
            }
            Some(ref track) => {
                let mut text = StrBuf::new();
                text.push_str(format!("'{}' by {}", track.name, track.performer));
                match track.release_name {
                    Some(ref rn) => {
                        text.push_str(format!("\nAlbum: {}", *rn));
                        match track.year {
                            Some(year) => text.push_str(format!(" ({})", year)),
                            None => ()
                        }
                    }
                    None => ()
                }
                unsafe {
                    text.as_slice().with_c_str(|cstr|
                        gtk_label_set_text(self.info_label as *mut GtkLabel, cstr)
                    );
                }
            }
        }
    }

    pub fn init(&mut self, args: ~[~str]) {
        if !self.initialized {
            let args2;
            unsafe {
                args2 = gtk_init_with_args(args.clone());
                self.main_window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
                gtk_window_set_default_size(self.main_window as *mut GtkWindow, 400, 500);
                "destroy".with_c_str(|destroy| {
                    g_signal_connect(self.main_window as gpointer,
                                     destroy,
                                     Some(cast::transmute(close_button_pressed)),
                                     cast::transmute::<&Gui, gpointer>(self));
                });
                let icon = get_icon_pixbuf();
                gtk_window_set_icon(self.main_window as *mut GtkWindow, icon);
                gdk_pixbuf_unref(icon);

                let main_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 5);
                gtk_container_add(self.main_window as *mut GtkContainer, main_box);

                let control_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
                gtk_container_add(main_box as *mut GtkContainer, control_box);

                self.toggle_button = PAUSE_ICON_NAME.with_c_str(|cstr|
                    gtk_button_new_from_icon_name(cstr, GTK_ICON_SIZE_BUTTON)
                );
                "clicked".with_c_str(|clicked| {
                    g_signal_connect(self.toggle_button as gpointer,
                                     clicked,
                                     Some(cast::transmute(toggle_button_clicked)),
                                     cast::transmute::<&Gui, gpointer>(self));
                });
                gtk_box_pack_start(control_box as *mut GtkBox, self.toggle_button, 0, 0, 0);

                self.skip_button = SKIP_ICON_NAME.with_c_str(|cstr|
                    gtk_button_new_from_icon_name(cstr, GTK_ICON_SIZE_BUTTON)
                );
                "clicked".with_c_str(|clicked| {
                    g_signal_connect(self.skip_button as gpointer,
                                     clicked,
                                     Some(cast::transmute(skip_button_clicked)),
                                     cast::transmute::<&Gui, gpointer>(self));
                });
                gtk_box_pack_start(as_box(control_box), self.skip_button, 0, 0, 0);

                self.progress_bar = gtk_progress_bar_new();
                gtk_box_pack_end(as_box(control_box), self.progress_bar, 1, 1, 0);
                "".with_c_str(|cstr|
                    gtk_progress_bar_set_text(self.progress_bar as *mut GtkProgressBar, cstr)
                );
                gtk_progress_bar_set_show_text(self.progress_bar as *mut GtkProgressBar, 1);

                self.control_buttons_set_sensitive(false);

                self.info_label = gtk_label_new(ptr::null());
                gtk_box_pack_start(as_box(main_box), self.info_label, 0, 0, 0);
                gtk_label_set_justify(self.info_label as *mut GtkLabel, GTK_JUSTIFY_CENTER);

                self.mixes_scrolled_window = gtk_scrolled_window_new(ptr::mut_null(),
                                                                   ptr::mut_null());
                gtk_scrolled_window_set_policy(self.mixes_scrolled_window as *mut GtkScrolledWindow,
                    GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
                gtk_box_pack_start(as_box(main_box), self.mixes_scrolled_window, 1, 1, 0);

                self.mixes_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 5);
                gtk_container_add(self.mixes_scrolled_window as *mut GtkContainer, self.mixes_box);

                let smart_id_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 5);
                gtk_box_pack_start(as_box(main_box), smart_id_box, 0, 0, 0);

                let smart_id_ordering_combo = gtk_combo_box_text_new();
                gtk_box_pack_start(as_box(smart_id_box), smart_id_ordering_combo, 0, 0, 0);
                "popular".with_c_str(|cstr|
                    gtk_combo_box_text_append(smart_id_ordering_combo as *mut GtkComboBoxText,
                                     ptr::null(), cstr)
                );
                "new".with_c_str(|cstr|
                    gtk_combo_box_text_append(smart_id_ordering_combo as *mut GtkComboBoxText,
                                     ptr::null(), cstr)
                );
                gtk_combo_box_set_active(smart_id_ordering_combo as *mut GtkComboBox,
                                         Popular as libc::c_int);

                let smart_id_entry = gtk_entry_new();
                gtk_box_pack_start(as_box(smart_id_box), smart_id_entry, 1, 1, 0);
                "activate".with_c_str(|cstr|
                    g_signal_connect(smart_id_entry as gpointer,
                                     cstr,
                                     Some(cast::transmute(smart_id_entry_activated)),
                                     cast::transmute::<&Gui, gpointer>(self))
                );

                self.status_bar = gtk_statusbar_new();
                self.status_bar_ci = "rusttracks".with_c_str(|cstr|
                    Some(gtk_statusbar_get_context_id(self.status_bar as *mut GtkStatusbar, cstr))
                );
                gtk_box_pack_start(as_box(main_box), self.status_bar, 0, 0, 0);

                let g_source = g_source_new(&mut self.g_source_funcs as *mut GSourceFuncs,
                                            mem::size_of::<GuiGSource>() as guint);
                self.gui_g_source = g_source as *mut GuiGSource;
                (*self.gui_g_source).gui_ptr = self as *mut Gui;
            }
            self.initialized = true;
            let sender = self.get_sender().clone();
            let _args3 = self.player.init(args2, sender);
        }
    }

    pub fn run(&mut self) {
        if !self.running {
            self.running = true;
            unsafe {
                gtk_widget_show_all(self.main_window);
                let context = g_main_context_default();
                g_source_attach(self.gui_g_source as *mut GSource,
                                context);
                gtk_main();
            }
        }
    }

    pub fn quit(&mut self) {
        if self.initialized {
            self.player.stop();
            {
                if self.main_window != ptr::mut_null() {
                    unsafe {
                        gtk_widget_destroy(self.main_window);
                    }
                    self.main_window = ptr::mut_null();
                }
                unsafe {
                    g_source_destroy(self.gui_g_source as *mut GSource);
                    gtk_main_quit();
                }
                self.initialized = false;
            }
        }
    }

    pub fn initialized(&self) -> bool {
        self.initialized
    }

    pub fn notify(&self, message: &str) {
        if !self.initialized() {
            warn!("Not initialized, message `{}` is ignored", message);
            return
        }

        unsafe {
            message.with_c_str(|cstr|
                gtk_statusbar_push(self.status_bar as *mut GtkStatusbar,
                                   *self.status_bar_ci.get_ref(), cstr)
            );
        }
    }

    fn fetch_play_token(&self) {
        if self.play_token.is_some() {
            debug!("play token already exists, ignoring request");
            return;
        }

        debug!("fetching play token");
        let sender = self.sender.clone();
        spawn(proc() {
            let pt_json = webinterface::get_play_token();
            let pt = api::parse_play_token_response(&pt_json);
            match pt.contents {
                Some(pt) => sender.send(SetPlayToken(pt)),
                None => sender.send(Notify(~"Playtoken could not be obtained"))
            }
        });
    }

    fn set_play_token(&mut self, pt: api::PlayToken) {
        debug!("setting play token to `{}`", pt.s);
        self.play_token = Some(pt.clone());
    }

    fn set_mixes(&mut self, mixes: ~[api::Mix]) {
        self.mix_index_table = Vec::from_fn(mixes.len(), |i| {
            (self as *mut Gui, i)
        });
        self.mix_entries.clear();
        debug!("setting mixes, length {}", mixes.len());
        unsafe {
            clear_gtk_container(self.mixes_box as *mut GtkContainer);
            for i in iter::range(0, mixes.len()) {
                let mix_entry = MixEntry::new(mixes[i].clone(), self.mix_index_table.get(i));
                gtk_box_pack_start(as_box(self.mixes_box),
                    mix_entry.widget, 0, 1, 0);
                self.mix_entries.push(mix_entry);

                // Fetch cover pic
                let sender = self.sender.clone();
                let pic_url_str = mixes[i].cover_urls.sq133.clone();
                spawn(proc() {
                    let pic_data = webinterface::get_data_from_url_str(pic_url_str);
                    sender.send(SetPic(i, pic_data));
                });
            }
            gtk_widget_show_all(self.mixes_box);
            let adj = gtk_scrolled_window_get_vadjustment(
                self.mixes_scrolled_window as *mut GtkScrolledWindow);
            let lower = gtk_adjustment_get_lower(adj);
            gtk_adjustment_set_value(adj, lower);
        }
    }

    fn get_mixes(&self, smart_id: ~str) {
        debug!("getting mixes for smart id '{}'", smart_id);
        let sender = self.get_sender().clone();
        spawn(proc() {
            let mix_set_json = webinterface::get_mix_set(smart_id);
            let mix_set = api::parse_mix_set_response(&mix_set_json);
            match mix_set.contents {
                Some(ms) => sender.send(UpdateMixes(ms.mixes)),
                None => sender.send(Notify(~"Mix list could not be obtained"))
            }
        });
    }

    fn play_mix(&mut self, i: uint) {
        debug!("playing mix with index {}", i);
        if i >= self.mix_entries.len() {
            warn!("index is out of bounds, ignoring message");
        } else {
            self.current_mix_index = Some(i);
            let mix = self.mix_entries.get(i).mix.clone();
            debug!("playing mix with name `{}`", mix.name);
            let sender = self.sender.clone();
            let pt = self.play_token.get_ref().clone();
            spawn(proc() {
                let play_state_json = webinterface::get_play_state(&pt, &mix);
                let play_state = api::parse_play_state_response(&play_state_json);
                match play_state.contents {
                    Some(ps) => sender.send(PlayTrack(ps.track)),
                None => sender.send(Notify(~"Could not start playing mix"))
                }
            });
        }
    }

    fn play_track(&mut self, track: api::Track) {
        debug!("playing track `{}`", track.name);
        self.set_current_track(track.clone());
        debug!("setting uri to `{}`", track.track_file_stream_url);
        self.player.set_uri(track.track_file_stream_url);
        self.player.play();
        self.update_play_button_icon();
        self.control_buttons_set_sensitive(true);
        self.set_progress(None);
    }

    fn report_current_track(&self) {
        debug!("reporting current track");
        let (pt, ti, mi) =
            (
                self.play_token.get_ref().clone(),
                self.mix_entries.get(*self.current_mix_index.get_ref()).mix.id,
                self.current_track.get_ref().id,
            );
        spawn(proc() {
            webinterface::report_track(&pt, ti, mi);
        });
    }

    fn toggle_playing(&mut self) {
        debug!("toggling!");
        self.player.toggle();
        self.update_play_button_icon();
    }

    fn set_buffering(&mut self, is_buffering: bool) {
        debug!("set_buffering({})", is_buffering);
        self.player.set_buffering(is_buffering);
        self.update_play_button_icon();
    }

    fn next_track(&mut self) {
        self.player.stop();
        self.remove_current_track();
        self.control_buttons_set_sensitive(false);

        let i = self.current_mix_index.unwrap();
        let mix = self.mix_entries.get(i).mix.clone();
        debug!("getting next track of mix with name `{}`", mix.name);
        let sender = self.sender.clone();
        let pt = self.play_token.get_ref().clone();
        spawn(proc() {
            let next_track_json = webinterface::get_next_track(&pt, &mix);
            let play_state = api::parse_play_state_response(&next_track_json);
            match play_state.contents {
                Some(ps) => sender.send(PlayTrack(ps.track)),
                None => sender.send(Notify(~"Next track could not be obtained"))
            }
        });
    }

    fn skip_track(&mut self) {
        self.player.pause();
        self.update_play_button_icon();

        let i = self.current_mix_index.unwrap();
        let mix = self.mix_entries.get(i).mix.clone();
        debug!("skipping track of mix with name `{}`", mix.name);
        let sender = self.sender.clone();
        let pt = self.play_token.get_ref().clone();
        spawn(proc() {
            let skip_track_json = webinterface::get_skip_track(&pt, &mix);
            let play_state = api::parse_play_state_response(&skip_track_json);
            match play_state.contents {
                Some(ps) => sender.send(PlayTrack(ps.track)),
                None => sender.send(Notify(~"Could not skip track"))
            }
        });
    }

    fn set_pic(&mut self, i: uint, pic_data: Vec<u8>) {
        let pixbuf = unsafe {
            let mut err = ptr::mut_null();

            let stream = g_memory_input_stream_new_from_data(
                pic_data.as_ptr() as *libc::c_void,
                pic_data.len() as i64, None);
            let pixbuf = gdk_pixbuf_new_from_stream(stream, ptr::mut_null(), &mut err);

            g_input_stream_close(stream, ptr::mut_null(), &mut err);
            pixbuf
        };

        if i >= self.mix_entries.len() {
            warn!("set_pic: index {} is out of range, only {} mix_entries",
                  i, self.mix_entries.len());
        } else {
            self.mix_entries.get_mut(i).set_pic(pixbuf);
        }

        unsafe {
            gdk_pixbuf_unref(pixbuf);
        }
    }

    fn start_timers(&mut self) {
        debug!("starting timers");
        self.player.start_timers(self.sender.clone());
    }

    fn pause_timers(&mut self) {
        debug!("pausing timers");
        self.player.pause_timers();
    }

    fn set_progress(&mut self, progress: Option<(i64, i64)>) {
        debug!("setting progress to {:?}", progress);
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
                        gtk_progress_bar_set_text(self.progress_bar as *mut GtkProgressBar, cstr)
                    );
                    gtk_progress_bar_set_fraction(self.progress_bar as *mut GtkProgressBar, fraction);
                }
            }
            None => {
                unsafe {
                    "".with_c_str(|cstr|
                        gtk_progress_bar_set_text(self.progress_bar as *mut GtkProgressBar, cstr)
                    );
                    gtk_progress_bar_set_fraction(self.progress_bar as *mut GtkProgressBar, 0.);
                }
            }
        }
    }

    pub fn test_receive(&mut self) -> bool {
        if self.buffered_msg.is_some() {
            return true;
        }

        match self.receiver.try_recv() {
            Ok(msg) => {
                self.buffered_msg = Some(msg);
                return true;
            }
            Err(comm::Empty) => {
                return false;
            }
            Err(comm::Disconnected) => {
                fail!("wut? noone allowed you to disconnect!")
            }
        }
    }

    /// This can only be called from one thread at a time, not
    /// synchronized!!
    pub fn dispatch_message(&mut self) -> bool {
        if !self.test_receive() {
            return false;
        }

        let msg = self.buffered_msg.take_unwrap();
        match msg {
            FetchPlayToken => self.fetch_play_token(),
            SetPlayToken(pt) => self.set_play_token(pt),
            UpdateMixes(m) => self.set_mixes(m),
            GetMixes(s) => self.get_mixes(s),
            PlayMix(i) => self.play_mix(i),
            PlayTrack(t) => self.play_track(t),
            ReportCurrentTrack => self.report_current_track(),
            TogglePlaying => self.toggle_playing(),
            SetBuffering(b) => self.set_buffering(b),
            NextTrack => self.next_track(),
            SkipTrack => self.skip_track(),
            SetPic(i, d) => self.set_pic(i, d),
            SetProgress(p) => self.set_progress(p),
            Notify(m) => self.notify(m),
            StartTimers => self.start_timers(),
            PauseTimers => self.pause_timers(),
        }

        return true;
    }

    /// This channel is synchronized, call it as often as you want
    pub fn get_sender<'a>(&'a self) -> &'a comm::Sender<GuiUpdateMessage> {
        &self.sender
    }
}

fn clear_gtk_container(container: *mut GtkContainer) {
    unsafe {
        let l = gtk_container_get_children(container);
        for ptr in GListIterator::new(&*l) {
            let widget = ptr as *mut GtkWidget;
            gtk_widget_destroy(widget);
        }
        g_list_free(l);
    }
}

unsafe fn get_gui_from_src(src: *mut GSource) -> & mut Gui {
    let gui_g_source = src as *mut GuiGSource;
    &mut *(*gui_g_source).gui_ptr
}

extern "C" fn prepare_gui_g_source(_src: *mut GSource, _timeout: *mut gint) -> gboolean {
    0
}

extern "C" fn check_gui_g_source(src: *mut GSource) -> gboolean {
    let gui = unsafe { get_gui_from_src(src) };
    if gui.test_receive() { 1 } else { 0 }
}

extern "C" fn dispatch_gui_g_source(src: *mut GSource,
        _callback: GSourceFunc, _user_data: gpointer) -> gboolean {
    let gui = unsafe { get_gui_from_src(src) };
    debug!("dispatching...")
    while gui.dispatch_message() { }

    // Returning 0 here would remove this GSource from the main loop
    return 1;
}

extern "C" fn close_button_pressed(_object: *GtkWidget, user_data: gpointer) {
    let gui: &mut Gui = unsafe { &mut *(user_data as *mut Gui) };
    gui.quit();
}

extern "C" fn play_button_clicked(_button: *GtkButton, user_data: gpointer) {
    let (gui, i) = unsafe {
        let &(gui_ptr, i): &(*Gui, uint) = cast::transmute(user_data);
        (&*gui_ptr, i)
    };
    gui.get_sender().send(PlayMix(i));
}

extern "C" fn toggle_button_clicked(_button: *GtkButton, user_data: gpointer) {
    let gui: &mut Gui = unsafe { &mut *(user_data as *mut Gui) };
    gui.get_sender().send(TogglePlaying);
}

extern "C" fn skip_button_clicked(_button: *GtkButton, user_data: gpointer) {
    let gui: &mut Gui = unsafe { &mut *(user_data as *mut Gui) };
    gui.get_sender().send(SkipTrack);
}

extern "C" fn smart_id_entry_activated(entry: *mut GtkEntry, user_data: gpointer) {
    let gui: &mut Gui = unsafe { &mut *(user_data as *mut Gui) };
    let id = unsafe { strraw::from_c_str(gtk_entry_get_text(entry)) };
    gui.get_sender().send(GetMixes(id));
}
