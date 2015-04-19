#![feature(rustc_private)]
#![feature(unsafe_destructor)]

#[macro_use]
extern crate log;

extern crate libc;
extern crate rustc_serialize;
extern crate url;

extern crate gtk;
extern crate hyper;
extern crate timerfd;

use std::env;

mod api;
mod gui;
mod player;
mod utils;
mod webinterface;

pub fn main() {
    let mut gui = gui::Gui::new();
    gui.init(env::args().collect());

    gui.get_sender().send(gui::GuiUpdateMessage::Notify("Welcome to RustTracks!".to_string()));
    gui.get_sender().send(gui::GuiUpdateMessage::FetchPlayToken);
    gui.get_sender().send(gui::GuiUpdateMessage::GetMixes("tags:folk:recent".to_string()));

    gui.run();
}
