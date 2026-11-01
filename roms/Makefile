# The ROMs, the hard disk card's firmware, and the interrupt, bus, WAI and
# STP, Apple II, Mockingboard and hard disk tests' programs, built with
# cc65 (from ~/cc65 if it is there).
CC65 ?= $(if $(wildcard $(HOME)/cc65/bin/cl65),$(HOME)/cc65/bin/)

all: monitor.bin interrupt_test.bin bus_test.bin wai_stp_test.bin apple2_test.bin mockingboard_test.bin hard_disk.bin hard_disk_test.bin

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

clean:
	rm -f monitor.bin monitor.map interrupt_test.bin interrupt_test.lbl bus_test.bin bus_test.lbl wai_stp_test.bin wai_stp_test.lbl apple2_test.bin apple2_test.lbl mockingboard_test.bin mockingboard_test.lbl hard_disk.lbl hard_disk_test.bin hard_disk_test.lbl *.o
