/* LPC1114/1115: 64 KB flash, 8 KB SRAM.
   The top 32 bytes of RAM are reserved for the boot ROM's IAP calls. */
MEMORY
{
  FLASH : ORIGIN = 0x00000000, LENGTH = 64K
  RAM   : ORIGIN = 0x10000000, LENGTH = 8K - 32
}
