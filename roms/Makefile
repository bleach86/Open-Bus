# The ROMs, and the interrupt and bus tests' programs, built with cc65
# (from ~/cc65 if it is there).
CC65 ?= $(if $(wildcard $(HOME)/cc65/bin/cl65),$(HOME)/cc65/bin/)

all: monitor.bin interrupt_test.bin bus_test.bin

monitor.bin: monitor.c crt0.s monitor.cfg
	$(CC65)cl65 -t none -O -C monitor.cfg -m monitor.map -o $@ crt0.s monitor.c

interrupt_test.bin: interrupt_test.s interrupt_test.cfg
	$(CC65)ca65 -g -o interrupt_test.o interrupt_test.s
	$(CC65)ld65 -C interrupt_test.cfg -Ln interrupt_test.lbl -o $@ interrupt_test.o

# The same layout as the interrupt test's: code at $0400, vectors at $FFFA.
bus_test.bin: bus_test.s interrupt_test.cfg
	$(CC65)ca65 -g -o bus_test.o bus_test.s
	$(CC65)ld65 -C interrupt_test.cfg -Ln bus_test.lbl -o $@ bus_test.o

clean:
	rm -f monitor.bin monitor.map interrupt_test.bin interrupt_test.lbl bus_test.bin bus_test.lbl *.o
