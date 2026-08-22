BIN    := $(shell rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin
QEMU   := qemu-system-aarch64

MEM      ?= 128M
DTB_ADDR ?= 0x47000000

# QEMU does not hand an ELF kernel a device tree, so load one ourselves.
DTBLOAD := -device loader,file=virt.dtb,addr=$(DTB_ADDR),force-raw=on

QFLAGS := -M virt -cpu cortex-a72 -m $(MEM) -nographic -kernel kernel.elf $(DTBLOAD)
MON    := -M virt -cpu cortex-a72 -m $(MEM) -display none -serial null -monitor stdio $(DTBLOAD)

CARGO_OUT := target/aarch64-unknown-none-softfloat/release/zheos

# cargo does its own change detection, so this always runs and is cheap.
# kernel.elf is a copy so the debugger and QEMU have one stable path.
kernel.elf:
	cargo build --release
	@cp $(CARGO_OUT) $@

# dumpdtb writes the file and exits, so this is ~80ms. Always run, so the blob
# tracks MEM instead of going stale.
virt.dtb:
	@$(QEMU) -M virt,dumpdtb=$@ -cpu cortex-a72 -m $(MEM) -display none -serial null

run: kernel.elf virt.dtb
	$(QEMU) $(QFLAGS)

debug: kernel.elf virt.dtb
	$(QEMU) $(QFLAGS) -s -S

regs: kernel.elf virt.dtb
	{ sleep 1; printf 'info registers\nquit\n'; } | $(QEMU) $(MON) -kernel kernel.elf \
	  | tr '\r' '\n' | grep -E '^ ?(PC|SP|X[0-9])' | head -6

ADDR ?= 0x40000000
N    ?= 16
FMT  ?= xb

mem: kernel.elf virt.dtb
	{ sleep 1; printf 'xp /$(N)$(FMT) $(ADDR)\nquit\n'; } | $(QEMU) $(MON) -kernel kernel.elf \
	  | tr '\r' '\n' | grep -E '^(0x)?[0-9a-f]+:'

# Fill .bss with 0xAA and the 8 bytes just past it with 0xBB, then boot.
# Correct zeroing => all 00 up to __bss_end, guard still BB.
test-bss: kernel.elf virt.dtb
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
	  $(QEMU) $(MON) -kernel kernel.elf $$ARGS | tr '\r' '\n' | grep -E '^[0-9a-f]+:'

# Feed scripted keystrokes to the guest's serial input, capture output.
#   make feed INPUT='1234'
INPUT ?= abc123
feed: kernel.elf virt.dtb
	@{ sleep 1; printf '$(INPUT)'; sleep 2; } | \
	  $(QEMU) -M virt -cpu cortex-a72 -m $(MEM) -display none -serial stdio -monitor none \
	  -kernel kernel.elf $(DTBLOAD)

dis: kernel.elf
	$(BIN)/llvm-objdump -d $<

# Disassembly with the Rust source interleaved, into a file you can scroll.
# Needs debug = true in the release profile, which Cargo.toml sets.
asm: kernel.elf
	$(BIN)/llvm-objdump -d -S --no-show-raw-insn $< > kernel.asm
	@echo "wrote kernel.asm ($$(grep -c . kernel.asm) lines)"

sections: kernel.elf
	$(BIN)/llvm-readobj --section-headers $<

syms: kernel.elf
	$(BIN)/llvm-nm -n $<

# ulimit -f is in 512-byte blocks; QEMU dies with SIGXFSZ instead of filling the disk.
LOGCAP ?= 400000

trace: kernel.elf virt.dtb
	rm -f /tmp/zheos.log
	ulimit -f $(LOGCAP); $(QEMU) $(QFLAGS) -d int,in_asm -D /tmp/zheos.log

kill:
	-pkill -f qemu-system-aarch64
	@ps -Ao pid,etime,comm | grep qemu-system || echo "no qemu running"

clean:
	cargo clean
	rm -f kernel.elf kernel.asm virt.dtb

.PHONY: kernel.elf virt.dtb run debug regs mem test-bss feed dis asm sections syms trace kill clean
