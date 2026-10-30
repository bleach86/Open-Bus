# The ROMs, and the interrupt test's program, built with cc65 (from ~/cc65
# if it is there).
CC65 ?= $(if $(wildcard $(HOME)/cc65/bin/cl65),$(HOME)/cc65/bin/)

all: monitor.bin interrupt_test.bin

monitor.bin: monitor.c crt0.s monitor.cfg
	$(CC65)cl65 -t none -O -C monitor.cfg -m monitor.map -o $@ crt0.s monitor.c

interrupt_test.bin: interrupt_test.s interrupt_test.cfg
	$(CC65)ca65 -g -o interrupt_test.o interrupt_test.s
	$(CC65)ld65 -C interrupt_test.cfg -Ln interrupt_test.lbl -o $@ interrupt_test.o

clean:
	rm -f monitor.bin monitor.map interrupt_test.bin interrupt_test.lbl *.o
