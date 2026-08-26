BIN    := $(shell rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin
QEMU   := qemu-system-aarch64

MEM ?= 128M

# A raw image, not the ELF: only then does QEMU build its Linux bootloader stub,
# which generates a device tree, loads it, and passes its address in x0.
QFLAGS := -M virt -cpu cortex-a72 -m $(MEM) -nographic -kernel kernel.bin
MON    := -M virt -cpu cortex-a72 -m $(MEM) -display none -serial null -monitor stdio -kernel kernel.bin

CARGO_OUT := target/aarch64-unknown-none-softfloat/release/zheos

# cargo does its own change detection, so this always runs and is cheap.
# kernel.elf is a copy so the debugger and QEMU have one stable path.
kernel.elf:
	cargo build --release
	@cp $(CARGO_OUT) $@

# QEMU strips the ELF headers for us; the load address stays the linked one.
kernel.bin: kernel.elf
	@$(BIN)/llvm-objcopy -O binary $< $@

# Only for reading with dtc - the booted tree comes from QEMU itself now.
virt.dtb:
	@$(QEMU) -M virt,dumpdtb=$@ -cpu cortex-a72 -m $(MEM) -display none -serial null

run: kernel.bin
	$(QEMU) $(QFLAGS)

debug: kernel.bin
	$(QEMU) $(QFLAGS) -s -S

regs: kernel.bin
	{ sleep 1; printf 'info registers\nquit\n'; } | $(QEMU) $(MON) \
	  | tr '\r' '\n' | grep -E '^ ?(PC|SP|X[0-9])' | head -6

ADDR ?= 0x40000000
N    ?= 16
FMT  ?= xb

mem: kernel.bin
	{ sleep 1; printf 'xp /$(N)$(FMT) $(ADDR)\nquit\n'; } | $(QEMU) $(MON) \
	  | tr '\r' '\n' | grep -E '^(0x)?[0-9a-f]+:'

# Fill .bss with 0xAA and the 8 bytes just past it with 0xBB, then boot.
# Correct zeroing => all 00 up to __bss_end, guard still BB.
test-bss: kernel.bin
	@S=0x$$($(BIN)/llvm-nm kernel.elf | awk '/ __bss_start$$/{print $$1}'); \
	E=0x$$($(BIN)/llvm-nm kernel.elf | awk '/ __bss_end$$/{print $$1}'); \
	A=$$S; ARGS=""; \
	while [ $$(($$A)) -lt $$(($$E)) ]; do \
	  ARGS="$$ARGS -device loader,addr=$$A,data=0xAAAAAAAAAAAAAAAA,data-len=8"; \
	  A=$$(printf '0x%x' $$(($$A + 8))); \
	done; \
	ARGS="$$ARGS -device loader,addr=$$E,data=0xBBBBBBBBBBBBBBBB,data-len=8"; \
	echo ".bss $$S .. $$E   guard $$E"; \
	{ sleep 1; printf 'xp /%dxb %s\nquit\n' $$(( $$E - $$S + 8 )) $$S; } | \
	  $(QEMU) $(MON) $$ARGS | tr '\r' '\n' | grep -E '^[0-9a-f]+:'

# Feed scripted keystrokes to the guest's serial input, capture output.
#   make feed INPUT='1234'
INPUT ?= abc123
feed: kernel.bin
	@{ sleep 1; printf '$(INPUT)'; sleep 2; } | \
	  $(QEMU) -M virt -cpu cortex-a72 -m $(MEM) -display none -serial stdio -monitor none \
	  -kernel kernel.bin

dis: kernel.elf
	$(BIN)/llvm-objdump -d $<

# Disassembly with the Rust source interleaved, into a file you can scroll.
# Needs debug = true in the release profile, which Cargo.toml sets.
asm: kernel.elf
	$(BIN)/llvm-objdump -d -S --no-show-raw-insn $< > kernel.asm
	@echo "wrote kernel.asm ($$(grep -c . kernel.asm) lines)"

# Unit tests for the logic that has no hardware in it. Built for the host, not
# the bare-metal target, which has no libtest at all. The no_std/no_main/asm
# items are behind cfg(not(test)) so the same crate compiles both ways.
HOST := aarch64-apple-darwin

test:
	cargo test --target $(HOST)

# What the pre-commit hook runs. -D warnings makes every lint fatal. No
# --all-targets: the test target wants libtest, which aarch64-unknown-none has not.
lint:
	cargo clippy --release -- -D warnings
	cargo fmt --check

sections: kernel.elf
	$(BIN)/llvm-readobj --section-headers $<

syms: kernel.elf
	$(BIN)/llvm-nm -n $<

# ulimit -f is in 512-byte blocks; QEMU dies with SIGXFSZ instead of filling the disk.
LOGCAP ?= 400000

trace: kernel.bin
	rm -f /tmp/zheos.log
	ulimit -f $(LOGCAP); $(QEMU) $(QFLAGS) -d int,in_asm -D /tmp/zheos.log

kill:
	-pkill -f qemu-system-aarch64
	@ps -Ao pid,etime,comm | grep qemu-system || echo "no qemu running"

clean:
	cargo clean
	rm -f kernel.elf kernel.bin kernel.asm virt.dtb

.PHONY: kernel.elf virt.dtb run debug regs mem test test-bss feed dis asm lint sections syms trace kill clean
