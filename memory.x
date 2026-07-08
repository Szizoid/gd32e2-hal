/* GD32E230K8U6 memory map.
 *
 * Values taken from the colleague's verified Keil project:
 *   IROM1  start 0x08000000  size 0xC000  -> 48 KiB
 *   IRAM1  start 0x20000000  size 0x2000  ->  8 KiB
 *
 * NOTE: the GD32E230K8 die physically has 64 KiB of flash. Keil was set to 48 KiB,
 * which is safe (it just leaves the top 16 KiB unused). If you ever need the full
 * size, change FLASH LENGTH to 64K. 48K is the proven-good value, keep it for now.
 */
MEMORY
{
    FLASH : ORIGIN = 0x08000000, LENGTH = 48K
    RAM   : ORIGIN = 0x20000000, LENGTH = 8K
}
