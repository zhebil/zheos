BIN    := $(shell rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin
QEMU   := qemu-system-aarch64
QFLAGS := -M virt -cpu cortex-a72 -m 128M -nographic -kernel kernel.elf
MON    := -M virt -cpu cortex-a72 -m 128M -display none -serial null -monitor stdio

CARGO_OUT := target/aarch64-unknown-none/release/zheos

# cargo does its own change detection, so this always runs and is cheap.
# kernel.elf is a copy so the debugger and QEMU have one stable path.
# Debug printing is opt-in. Add PRINT=1 to any target: make run PRINT=1
CARGO_FLAGS := $(if $(PRINT),--features debug-print)

kernel.elf:
	cargo build --release $(CARGO_FLAGS)
	@cp $(CARGO_OUT) $@

run: kernel.elf
	$(QEMU) $(QFLAGS)

debug: kernel.elf
	$(QEMU) $(QFLAGS) -s -S

regs: kernel.elf
	{ sleep 1; printf 'info registers\nquit\n'; } | $(QEMU) $(MON) -kernel kernel.elf \
	  | tr '\r' '\n' | grep -E '^ ?(PC|SP|X[0-9])' | head -6

ADDR ?= 0x40000000
N    ?= 16
FMT  ?= xb

mem: kernel.elf
	{ sleep 1; printf 'xp /$(N)$(FMT) $(ADDR)\nquit\n'; } | $(QEMU) $(MON) -kernel kernel.elf \
	  | tr '\r' '\n' | grep -E '^(0x)?[0-9a-f]+:'

# Fill .bss with 0xAA and the 8 bytes just past it with 0xBB, then boot.
# Correct zeroing => all 00 up to __bss_end, guard still BB.
test-bss: kernel.elf
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
feed: kernel.elf
	@{ sleep 1; printf '$(INPUT)'; sleep 2; } | \
	  $(QEMU) -M virt -cpu cortex-a72 -m 128M -display none -serial stdio -monitor none \
	  -kernel kernel.elf

dis: kernel.elf
	$(BIN)/llvm-objdump -d $<

# Disassembly with the Rust source interleaved, into a file you can scroll.
# Needs debug = true in the release profile, which Cargo.toml sets.
asm: kernel.elf
	$(BIN)/llvm-objdump -d -S --no-show-raw-insn $< > kernel.asm
	@echo "wrote kernel.asm ($$(grep -c . kernel.asm) lines)"

# .text with and without the debug-print feature.
sizes:
	@for f in "" "--features debug-print"; do \
	  cargo build --release $$f >/dev/null 2>&1; \
	  printf '%-24s .text = %s bytes\n' "$${f:-(no prints)}" \
	    "$$($(BIN)/llvm-readobj --section-headers $(CARGO_OUT) | grep -A9 'Name: .text' | awk '/Size:/{print $$2; exit}')"; \
	done

sections: kernel.elf
	$(BIN)/llvm-readobj --section-headers $<

syms: kernel.elf
	$(BIN)/llvm-nm -n $<

# ulimit -f is in 512-byte blocks; QEMU dies with SIGXFSZ instead of filling the disk.
LOGCAP ?= 400000

trace: kernel.elf
	rm -f /tmp/zheos.log
	ulimit -f $(LOGCAP); $(QEMU) $(QFLAGS) -d int,in_asm -D /tmp/zheos.log

kill:
	-pkill -f qemu-system-aarch64
	@ps -Ao pid,etime,comm | grep qemu-system || echo "no qemu running"

clean:
	cargo clean
	rm -f kernel.elf kernel.asm

.PHONY: kernel.elf run debug regs mem test-bss feed dis asm sizes sections syms trace kill clean
