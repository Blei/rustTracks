use libc;

use std::ffi as rffi;
use std::iter;
use std::ptr;
use std::mem;
use std::str;
use std::sync::mpsc;
use std::thread;

use gtk::ffi::*;
use gtk::*;

use api;
use player;
use webinterface;

fn as_box<T>(in_ptr: *mut T) -> *mut GtkBox {
    in_ptr as *mut GtkBox
}

static ICON_DATA: &'static [u8] = include_bytes!("8tracks-icon.jpg");

fn get_icon_pixbuf() -> *mut GdkPixbuf {
    get_pixbuf_from_data(ICON_DATA)
}

fn get_pixbuf_from_data(pic_data: &[u8]) -> *mut GdkPixbuf {
    unsafe {
        let mut err = ptr::null_mut();

        let stream = g_memory_input_stream_new_from_data(
            pic_data.as_ptr() as *const libc::c_void,
            pic_data.len() as i64, None);
        let pixbuf = gdk_pixbuf_new_from_stream(stream, ptr::null_mut(), &mut err);
        assert!(pixbuf != ptr::null_mut());
        g_input_stream_close(stream, ptr::null_mut(), &mut err);
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
    _g_source: GSource,
    gui_ptr: *mut Gui,
}

pub enum GuiUpdateMessage {
    FetchPlayToken,
    SetPlayToken(api::PlayToken),
    GetMixes(String),
    UpdateMixes(Vec<api::Mix>),
    PlayMix(usize),
    PlayTrack(api::Track),
    ReportCurrentTrack,
    TogglePlaying,
    SetBuffering(bool),
    NextTrack,
    SkipTrack,
    SetPic(usize, Vec<u8>),
    SetCurrentPic(Vec<u8>),
    UpdateProgress,
    Notify(String),
    StartTimers,
    PauseTimers,
}

struct LoadingImage {
    image: *mut GtkImage,
    // Currently only square images.
    size: libc::c_int,
}

impl LoadingImage {
    fn new(size: libc::c_int) -> LoadingImage {
        LoadingImage {
            image: unsafe{ gtk_image_new() } as *mut GtkImage,
            size: size,
        }
    }

    fn set_image(&mut self, pixbuf: *mut GdkPixbuf) {
        unsafe {
            gtk_image_set_from_pixbuf(self.image, pixbuf);
        }
    }

    fn set_image_from_data(&mut self, data: &[u8]) {
        unsafe {
            let pixbuf1 = get_pixbuf_from_data(data);
            let pixbuf2 = gdk_pixbuf_scale_simple(&*pixbuf1, self.size, self.size, GDK_INTERP_BILINEAR);
            gdk_pixbuf_unref(pixbuf1);
            self.set_image(pixbuf2);
            gdk_pixbuf_unref(pixbuf2);
        }
    }

    fn reset(&mut self) {
        self.set_image_from_data(ICON_DATA);
    }
}

struct MixEntry {
    mix: api::Mix,

    widget: *mut GtkWidget,
    image: LoadingImage,
}

impl MixEntry {
    fn new(mix: api::Mix, mix_table_entry: &(*mut Gui, usize)) -> MixEntry {
        let (widget, image) = unsafe {
            let entry_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 5);

            let label = {
                let text = rffi::CString::from_slice(mix.name.as_bytes());
                gtk_label_new(text.as_ptr())
            };
            gtk_box_pack_start(as_box(entry_box), label, 1, 1, 0);
            gtk_label_set_line_wrap(label as *mut GtkLabel, 1);
            gtk_label_set_line_wrap_mode(label as *mut GtkLabel, PANGO_WRAP_WORD_CHAR);
            gtk_misc_set_alignment(label as *mut GtkMisc, 0f32, 0.5f32);

            let mut image = LoadingImage::new(133);
            image.reset();
            gtk_box_pack_end(as_box(entry_box), image.image as *mut GtkWidget, 0, 0, 0);

            let button_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
            gtk_box_pack_end(as_box(entry_box), button_box, 0, 0, 0);

            let button = {
                let text = rffi::CString::from_slice(b"Play");
                gtk_button_new_with_label(text.as_ptr())
            };
            gtk_box_pack_end(as_box(button_box), button, 1, 0, 0);
            {
                let signal = rffi::CString::from_slice(b"clicked");
                g_signal_connect(button as gpointer,
                                 signal.as_ptr(),
                                 Some(mem::transmute(play_button_clicked)),
                                 mem::transmute::<&(*mut Gui, usize), gpointer>(mix_table_entry));
            }

            (entry_box, image)
        };

        MixEntry {
            mix: mix,
            widget: widget,
            image: image,
        }
    }

    fn set_pic_from_data(&mut self, data: &[u8]) {
        self.image.set_image_from_data(data);
    }
}

pub struct Gui {
    initialized: bool,
    running: bool,

    mix_entries: Vec<MixEntry>,
    play_token: Option<api::PlayToken>,

    current_mix_index: Option<usize>,
    current_track: Option<api::Track>,

    main_window: *mut GtkWidget,
    main_notebook: *mut GtkWidget,

    // These are all in the first notebook page, the playlists
    playlists_notebook_index: libc::c_int,
    mixes_scrolled_window: *mut GtkWidget,
    mixes_box: *mut GtkWidget,
    status_bar: *mut GtkWidget,
    status_bar_ci: Option<guint>,

    // And these are on the second page, the current list
    current_notebook_index: libc::c_int,
    // None at program start, Some forever after.
    current_image: Option<LoadingImage>,
    toggle_button: *mut GtkWidget,
    skip_button: *mut GtkWidget,
    progress_bar: *mut GtkWidget,
    info_label: *mut GtkWidget,

    receiver: mpsc::Receiver<GuiUpdateMessage>,
    sender: mpsc::Sender<GuiUpdateMessage>,
    buffered_msg: Option<GuiUpdateMessage>,

    gui_g_source: *mut GuiGSource,
    g_source_funcs: GSourceFuncs,

    player: player::Player,

    // this is such a hack...
    mix_index_table: Vec<(*mut Gui, usize)>,
}

#[unsafe_destructor]
impl Drop for Gui {
    fn drop(&mut self) {
        self.quit();
        if self.gui_g_source != ptr::null_mut() {
            unsafe {
                g_source_unref(self.gui_g_source as *mut GSource);
            }
            self.gui_g_source = ptr::null_mut();
        }
    }
}

impl Gui {
    pub fn new() -> Gui {
        let (sender, receiver) = mpsc::channel();
        Gui {
            initialized: false,
            running: false,
            mix_entries: Vec::new(),
            play_token: None,
            current_mix_index: None,
            current_track: None,
            main_window: ptr::null_mut(),
            main_notebook: ptr::null_mut(),

            playlists_notebook_index: -1,
            mixes_scrolled_window: ptr::null_mut(),
            mixes_box: ptr::null_mut(),
            status_bar: ptr::null_mut(),
            status_bar_ci: None,

            current_notebook_index: -1,
            current_image: None,
            toggle_button: ptr::null_mut(),
            skip_button: ptr::null_mut(),
            progress_bar: ptr::null_mut(),
            info_label: ptr::null_mut(),

            receiver: receiver,
            sender: sender,
            buffered_msg: None,
            gui_g_source: ptr::null_mut(),
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
            let name = rffi::CString::from_slice(icon_name.as_bytes());
            let image = gtk_image_new_from_icon_name(name.as_ptr(), GTK_ICON_SIZE_BUTTON);
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
                let empty = rffi::CString::from_slice(b"");
                unsafe {
                    gtk_label_set_text(self.info_label as *mut GtkLabel, empty.as_ptr());
                }
            }
            Some(ref track) => {
                let mut text = String::new();
                text.push_str(format!("'{}' by {}", track.name, track.performer).as_slice());
                match track.release_name {
                    Some(ref rn) => {
                        text.push_str(format!("\nAlbum: {}", *rn).as_slice());
                        match track.year {
                            Some(year) => text.push_str(format!(" ({})", year).as_slice()),
                            None => ()
                        }
                    }
                    None => ()
                }
                let text_c_str = rffi::CString::from_slice(text.as_bytes());
                unsafe {
                    gtk_label_set_text(self.info_label as *mut GtkLabel, text_c_str.as_ptr());
                }
            }
        }
    }

    pub fn init(&mut self, args: Vec<String>) {
        if !self.initialized {
            let args2;
            unsafe {
                args2 = gtk_init_with_args_2(args.clone());
                self.main_window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
                gtk_window_set_default_size(self.main_window as *mut GtkWindow, 400, 500);
                let destroy = rffi::CString::from_slice(b"destroy");
                g_signal_connect(self.main_window as gpointer,
                                 destroy.as_ptr(),
                                 Some(mem::transmute(close_button_pressed)),
                                 mem::transmute::<&Gui, gpointer>(self));
                let icon = get_icon_pixbuf();
                gtk_window_set_icon(self.main_window as *mut GtkWindow, icon);
                gdk_pixbuf_unref(icon);

                self.main_notebook = gtk_notebook_new();
                gtk_container_add(self.main_window as *mut GtkContainer, self.main_notebook);

                // First page: All Playlists
                let main_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 5);

                let playlists_c_str = rffi::CString::from_slice(b"Playlists");
                let playlist_label = gtk_label_new(playlists_c_str.as_ptr());
                self.playlists_notebook_index = gtk_notebook_append_page(
                    self.main_notebook as *mut GtkNotebook,
                    main_box,
                    playlist_label);
                if self.playlists_notebook_index < 0 {
                    panic!("Adding first page to notebook failed");
                }

                self.mixes_scrolled_window = gtk_scrolled_window_new(ptr::null_mut(),
                                                                   ptr::null_mut());
                gtk_scrolled_window_set_policy(self.mixes_scrolled_window as *mut GtkScrolledWindow,
                    GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
                gtk_box_pack_start(as_box(main_box), self.mixes_scrolled_window, 1, 1, 0);

                self.mixes_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 5);
                gtk_container_add(self.mixes_scrolled_window as *mut GtkContainer, self.mixes_box);

                let smart_id_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 5);
                gtk_box_pack_start(as_box(main_box), smart_id_box, 0, 0, 0);

                let smart_id_ordering_combo = gtk_combo_box_text_new();
                gtk_box_pack_start(as_box(smart_id_box), smart_id_ordering_combo, 0, 0, 0);
                {
                    let popular_c_str = rffi::CString::from_slice(b"popular");
                    gtk_combo_box_text_append(smart_id_ordering_combo as *mut GtkComboBoxText,
                                              ptr::null(), popular_c_str.as_ptr());
                }
                {
                    let new_c_str = rffi::CString::from_slice(b"new");
                    gtk_combo_box_text_append(smart_id_ordering_combo as *mut GtkComboBoxText,
                                              ptr::null(), new_c_str.as_ptr());
                }
                gtk_combo_box_set_active(smart_id_ordering_combo as *mut GtkComboBox,
                                         MixesOrdering::Popular as libc::c_int);

                let smart_id_entry = gtk_entry_new();
                gtk_box_pack_start(as_box(smart_id_box), smart_id_entry, 1, 1, 0);
                {
                    let activate_c_str = rffi::CString::from_slice(b"activate");
                    g_signal_connect(smart_id_entry as gpointer,
                                     activate_c_str.as_ptr(),
                                     Some(mem::transmute(smart_id_entry_activated)),
                                     mem::transmute::<&Gui, gpointer>(self));
                }

                self.status_bar = gtk_statusbar_new();
                let rusttracks_c_str = rffi::CString::from_slice(b"rusttracks");
                self.status_bar_ci = Some(gtk_statusbar_get_context_id(
                        self.status_bar as *mut GtkStatusbar,
                        rusttracks_c_str.as_ptr()));
                gtk_box_pack_start(as_box(main_box), self.status_bar, 0, 0, 0);

                // Second page: Current Playlist
                let current_box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 5);

                let current_c_str = rffi::CString::from_slice(b"Current");
                let current_label = gtk_label_new(current_c_str.as_ptr());
                self.current_notebook_index = gtk_notebook_append_page(
                    self.main_notebook as *mut GtkNotebook,
                    current_box,
                    current_label);
                if self.current_notebook_index < 0 {
                    panic!("Adding second page to notebook failed");
                }

                let mut image = LoadingImage::new(250);
                image.reset();
                gtk_container_add(current_box as *mut GtkContainer,
                                  image.image as *mut GtkWidget);
                self.current_image = Some(image);

                let control_box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
                gtk_container_add(current_box as *mut GtkContainer, control_box);

                let pause_icon_c_str = rffi::CString::from_slice(PAUSE_ICON_NAME.as_bytes());
                self.toggle_button = gtk_button_new_from_icon_name(
                    pause_icon_c_str.as_ptr(), GTK_ICON_SIZE_BUTTON);
                let clicked_c_str = rffi::CString::from_slice(b"clicked");
                g_signal_connect(self.toggle_button as gpointer,
                                 clicked_c_str.as_ptr(),
                                 Some(mem::transmute(toggle_button_clicked)),
                                 mem::transmute::<&Gui, gpointer>(self));
                gtk_box_pack_start(control_box as *mut GtkBox, self.toggle_button, 0, 0, 0);

                let skip_icon_c_str = rffi::CString::from_slice(SKIP_ICON_NAME.as_bytes());
                self.skip_button = gtk_button_new_from_icon_name(
                    skip_icon_c_str.as_ptr(), GTK_ICON_SIZE_BUTTON);
                g_signal_connect(self.skip_button as gpointer,
                                 clicked_c_str.as_ptr(),
                                 Some(mem::transmute(skip_button_clicked)),
                                 mem::transmute::<&Gui, gpointer>(self));
                gtk_box_pack_start(as_box(control_box), self.skip_button, 0, 0, 0);

                self.progress_bar = gtk_progress_bar_new();
                gtk_box_pack_end(as_box(control_box), self.progress_bar, 1, 1, 0);
                let empty = rffi::CString::from_slice(b"");
                gtk_progress_bar_set_text(self.progress_bar as *mut GtkProgressBar, empty.as_ptr());
                gtk_progress_bar_set_show_text(self.progress_bar as *mut GtkProgressBar, 1);

                self.control_buttons_set_sensitive(false);

                self.info_label = gtk_label_new(ptr::null());
                gtk_box_pack_start(as_box(current_box), self.info_label, 0, 0, 0);
                gtk_label_set_justify(self.info_label as *mut GtkLabel, GTK_JUSTIFY_CENTER);

                // And finally the GSource
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
                if self.main_window != ptr::null_mut() {
                    unsafe {
                        gtk_widget_destroy(self.main_window);
                    }
                    self.main_window = ptr::null_mut();
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

        info!("Notification message: {}", message);
        let message_c_str = rffi::CString::from_slice(message.as_bytes());
        unsafe {
            gtk_statusbar_push(self.status_bar as *mut GtkStatusbar,
                               *self.status_bar_ci.as_ref().unwrap(), message_c_str.as_ptr());
        }
    }

    fn fetch_play_token(&self) {
        if self.play_token.is_some() {
            debug!("play token already exists, ignoring request");
            return;
        }

        debug!("fetching play token");
        let sender = self.sender.clone();
        thread::Thread::spawn(move || {
            let pt_json = match webinterface::get_play_token() {
                Ok(ptj) => ptj,
                Err(io_err) => {
                    sender.send(GuiUpdateMessage::Notify(format!("Playtoken could not be obtained: `{}`", io_err)));
                    return;
                }
            };
            let pt = api::parse_play_token_response(&pt_json);
            match pt.contents {
                Some(pt) => sender.send(GuiUpdateMessage::SetPlayToken(pt)),
                None => sender.send(GuiUpdateMessage::Notify("Playtoken could not be obtained".to_string()))
            };
        });
    }

    fn set_play_token(&mut self, pt: api::PlayToken) {
        debug!("setting play token to `{}`", pt.s);
        self.play_token = Some(pt.clone());
    }

    fn set_mixes(&mut self, mixes: Vec<api::Mix>) {
        self.mix_index_table = (0..mixes.len()).map(|i| (self as *mut Gui, i)).collect();
        self.mix_entries.clear();
        debug!("setting mixes, length {}", mixes.len());
        unsafe {
            clear_gtk_container(self.mixes_box as *mut GtkContainer);
            for i in iter::range(0, mixes.len()) {
                let mix_entry = MixEntry::new(mixes[i].clone(), &self.mix_index_table[i]);
                gtk_box_pack_start(as_box(self.mixes_box),
                    mix_entry.widget, 0, 1, 0);
                self.mix_entries.push(mix_entry);

                // Fetch cover pic
                let sender = self.sender.clone();
                let pic_url_str = mixes[i].cover_urls.sq133.clone();
                thread::Thread::spawn(move || {
                    let pic_data = match webinterface::get_data_from_url_str(pic_url_str.as_slice()) {
                        Ok(pd) => pd,
                        Err(io_err) => {
                            sender.send(GuiUpdateMessage::Notify(format!("Could not get picture: `{}`", io_err)));
                            return;
                        }
                    };
                    sender.send(GuiUpdateMessage::SetPic(i, pic_data));
                });
            }
            gtk_widget_show_all(self.mixes_box);
            let adj = gtk_scrolled_window_get_vadjustment(
                self.mixes_scrolled_window as *mut GtkScrolledWindow);
            let lower = gtk_adjustment_get_lower(adj);
            gtk_adjustment_set_value(adj, lower);
        }
    }

    fn get_mixes(&self, smart_id: String) {
        debug!("getting mixes for smart id '{}'", smart_id);
        let sender = self.get_sender().clone();
        thread::Thread::spawn(move || {
            let mix_set_json = match webinterface::get_mix_set(smart_id.as_slice()) {
                Ok(msj) => msj,
                Err(io_err) => {
                    sender.send(GuiUpdateMessage::Notify(format!("Could not get mix list: `{}`", io_err)));
                    return;
                }
            };
            let mix_set = api::parse_mix_set_response(&mix_set_json);
            match mix_set.contents {
                Some(ms) => sender.send(GuiUpdateMessage::UpdateMixes(ms.mixes)),
                None => sender.send(GuiUpdateMessage::Notify("Mix list could not be obtained".to_string()))
            };
        });
    }

    fn play_mix(&mut self, i: usize) {
        debug!("playing mix with index {}", i);
        if i >= self.mix_entries.len() {
            warn!("index is out of bounds, ignoring message");
        } else {
            self.current_mix_index = Some(i);
            let mix = self.mix_entries[i].mix.clone();
            debug!("playing mix with name `{}`", mix.name);
            let pt = self.play_token.as_ref().unwrap().clone();
            self.player.pause();

            // Fetch cover pic
            self.current_image.as_mut().unwrap().reset();
            let sender = self.sender.clone();
            let pic_url_str = mix.cover_urls.sq250.clone();
            thread::Thread::spawn(move || {
                let pic_data = match webinterface::get_data_from_url_str(pic_url_str.as_slice()) {
                    Ok(pd) => pd,
                    Err(io_err) => {
                        sender.send(GuiUpdateMessage::Notify(format!("Could not get picture: `{}`", io_err)));
                        return;
                    }
                };
                sender.send(GuiUpdateMessage::SetCurrentPic(pic_data));
            });

            // Actually play
            let sender = self.sender.clone();
            thread::Thread::spawn(move || {
                let play_state_json = match webinterface::get_play_state(&pt, &mix) {
                    Ok(psj) => psj,
                    Err(io_err) => {
                        sender.send(GuiUpdateMessage::Notify(format!("Could not start playing mix: `{}`", io_err)));
                        return;
                    }
                };
                let play_state = api::parse_play_state_response(&play_state_json);
                match play_state.contents {
                    Some(ps) => sender.send(GuiUpdateMessage::PlayTrack(ps.track)),
                    None => sender.send(GuiUpdateMessage::Notify("Could not start playing mix".to_string()))
                };
            });

            unsafe {
                gtk_notebook_set_current_page(self.main_notebook as *mut GtkNotebook,
                                              self.current_notebook_index);
            }
        }
    }

    fn play_track(&mut self, track: api::Track) {
        debug!("playing track `{}`", track.name);
        self.set_current_track(track.clone());
        debug!("setting uri to `{}`", track.track_file_stream_url);
        self.player.set_uri(track.track_file_stream_url.as_slice());
        self.player.play();
        self.update_play_button_icon();
        self.control_buttons_set_sensitive(true);
        self.set_progress(None);
    }

    fn report_current_track(&self) {
        debug!("reporting current track");
        let (pt, ti, mi) =
            (
                self.play_token.as_ref().unwrap().clone(),
                self.mix_entries[*self.current_mix_index.as_ref().unwrap()].mix.id,
                self.current_track.as_ref().unwrap().id,
            );
        thread::Thread::spawn(move || {
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
        let mix = self.mix_entries[i].mix.clone();
        debug!("getting next track of mix with name `{}`", mix.name);
        let sender = self.sender.clone();
        let pt = self.play_token.as_ref().unwrap().clone();
        thread::Thread::spawn(move || {
            let next_track_json = match webinterface::get_next_track(&pt, &mix) {
                Ok(ntj) => ntj,
                Err(io_err) => {
                    sender.send(GuiUpdateMessage::Notify(format!("Could not get next track: `{}`", io_err)));
                    return;
                }
            };
            let play_state = api::parse_play_state_response(&next_track_json);
            match play_state.contents {
                Some(ps) => sender.send(GuiUpdateMessage::PlayTrack(ps.track)),
                None => sender.send(GuiUpdateMessage::Notify("Next track could not be obtained".to_string()))
            };
        });
    }

    fn skip_track(&mut self) {
        self.player.pause();
        self.update_play_button_icon();

        let i = self.current_mix_index.unwrap();
        let mix = self.mix_entries[i].mix.clone();
        debug!("skipping track of mix with name `{}`", mix.name);
        let sender = self.sender.clone();
        let pt = self.play_token.as_ref().unwrap().clone();
        thread::Thread::spawn(move || {
            let skip_track_json = match webinterface::get_skip_track(&pt, &mix) {
                Ok(stj) => stj,
                Err(io_err) => {
                    sender.send(GuiUpdateMessage::Notify(format!("Could not skip track: `{}`", io_err)));
                    return;
                }
            };
            let play_state = api::parse_play_state_response(&skip_track_json);
            match play_state.contents {
                Some(ps) => sender.send(GuiUpdateMessage::PlayTrack(ps.track)),
                None => sender.send(GuiUpdateMessage::Notify("Could not skip track".to_string())),
            };
        });
    }

    fn set_pic(&mut self, i: usize, pic_data: Vec<u8>) {
        if i >= self.mix_entries.len() {
            warn!("set_pic: index {} is out of range, only {} mix_entries",
                  i, self.mix_entries.len());
        } else {
            self.mix_entries[i].set_pic_from_data(pic_data.as_slice());
        }
    }

    fn set_current_pic(&mut self, pic_data: Vec<u8>) {
        self.current_image.as_mut().unwrap().set_image_from_data(pic_data.as_slice());
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
                let text = format!("{}:{:02} / {}:{:02}",
                                   pos_sec / 60, pos_sec % 60,
                                   dur_sec / 60, dur_sec % 60);
                let text_c_str = rffi::CString::from_slice(text.as_bytes());
                unsafe {
                    gtk_progress_bar_set_text(self.progress_bar as *mut GtkProgressBar,
                                              text_c_str.as_ptr());
                    gtk_progress_bar_set_fraction(self.progress_bar as *mut GtkProgressBar,
                                                  fraction);
                }
            }
            None => {
                let empty_c_str = rffi::CString::from_slice(b"");
                unsafe {
                    gtk_progress_bar_set_text(self.progress_bar as *mut GtkProgressBar,
                                              empty_c_str.as_ptr());
                    gtk_progress_bar_set_fraction(self.progress_bar as *mut GtkProgressBar, 0.);
                }
            }
        }
    }

    fn update_progress(&mut self) {
        let progress = self.player.get_progress_info();
        self.set_progress(progress);
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
            Err(mpsc::TryRecvError::Empty) => {
                return false;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                panic!("wut? noone allowed you to disconnect!")
            }
        }
    }

    /// This can only be called from one thread at a time, not
    /// synchronized!!
    pub fn dispatch_message(&mut self) -> bool {
        if !self.test_receive() {
            return false;
        }

        let msg = self.buffered_msg.take().unwrap();
        match msg {
            GuiUpdateMessage::FetchPlayToken => self.fetch_play_token(),
            GuiUpdateMessage::SetPlayToken(pt) => self.set_play_token(pt),
            GuiUpdateMessage::UpdateMixes(m) => self.set_mixes(m),
            GuiUpdateMessage::GetMixes(s) => self.get_mixes(s),
            GuiUpdateMessage::PlayMix(i) => self.play_mix(i),
            GuiUpdateMessage::PlayTrack(t) => self.play_track(t),
            GuiUpdateMessage::ReportCurrentTrack => self.report_current_track(),
            GuiUpdateMessage::TogglePlaying => self.toggle_playing(),
            GuiUpdateMessage::SetBuffering(b) => self.set_buffering(b),
            GuiUpdateMessage::NextTrack => self.next_track(),
            GuiUpdateMessage::SkipTrack => self.skip_track(),
            GuiUpdateMessage::SetPic(i, d) => self.set_pic(i, d),
            GuiUpdateMessage::SetCurrentPic(d) => self.set_current_pic(d),
            GuiUpdateMessage::UpdateProgress => self.update_progress(),
            GuiUpdateMessage::Notify(m) => self.notify(m.as_slice()),
            GuiUpdateMessage::StartTimers => self.start_timers(),
            GuiUpdateMessage::PauseTimers => self.pause_timers(),
        }

        return true;
    }

    /// This channel is synchronized, call it as often as you want
    pub fn get_sender<'a>(&'a self) -> &'a mpsc::Sender<GuiUpdateMessage> {
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

// The lifetime of 'static is a lie.
unsafe fn get_gui_from_src(src: *mut GSource) -> &'static mut Gui {
    let gui_g_source = src as *mut GuiGSource;
    &mut *(*gui_g_source).gui_ptr
}

extern "C" fn prepare_gui_g_source(_src: *mut GSource, timeout: *mut gint) -> gboolean {
    unsafe {
        // Wait at most 500ms before checking again.
        *timeout = 500;
    }
    0
}

extern "C" fn check_gui_g_source(src: *mut GSource) -> gboolean {
    let gui = unsafe { get_gui_from_src(src) };
    if gui.test_receive() { 1 } else { 0 }
}

extern "C" fn dispatch_gui_g_source(src: *mut GSource,
        _callback: GSourceFunc, _user_data: gpointer) -> gboolean {
    let gui = unsafe { get_gui_from_src(src) };
    debug!("dispatching...");
    while gui.dispatch_message() { }

    // Returning 0 here would remove this GSource from the main loop
    return 1;
}

extern "C" fn close_button_pressed(_object: *const GtkWidget, user_data: gpointer) {
    let gui: &mut Gui = unsafe { &mut *(user_data as *mut Gui) };
    gui.quit();
}

extern "C" fn play_button_clicked(_button: *const GtkButton, user_data: gpointer) {
    let (gui, i) = unsafe {
        let &(gui_ptr, i): &(*const Gui, usize) = mem::transmute(user_data);
        (&*gui_ptr, i)
    };
    gui.get_sender().send(GuiUpdateMessage::PlayMix(i));
}

extern "C" fn toggle_button_clicked(_button: *const GtkButton, user_data: gpointer) {
    let gui: &mut Gui = unsafe { &mut *(user_data as *mut Gui) };
    gui.get_sender().send(GuiUpdateMessage::TogglePlaying);
}

extern "C" fn skip_button_clicked(_button: *const GtkButton, user_data: gpointer) {
    let gui: &mut Gui = unsafe { &mut *(user_data as *mut Gui) };
    gui.get_sender().send(GuiUpdateMessage::SkipTrack);
}

extern "C" fn smart_id_entry_activated(entry: *mut GtkEntry, user_data: gpointer) {
    let gui: &mut Gui = unsafe { &mut *(user_data as *mut Gui) };
    let id = unsafe { str::from_c_str(gtk_entry_get_text(entry) as *const i8).to_string() };
    gui.get_sender().send(GuiUpdateMessage::GetMixes(id));
}
