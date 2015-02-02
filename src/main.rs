#![feature(collections)]
#![feature(core)]
#![feature(io)]
#![feature(libc)]
#![feature(os)]
#![feature(rustc_private)]
#![feature(std_misc)]
#![feature(unsafe_destructor)]

#[macro_use]
extern crate log;

extern crate libc;
extern crate "rustc-serialize" as rustc_serialize;
extern crate url;

extern crate gtk;
extern crate hyper;
extern crate timerfd;

use std::os;

mod api;
mod gui;
mod player;
mod webinterface;

pub fn main() {
    let mut gui = gui::Gui::new();
    gui.init(os::args());

    gui.get_sender().send(gui::GuiUpdateMessage::Notify("Welcome to RustTracks!".to_string()));
    gui.get_sender().send(gui::GuiUpdateMessage::FetchPlayToken);
    gui.get_sender().send(gui::GuiUpdateMessage::GetMixes("tags:folk:recent".to_string()));

    gui.run();
}