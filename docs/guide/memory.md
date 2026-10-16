# Memory Limits

The PlayStation 2 has 32 MB of main RAM, 4 MB of video RAM (VRAM), and 2 MB of sound RAM (SPU2). The game loads scripts, models, maps, textures, and sounds into fixed-size memory buffers. If a modded file grows larger than its buffer, the game will crash or corrupt memory.

To protect your mod, `dc2tk build` automatically checks every modified file against the game's hardware budgets and prints a report:

- `memory:` — Shows how much of a buffer your modified asset uses.
- `warning:` — The asset is close to or slightly over a soft limit; test the scene in PCSX2.
- `error:` — The asset exceeds a hard buffer limit. `dc2tk build` still writes the ISO so you can inspect it, but exits with an error code.

## Key limits to keep in mind

| Asset Type | Hardware Limit | Tip if You Hit the Limit |
| --- | --- | --- |
| **Map script (`.stb`)** | `312,816 bytes` (`0x4C5F0`) | Use function patches (`#pragma override`) instead of duplicating large helper libraries. |
| **Disc file index (`DATA.HD4`)** | `327,680 bytes` (`0x50000`, ~150 new top-level files free) | Put new assets inside existing pack archives (`.chr`, `.mpk`, `.snd`) instead of adding top-level disc files. |
| **Hero models & weapons** | Tight per-slot RAM & VRAM stacks | Keep replacement weapon and outfit meshes near the vertex and texture size of the original model. |
| **Textures (`.img`)** | Fixed GS VRAM block count per slot | Keep texture dimensions power-of-two and avoid making 3D textures larger than `256x256` unless needed. |
| **Sound banks (`.snd`)** | Fixed SPU2 RAM per port | Trim silence from WAV files or downsample from `44100 Hz` to `22050 Hz` or `11025 Hz`. |
| **Music banks (`BG_*`)** | `546,096 bytes` of sample data | Use shorter loop samples for custom instruments. |
| **System text (`.mes`)** | `53,248` to `80,000 bytes` | Keep edits concise in `system_N.mes` and `sysmes_N.mes`. |

For the exact memory addresses and buffer sizes in the PS2 executable, see [DC2 Internals: Memory Layout](../internals/memory.md).
