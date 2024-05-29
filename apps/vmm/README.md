# ArceOS-VMM-Tutorial

Let's build an x86 hypervisor upon ArceOS unikernel!

## Documents

* [RVM-Tutirial wiki](https://github.com/equation314/RVM-Tutorial/wiki) (in Chinese)

## Features

* Lightweight enough, only 1K+ LoC
* Supported guest OS: [NimbOS](https://github.com/equation314/nimbos)
* Guest/host memory isolation with nested paging
* Device emulation:
    + serial port I/O
    + APIC timer
* Currently, only supports single core single vCPU and single guest

## Build Guest OS

```console
$ git submodule init && git submodule update
$ cd apps/vmm_guest/nimbos/kernel
$ make user
$ make GUEST=on
```

## Build Guest BIOS

```console
$ cd apps/vmm_guest/bios
$ make
```

## Build & Run Hypervisor

ArceOS has its own FATFS file system (with "fs" feature enabled) and block driver support, allowing the loading of image files through ArceOS's file system instead of using QEMU's `-device loader,addr=xxx` parameter to place the image file at a specified physical memory address (which is not a good way).

Before running the following command, remember to create a `disk.img` FAT-formatted file system image and copy the compiled outputs from the above two steps into disk.img.

```console
# Path for bios image: apps/vmm_guest/bios/out/rvm-bios.bin
# Path for nimbos image: apps/vmm_guest/nimbos/kernel/target/x86_64/release/nimbos.bin

$ make A=apps/vmm BLK=y run [LOG=warn|info|debug|trace]
......
Booting from ROM..
Initialize IDT & GDT...

       d8888                            .d88888b.   .d8888b.
      d88888                           d88P" "Y88b d88P  Y88b
     d88P888                           888     888 Y88b.
    d88P 888 888d888  .d8888b  .d88b.  888     888  "Y888b.
   d88P  888 888P"   d88P"    d8P  Y8b 888     888     "Y88b.
  d88P   888 888     888      88888888 888     888       "888
 d8888888888 888     Y88b.    Y8b.     Y88b. .d88P Y88b  d88P
d88P     888 888      "Y8888P  "Y8888   "Y88888P"   "Y8888P"

arch = x86_64
platform = x86_64-qemu-q35
target = x86_64-unknown-none
smp = 1
build_mode = release
log_level = warn

Starting virtualization...
Running guest...

NN   NN  iii               bb        OOOOO    SSSSS
NNN  NN       mm mm mmmm   bb       OO   OO  SS
NN N NN  iii  mmm  mm  mm  bbbbbb   OO   OO   SSSSS
NN  NNN  iii  mmm  mm  mm  bb   bb  OO   OO       SS
NN   NN  iii  mmm  mm  mm  bbbbbb    OOOO0    SSSSS
              ___    ____    ___    ___
             |__ \  / __ \  |__ \  |__ \
             __/ / / / / /  __/ /  __/ /
            / __/ / /_/ /  / __/  / __/
           /____/ \____/  /____/ /____/

arch = x86_64
platform = rvm-guest-x86_64
build_mode = release
log_level = warn

Initializing kernel heap at: [0xffffff800028ed00, 0xffffff800068ed00)
Initializing IDT...
Loading GDT for CPU 0...
Initializing frame allocator at: [PA:0x68f000, PA:0x1000000)
Mapping .text: [0xffffff8000200000, 0xffffff800021b000)
Mapping .rodata: [0xffffff800021b000, 0xffffff8000220000)
Mapping .data: [0xffffff8000220000, 0xffffff800028a000)
Mapping .bss: [0xffffff800028e000, 0xffffff800068f000)
Mapping boot stack: [0xffffff800028a000, 0xffffff800028e000)
Mapping physical memory: [0xffffff800068f000, 0xffffff8001000000)
Mapping MMIO: [0xffffff80fec00000, 0xffffff80fec01000)
Mapping MMIO: [0xffffff80fed00000, 0xffffff80fed01000)
Mapping MMIO: [0xffffff80fee00000, 0xffffff80fee01000)
Initializing drivers...
Initializing Local APIC...
Initializing HPET...
HPET: 100.000000 MHz, 64-bit, 3 timers
Calibrated TSC frequency: 2993.403 MHz
Calibrated LAPIC frequency: 1000.235 MHz
Initializing task manager...
/**** APPS ****
cyclictest
exit
fantastic_text
forktest
forktest2
forktest_simple
forktest_simple_c
forktree
hello_c
hello_world
matrix
sleep
sleep_simple
stack_overflow
thread_simple
user_shell
usertests
yield
**************/
Running tasks...
test kernel task: pid = TaskId(2), arg = 0xdead
test kernel task: pid = TaskId(3), arg = 0xbeef
Rust user shell
>>
```
