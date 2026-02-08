Open a Probe and call attach to get a Session (or call Session::auto_attach)
A Session object contains the CombinedCoreState for each core
CombinedCoreState has attach_leon3 function which returns a Core
The Core struct implements CoreMemoryInterface, and therefore implements MemoryInterface

Digilent probe should be supported by grmon, Vivado, and probe-rs.
Need to build an AHBJTAG driver based on a JtagAdapter probe.
* AHB memory access
* Implement MemoryInterface
Need to implement Plug&Play scanner that uses AhbAccess driver
* See grlib.pdf section 5.3
Need to build DSU3 driver based on AhbAccess with info from Plug&Play scanner
* CPU halt, break, step, readback, etc.
* AHB and instruction trace buffers

## AHBJTAG

On Xilinx, uses USER1/USER2 or USER3/USER4 user-defined JTAG DR registers by
internally instantiating the BSCANE2 primitive.

Ultrascale architecture instructions for USERx registers:
  USER1 	000010 	Access user-defined register 1
  USER2 	000011 	Access user-defined register 2
  USER3 	100010 	Access user-defined register 3
  USER4 	100011 	Access user-defined register 4
  IDCODE 	001001 	Enables shifting out of IDCODE
  
Virtex6 architecture instructions for USERx registers:
  USER1  1111000010 Access user-defined register 1
  USER2  1111000011 Access user-defined register 2
  USER3  1111100010 Access user-defined register 3
  USER4  1111100011 Access user-defined register 4
  IDCODE 1111001001 Enables shifting out of ID code

Registers:
* Command/Address (ADATA), 35 bits
* Data (DDATA), 33 bits

ADATA:
- 1-bit read/write (0=read, 1=write)
- 2 bit size (00=byte, 01=half-word, 10=word, 11=reserved)
- 32-bit AHB address

DDATA:
- 1-bit SEQ
  - As written: 1=initiate another transaction at the next (4-bytes incremented) address, 0=NA
  - As read: 
    - For reads: 1=AHB access completed and DR is valid, 0=not complete, need to retry
    - For writes: 1=Data accepted, 0=previous transaction not completed, data dropped, need to retry
- 32-bit AHB data: "For byte and half-word transfers data is aligned according to big-endian
  order where data with address offset 0 data is placed in MSB bits"

"Sequential transfers should not cross a 1 kB boundary and are always word based."

"For both reads and writes, accesses are nominally initiated when the TAP enters the Update-DR state.
However, a few extra TCK cycles may be needed before this information reaches the AMBA clock
domain"

## Overview of Structs and State

`Session`
- Main user handle for a session.
- Owns the `ArchitectureInterface` and `CombinedCoreState`s
- The `core` method returns a temporary `Core` struct by
  - Calling `ArchitectureInterface::attach`, which creates a `LeonCommunicationInterface` and gives it to `CombinedCoreState::attach_leon3`

`ArchitectureInterface::SystemBusInterface`
- Owned by the `Session`
- Owns the `Probe` and `Leon3DebugInterfaceState`
- The `attach` method takes a mutable reference to `CombinedCoreState` and produces a `Core`.
 
`Leon3DebugInterfaceState`
- Session-persistent state for the interface itself (no core-specific state)
- Owned by the `ArchitectureInterface`
- Owns the results of the Plug&Play scan and `Dsu3State`
- Destructured to provide references for the `Leon3CommunicationInterface`

`CombinedCoreState`
- Persistent core-specific state for the session.
- Owns the generic `CoreState` and `Leon3CoreState`
- Has a core id
- `attach_leon3` method produces a `Core`.

`Leon3CoreState`
- Leon3-specific core state, for just a single core

`Leon3CommunicationInterface`
- Temporary struct just holding references to other state/structs
- Notably does not hold any core-specific state, only interface state
- References:
  - Probe
  - Dsu
  - Plug&Play records

`Core`
- Temporary struct, main user interface to controlling a single core
- Has a Boxed reference to the `Leon3` as a `dyn CoreInterface`.

`Leon3`
- Main internal handle for a Leon3 core, implements `CoreInterface` and `MemoryInterface` traits.
- Is a temporary struct, just holds references to state
- Owns a (temporary) `Leon3CommunicationInterface` and reference to the `Leon3CoreState`.
