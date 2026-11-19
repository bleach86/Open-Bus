# The ROMs, the hard disk and clock cards' firmware, and the interrupt,
# bus, WAI and STP, Apple II, Mockingboard, hard disk, Atari 7800, Skink
# and Commander X16 FX tests' programs, and the serial card's firmware,
# built with cc65 (from ~/cc65 if it is there).
CC65 ?= $(if $(wildcard $(HOME)/cc65/bin/cl65),$(HOME)/cc65/bin/)

all: monitor.bin interrupt_test.bin bus_test.bin wai_stp_test.bin apple2_test.bin mockingboard_test.bin hard_disk.bin hard_disk_test.bin clock.bin serial_card.bin atari7800_test.bin skink_test.bin x16_fx_test.bin

monitor.bin: monitor.c crt0.s monitor.cfg
	$(CC65)cl65 -t none -O -C monitor.cfg -m monitor.map -o $@ crt0.s monitor.c

interrupt_test.bin: interrupt_test.s interrupt_test.cfg
	$(CC65)ca65 -g -o interrupt_test.o interrupt_test.s
	$(CC65)ld65 -C interrupt_test.cfg -Ln interrupt_test.lbl -o $@ interrupt_test.o

# The same layout as the interrupt test's: code at $0400, vectors at $FFFA.
bus_test.bin: bus_test.s interrupt_test.cfg
	$(CC65)ca65 -g -o bus_test.o bus_test.s
	$(CC65)ld65 -C interrupt_test.cfg -Ln bus_test.lbl -o $@ bus_test.o

# The same layout as the interrupt test's.
wai_stp_test.bin: wai_stp_test.s interrupt_test.cfg
	$(CC65)ca65 -g -o wai_stp_test.o wai_stp_test.s
	$(CC65)ld65 -C interrupt_test.cfg -Ln wai_stp_test.lbl -o $@ wai_stp_test.o

apple2_test.bin: apple2_test.s apple2_test.cfg
	$(CC65)ca65 -g -o apple2_test.o apple2_test.s
	$(CC65)ld65 -C apple2_test.cfg -Ln apple2_test.lbl -o $@ apple2_test.o

# The same layout as the Apple II test's: a 12K ROM at $D000.
mockingboard_test.bin: mockingboard_test.s apple2_test.cfg
	$(CC65)ca65 -g -o mockingboard_test.o mockingboard_test.s
	$(CC65)ld65 -C apple2_test.cfg -Ln mockingboard_test.lbl -o $@ mockingboard_test.o

# The same layout as the Apple II test's.
hard_disk_test.bin: hard_disk_test.s apple2_test.cfg
	$(CC65)ca65 -g -o hard_disk_test.o hard_disk_test.s
	$(CC65)ld65 -C apple2_test.cfg -Ln hard_disk_test.lbl -o $@ hard_disk_test.o

# The hard disk card's firmware: committed, as the emulator has it built
# in (its test checks it's this source's).
hard_disk.bin: hard_disk.s hard_disk.cfg
	$(CC65)ca65 -g -o hard_disk.o hard_disk.s
	$(CC65)ld65 -C hard_disk.cfg -Ln hard_disk.lbl -o $@ hard_disk.o

# The clock card's firmware: committed, likewise.
clock.bin: clock.s clock.cfg
	$(CC65)ca65 -g -o clock.o clock.s
	$(CC65)ld65 -C clock.cfg -Ln clock.lbl -o $@ clock.o

# The Super Serial Card's firmware: committed, likewise.
serial_card.bin: serial_card.s serial_card.cfg
	$(CC65)ca65 -g -o serial_card.o serial_card.s
	$(CC65)ld65 -C serial_card.cfg -Ln serial_card.lbl -o $@ serial_card.o

# The Atari 7800 test's cartridge: 16K at $C000.
atari7800_test.bin: atari7800_test.s atari7800_test.cfg
	$(CC65)ca65 -g -o atari7800_test.o atari7800_test.s
	$(CC65)ld65 -C atari7800_test.cfg -Ln atari7800_test.lbl -o $@ atari7800_test.o

# Skink's test boot image: 4K at $F000.
skink_test.bin: skink_test.s skink_test.cfg
	$(CC65)ca65 -g -o skink_test.o skink_test.s
	$(CC65)ld65 -C skink_test.cfg -Ln skink_test.lbl -o $@ skink_test.o

# The Commander X16's VERA FX test: a program, loaded at $0801.
x16_fx_test.bin: x16_fx_test.c
	$(CC65)cl65 -t cx16 -O -o $@ x16_fx_test.c

clean:
	rm -f monitor.bin monitor.map interrupt_test.bin interrupt_test.lbl bus_test.bin bus_test.lbl wai_stp_test.bin wai_stp_test.lbl apple2_test.bin apple2_test.lbl mockingboard_test.bin mockingboard_test.lbl hard_disk.lbl hard_disk_test.bin hard_disk_test.lbl clock.lbl serial_card.lbl atari7800_test.bin atari7800_test.lbl skink_test.bin skink_test.lbl x16_fx_test.bin *.o
