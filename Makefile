RUSTC ?= rustc
LIBS = `pkg-config --libs gtk+-3.0 gstreamer-1.0`
RUSTC_FLAGS = -Z debug-info -Z extra-debug-info --opt-level=0
#RUSTC_FLAGS = -O --cfg ndebug

rusttracks: main.rs api.rs webinterface.rs gui.rs player.rs timerfd_source.rs
	$(RUSTC) -L../dumb-gtk -L../rust-http/build $(RUSTC_FLAGS) --link-args "$(LIBS)" main.rs -o $@

.PHONY: clean
clean:
	rm -f rusttracks
