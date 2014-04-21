RUSTC ?= rustc
LIBS = `pkg-config --libs gtk+-3.0 gstreamer-1.0`
#RUSTC_FLAGS = --dep-info -g --opt-level=0
RUSTC_FLAGS = --dep-info -O --cfg ndebug
RUSTC_BIN_FLAGS = --crate-type bin -Z lto
RUSTC_RLIB_FLAGS = --crate-type rlib

timerfd_source_libname = $(shell $(RUSTC) --crate-type rlib --crate-file-name timerfd_source.rs)

rusttracks: main.rs $(timerfd_source_libname)
	$(RUSTC) -o $@ -L../dumb-gtk -L../rust-http/build -L. $(RUSTC_FLAGS) $(RUSTC_BIN_FLAGS) -C link-args="$(LIBS)" "$<"

$(timerfd_source_libname): timerfd_source.rs
	$(RUSTC) -L../dumb-gtk $(RUSTC_FLAGS) $(RUSTC_RLIB_FLAGS) -C link-args="$(LIBS)" "$<"

.PHONY: clean
clean:
	rm -f rusttracks rusttracks.d $(timerfd_source_libname) timerfd_source.d

-include rusttracks.d
-include timerfd_source.d
