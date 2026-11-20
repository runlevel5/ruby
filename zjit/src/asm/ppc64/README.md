# PPC64

This module is responsible for encoding ZJIT operands into an appropriate 64-bit PowerPC encoding. It targets little-endian Linux (ppc64le) with the ELFv2 ABI, and only uses instructions available on POWER8 (Power ISA 2.07).

## Helpful links

* [Power ISA Version 2.07 B](https://openpowerfoundation.org/specifications/isa/) Official documentation
* [64-Bit ELF V2 ABI Specification](https://openpowerfoundation.org/specifications/64bitelfabi/) Calling convention and register roles
