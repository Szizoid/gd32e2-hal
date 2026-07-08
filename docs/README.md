# Reference documents

Drop the GD32E230 PDFs into this folder so both you and the LLM can consult them.
They are git-ignored (large binaries), but they stay on disk and Claude Code can
read them directly.

Download from the official site (English portal):
https://www.gd32mcu.com/en/download/0?kw=GD32E2

Get these two and save them here with these exact names (CLAUDE.md refers to them):

1. **GD32E23x User Manual**  ->  `docs/GD32E23x_User_Manual.pdf`
   The register reference. This is the one you will live in: every RCU / GPIO /
   ADC / USART register and bit field is described here.

2. **GD32E230xx Datasheet**  ->  `docs/GD32E230xx_Datasheet.pdf`
   Direct link (Rev 2.6):
   https://www.gd32mcu.com/data/documents/datasheet/GD32E230xx_Datasheet_Rev2.6.pdf
   Use it for the pinout (QFN32 / K-package), ADC channel-to-pin mapping, and
   electrical limits (max ADC clock, VDDA range).

Optional but handy:
- **AN074 GD32E23x Hardware Development Guide** — confirms the USART0 bootloader is
  on PA9/PA10 and covers power/boot wiring.
