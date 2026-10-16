# Memory Limits

The PlayStation 2 has 32 MB of main RAM, 4 MB of video RAM (VRAM), and 2 MB of sound RAM (SPU2). The game loads scripts, models, maps, textures, and sounds into fixed-size memory buffers. Exceeding a buffer limit crashes the game or corrupts memory.

`dc2tk build` checks every modified file against hardware limits and prints a report:

| Prefix | Meaning |
| --- | --- |
| `memory:` | Buffer usage of the modified asset. |
| `warning:` | Asset is near or over a soft limit. Test the scene in PCSX2. |
| `error:` | Asset exceeds a hard buffer limit. `dc2tk build` writes the ISO for inspection and exits with an error code. |

## Key limits

| Asset Type | Hardware Limit | How to Reduce Size |
| --- | --- | --- |
| **Map script (`.stb`)** | `312,816 bytes` (`0x4C5F0`) | Use function patches (`#pragma override`) instead of copying shared helper libraries. |
| **Disc file index (`DATA.HD4`)** | `327,680 bytes` (`0x50000`, ~150 new top-level files free) | Place new assets inside existing pack archives (`.chr`, `.mpk`, `.snd`) instead of adding top-level disc files. |
| **Hero models & weapons** | Fixed per-slot RAM & VRAM stacks | Keep replacement meshes and textures near the original vertex count and dimensions. |
| **Textures (`.img`)** | Fixed GS VRAM block count per slot | Use power-of-two dimensions and keep 3D textures at or below `256x256`. |
| **Sound banks (`.snd`)** | Fixed SPU2 RAM per port | Trim silence from WAV files or downsample to `22050 Hz` or `11025 Hz`. |
| **Music banks (`BG_*`)** | `546,096 bytes` of sample data | Use shorter loop samples for custom instruments. |
| **System text (`.mes`)** | `53,248` to `80,000 bytes` | Keep strings concise in `system_N.mes` and `sysmes_N.mes`. |

For exact memory addresses and buffer sizes in the PS2 executable, see [DC2 Internals: Memory Layout](../internals/memory.md).
