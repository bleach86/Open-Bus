# cart.mk: build a cart with the SDK. A cart is a directory anywhere, with
# its sources and a Makefile like this:
#
#   NAME = pet
#   BANKS = 2           # 8K banks: 2 (16K, never switched), 4, 8 or 16 (128K)
#   SAVE = 1            # K of save memory the cart uses, 0-32
#   RAM = yes           # run the program from RAM: up to 24K of it, in
#                       # banks 1-3; banks 4 up stay free (4, 8 or 16 banks)
#   include /path/to/sdk/cart.mk
#
# `make` builds NAME.crt from the directory's *.c and *.s files with cc65,
# against the machine's headers (sdk/include) and C runtime (sdk/lib); the
# linker config (sdk/cfg/cartBANKS.cfg) lays out the banks and the file
# header. Copy NAME.crt to the drive and choose it in the menu.
#
# The cart's icon in the menu is icon.png, 32x32 in up to 30 colours; without
# one, the SDK's default-icon.png.
#
# Pictures go in art/ as PNGs (at most 16 colours each, colour 0 or
# transparency the see-through colour). The SDK's converter, sdk/gfx, turns
# each art/NAME.png into tiles art_NAME and palette art_NAME_palette, in
# segment ART_SEGMENT (default RODATA; BANK1, say, to keep bank 0 for code),
# or ART_SEGMENT_NAME for that picture alone (art bigger than a bank).
# PNGs in art/16/ come out in 16x16 blocks, each a 16x16 sprite's four tiles
# in order: draw sprite sheets as they look.
#
# Songs go in music/ as text (docs/machine.md, "Music"): the SDK's `song`
# turns each music/NAME.song into song_NAME for the music driver (music.h),
# in MUSIC_SEGMENT (default RODATA), or MUSIC_SEGMENT_NAME for that song.
#
# NAME.lbl goes beside NAME.crt: its labels, for the emulator's monitor.
#
# Set OUT for where NAME.crt goes and BUILD for the intermediate files. cc65
# comes from CC65_HOME, ~/cc65 or the system install; the converter is built
# with cargo the first time it is needed.

SDK := $(patsubst %/,%,$(dir $(abspath $(lastword $(MAKEFILE_LIST)))))
# The cart's own Makefile: its settings (ART_SEGMENT, say) shape the output.
CART_MAKEFILE := $(firstword $(MAKEFILE_LIST))
BANKS ?= 2
SAVE ?= 0
OUT ?= .
BUILD ?= build
SOURCES ?= $(wildcard *.c *.s)
ART ?= $(wildcard art/*.png)
ART16 ?= $(wildcard art/16/*.png)
ICON ?= $(firstword $(wildcard icon.png) $(SDK)/default-icon.png)
ART_SEGMENT ?= RODATA
GFX ?= $(SDK)/gfx/target/release/gfx
SONGS ?= $(wildcard music/*.song)
MUSIC_SEGMENT ?= RODATA
SONG ?= $(SDK)/song/target/release/song

CC65_HOME ?= $(firstword $(wildcard $(HOME)/cc65) /usr/share/cc65)
CC65_BIN := $(if $(wildcard $(CC65_HOME)/bin/cc65),$(CC65_HOME)/bin/)
CFLAGS := -t none --cpu 6502 -Oirs -g -I $(SDK)/include
AFLAGS := --cpu 6502 -g -I $(CC65_HOME)/asminc -I $(SDK)/include

OBJECTS := $(patsubst %,$(BUILD)/%.o,$(basename $(SOURCES) $(ART) $(ART16) $(SONGS))) $(BUILD)/icon.o
SDK_SOURCES := $(wildcard $(SDK)/lib/*.c $(SDK)/lib/*.s)
SDK_OBJECTS := $(patsubst %,$(BUILD)/sdk/%.o,$(basename $(notdir $(SDK_SOURCES))))
# The SDK's headers, and this file: its flags shape every object.
HEADERS := $(wildcard $(SDK)/include/*) $(SDK)/cart.mk
CONFIG := $(SDK)/cfg/cart$(BANKS)$(if $(filter yes,$(RAM)),-ram).cfg

all: $(OUT)/$(NAME).crt $(OUT)/$(NAME).lbl

$(OUT)/$(NAME).crt $(OUT)/$(NAME).lbl &: $(OBJECTS) $(BUILD)/machine.lib $(CONFIG)
	@mkdir -p $(OUT)
	$(CC65_BIN)ld65 -C $(CONFIG) -D __CART_SAVE__=$(SAVE) -Ln $(OUT)/$(NAME).lbl -o $@ $(OBJECTS) $(BUILD)/machine.lib

clean:
	rm -rf $(BUILD) $(OUT)/$(NAME).crt $(OUT)/$(NAME).lbl

.PHONY: all clean
.SECONDARY:

# The icon, in the file after the header.
$(BUILD)/icon.s: $(ICON) $(GFX) $(CART_MAKEFILE)
	@mkdir -p $(@D)
	$(GFX) icon $< $@

# Art: each PNG to ca65 source.
$(BUILD)/art/%.s: art/%.png $(GFX) $(CART_MAKEFILE)
	@mkdir -p $(@D)
	$(GFX) asm $< $@ $(or $(ART_SEGMENT_$*),$(ART_SEGMENT))

$(BUILD)/art/16/%.s: art/16/%.png $(GFX) $(CART_MAKEFILE)
	@mkdir -p $(@D)
	$(GFX) asm16 $< $@ $(or $(ART_SEGMENT_$*),$(ART_SEGMENT))

$(GFX): $(wildcard $(SDK)/gfx/src/*.rs) $(SDK)/gfx/Cargo.toml
	cargo build --quiet --release --manifest-path $(SDK)/gfx/Cargo.toml --target-dir $(SDK)/gfx/target

# Songs: each to ca65 source.
$(BUILD)/music/%.s: music/%.song $(SONG) $(CART_MAKEFILE)
	@mkdir -p $(@D)
	$(SONG) $< $@ $(or $(MUSIC_SEGMENT_$*),$(MUSIC_SEGMENT))

$(SONG): $(wildcard $(SDK)/song/src/*.rs) $(SDK)/song/Cargo.toml
	cargo build --quiet --release --manifest-path $(SDK)/song/Cargo.toml --target-dir $(SDK)/song/target

# The C runtime: cc65's none.lib with the machine's modules in place of its
# own (crt0, for one).
$(BUILD)/machine.lib: $(SDK_OBJECTS)
	cp $(CC65_HOME)/lib/none.lib $@
	$(CC65_BIN)ar65 a $@ $^

$(BUILD)/%.s: %.c $(HEADERS)
	@mkdir -p $(@D)
	$(CC65_BIN)cc65 $(CFLAGS) -o $@ $<

$(BUILD)/%.o: %.s $(HEADERS)
	@mkdir -p $(@D)
	$(CC65_BIN)ca65 $(AFLAGS) -o $@ $<

$(BUILD)/%.o: $(BUILD)/%.s
	$(CC65_BIN)ca65 $(AFLAGS) -o $@ $<

$(BUILD)/sdk/%.s: $(SDK)/lib/%.c $(HEADERS)
	@mkdir -p $(@D)
	$(CC65_BIN)cc65 $(CFLAGS) -o $@ $<

$(BUILD)/sdk/%.o: $(SDK)/lib/%.s $(HEADERS)
	@mkdir -p $(@D)
	$(CC65_BIN)ca65 $(AFLAGS) -o $@ $<

$(BUILD)/sdk/%.o: $(BUILD)/sdk/%.s
	$(CC65_BIN)ca65 $(AFLAGS) -o $@ $<
