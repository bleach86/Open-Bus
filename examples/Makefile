# Every example cart, built with the SDK onto a drive: drive/ at the top of
# the repository unless OUT says otherwise. Run the machine on it with
# `cargo run --release -- --drive drive`.
OUT ?= $(abspath $(dir $(lastword $(MAKEFILE_LIST)))../drive)
CARTS := $(patsubst %/Makefile,%,$(wildcard */Makefile))

all: $(CARTS)

$(CARTS):
	$(MAKE) -C $@ OUT=$(OUT)

clean:
	for cart in $(CARTS); do $(MAKE) -C $$cart clean OUT=$(OUT); done

.PHONY: all clean $(CARTS)
